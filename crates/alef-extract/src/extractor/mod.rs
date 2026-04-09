mod defaults;
mod functions;
mod helpers;
mod reexports;
mod types;

use std::path::{Path, PathBuf};

use ahash::AHashMap;
use alef_core::ir::{ApiSurface, MethodDef, TypeDef, TypeRef};
use anyhow::{Context, Result};

use crate::type_resolver;

use self::functions::{detect_receiver, extract_function, extract_impl_block, extract_params, resolve_return_type};
use self::helpers::{build_rust_path, collect_reexport_map, extract_doc_comments, is_pub, is_thiserror_enum};
use self::reexports::{extract_module, resolve_use_tree};
use self::types::{extract_enum, extract_error_enum, extract_struct};

/// Extract the public API surface from Rust source files.
///
/// `sources` should be the root source files (e.g., `lib.rs`) of the crate.
/// Submodules referenced via `mod` declarations are resolved and extracted recursively.
/// `workspace_root` enables resolution of `pub use` re-exports from workspace sibling crates.
pub fn extract(
    sources: &[&Path],
    crate_name: &str,
    version: &str,
    workspace_root: Option<&Path>,
) -> Result<ApiSurface> {
    let mut surface = ApiSurface {
        crate_name: crate_name.to_string(),
        version: version.to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let mut visited = Vec::<PathBuf>::new();

    for source in sources {
        let canonical = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        visited.push(canonical);

        let content = std::fs::read_to_string(source)
            .with_context(|| format!("Failed to read source file: {}", source.display()))?;
        let file =
            syn::parse_file(&content).with_context(|| format!("Failed to parse source file: {}", source.display()))?;
        extract_items(
            &file.items,
            source,
            crate_name,
            "",
            &mut surface,
            workspace_root,
            &mut visited,
        )?;
    }

    // Post-processing: resolve newtype wrappers.
    // Single-field tuple structs like `pub struct Foo(String)` are detected by having
    // exactly one field named `_0`. We replace all `TypeRef::Named("Foo")` references
    // with the inner type, then remove the newtype TypeDefs from the surface.
    resolve_newtypes(&mut surface);

    // After newtype resolution, any remaining types with `_0` fields are tuple structs
    // that weren't resolved (because they have methods or complex inner types).
    // Make these opaque since their inner field is private and can't be accessed.
    for typ in &mut surface.types {
        if typ.fields.len() == 1 && typ.fields[0].name == "_0" {
            typ.fields.clear();
            typ.is_opaque = true;
        }
    }

    // Mark types that appear as function return types.
    // These may use a different DTO style (e.g., TypedDict in Python).
    let return_type_names: ahash::AHashSet<String> = surface
        .functions
        .iter()
        .filter_map(|f| match &f.return_type {
            TypeRef::Named(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    for typ in &mut surface.types {
        if return_type_names.contains(&typ.name) {
            typ.is_return_type = true;
        }
    }

    Ok(surface)
}

/// Returns `true` if the type is a simple leaf type (primitive, String, Bytes, Path, etc.)
/// rather than a complex Named, collection, or Optional type.
fn is_simple_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Primitive(_)
            | TypeRef::String
            | TypeRef::Bytes
            | TypeRef::Path
            | TypeRef::Unit
            | TypeRef::Duration
            | TypeRef::Json
    )
}

/// Resolve newtype wrappers in the API surface.
///
/// Single-field tuple structs (`pub struct Foo(T)`) are identified by having exactly
/// one field named `_0`, no methods, and a simple inner type (primitive, String, etc.).
/// For each such newtype, all `TypeRef::Named("Foo")` references throughout the surface
/// are replaced with the inner type `T`, and the newtype TypeDef itself is removed.
/// This makes newtypes fully transparent to backends.
///
/// Tuple structs wrapping complex Named types (e.g., builders) are kept as-is.
fn resolve_newtypes(surface: &mut ApiSurface) {
    // Build a map of newtype name → inner TypeRef.
    let newtype_map: AHashMap<String, TypeRef> = surface
        .types
        .iter()
        .filter(|t| {
            t.fields.len() == 1 && t.fields[0].name == "_0" && t.methods.is_empty() && is_simple_type(&t.fields[0].ty)
        })
        .map(|t| (t.name.clone(), t.fields[0].ty.clone()))
        .collect();

    if newtype_map.is_empty() {
        return;
    }

    // Remove newtype TypeDefs from the surface.
    surface.types.retain(|t| !newtype_map.contains_key(&t.name));

    // Walk all TypeRefs in the surface and replace Named references to newtypes.
    for typ in &mut surface.types {
        for field in &mut typ.fields {
            resolve_typeref(&newtype_map, &mut field.ty);
        }
        for method in &mut typ.methods {
            for param in &mut method.params {
                resolve_typeref(&newtype_map, &mut param.ty);
            }
            resolve_typeref(&newtype_map, &mut method.return_type);
        }
    }
    for func in &mut surface.functions {
        for param in &mut func.params {
            resolve_typeref(&newtype_map, &mut param.ty);
        }
        resolve_typeref(&newtype_map, &mut func.return_type);
    }
    for enum_def in &mut surface.enums {
        for variant in &mut enum_def.variants {
            for field in &mut variant.fields {
                resolve_typeref(&newtype_map, &mut field.ty);
            }
        }
    }
}

/// Recursively replace `TypeRef::Named(name)` with the newtype's inner type.
fn resolve_typeref(newtype_map: &AHashMap<String, TypeRef>, ty: &mut TypeRef) {
    match ty {
        TypeRef::Named(name) => {
            if let Some(inner) = newtype_map.get(name.as_str()) {
                *ty = inner.clone();
            }
        }
        TypeRef::Optional(inner) => resolve_typeref(newtype_map, inner),
        TypeRef::Vec(inner) => resolve_typeref(newtype_map, inner),
        TypeRef::Map(k, v) => {
            resolve_typeref(newtype_map, k);
            resolve_typeref(newtype_map, v);
        }
        _ => {}
    }
}

/// Extract items from a parsed syn file or module.
fn extract_items(
    items: &[syn::Item],
    source_path: &Path,
    crate_name: &str,
    module_path: &str,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    // Collect pub use re-exports at this level (for path flattening).
    // When a `pub use submod::*` or `pub use submod::TypeName` is found,
    // items defined in that submodule should get a shorter path (this level's path).
    let reexport_map = collect_reexport_map(items);

    // First pass: collect all structs/enums (no impl blocks yet)
    for item in items {
        match item {
            syn::Item::Struct(item_struct) => {
                if is_pub(&item_struct.vis) {
                    if let Some(td) = extract_struct(item_struct, crate_name, module_path) {
                        surface.types.push(td);
                    }
                }
            }
            syn::Item::Enum(item_enum) => {
                if is_pub(&item_enum.vis) {
                    if is_thiserror_enum(&item_enum.attrs) {
                        if let Some(ed) = extract_error_enum(item_enum, crate_name, module_path) {
                            surface.errors.push(ed);
                        }
                    } else if let Some(ed) = extract_enum(item_enum, crate_name, module_path) {
                        surface.enums.push(ed);
                    }
                }
            }
            syn::Item::Fn(item_fn) => {
                if is_pub(&item_fn.vis) {
                    if let Some(fd) = extract_function(item_fn, crate_name, module_path) {
                        surface.functions.push(fd);
                    }
                }
            }
            syn::Item::Type(item_type) => {
                if is_pub(&item_type.vis) && item_type.generics.params.is_empty() {
                    // Type alias: pub type Foo = Bar;
                    // Extract as a TypeDef with the aliased type
                    let name = item_type.ident.to_string();
                    let _ty = type_resolver::resolve_type(&item_type.ty);
                    let rust_path = build_rust_path(crate_name, module_path, &name);
                    let doc = extract_doc_comments(&item_type.attrs);
                    surface.types.push(TypeDef {
                        name,
                        rust_path,
                        fields: vec![],
                        methods: vec![],
                        is_opaque: true, // type aliases are opaque (no fields)
                        is_clone: false,
                        is_trait: false,
                        has_default: false,
                        has_stripped_cfg_fields: false,
                        is_return_type: false,
                        doc,
                        cfg: None,
                    });
                }
            }
            syn::Item::Trait(item_trait) => {
                if is_pub(&item_trait.vis) && item_trait.generics.params.is_empty() {
                    let name = item_trait.ident.to_string();
                    let rust_path = build_rust_path(crate_name, module_path, &name);
                    let doc = extract_doc_comments(&item_trait.attrs);

                    // Extract trait methods
                    let methods: Vec<MethodDef> = item_trait
                        .items
                        .iter()
                        .filter_map(|item| {
                            if let syn::TraitItem::Fn(method) = item {
                                let method_name = method.sig.ident.to_string();
                                let method_doc = extract_doc_comments(&method.attrs);
                                let mut is_async = method.sig.asyncness.is_some();
                                let (mut return_type, error_type, returns_ref) =
                                    resolve_return_type(&method.sig.output);

                                // Check for BoxFuture async pattern
                                if !is_async {
                                    if let Some(inner) = functions::unwrap_future_return(&method.sig.output) {
                                        is_async = true;
                                        return_type = inner;
                                    }
                                }

                                // Skip generic methods
                                if !method.sig.generics.params.is_empty() {
                                    return None;
                                }

                                let (receiver, is_static) = detect_receiver(&method.sig.inputs);
                                let params = extract_params(&method.sig.inputs);

                                Some(MethodDef {
                                    name: method_name,
                                    params,
                                    return_type,
                                    is_async,
                                    is_static,
                                    error_type,
                                    doc: method_doc,
                                    receiver,
                                    sanitized: false,
                                    trait_source: None,
                                    returns_ref,
                                })
                            } else {
                                None
                            }
                        })
                        .collect();

                    surface.types.push(TypeDef {
                        name,
                        rust_path,
                        fields: vec![],
                        methods,
                        is_opaque: true,
                        is_clone: false,
                        is_trait: true,
                        has_default: false,
                        has_stripped_cfg_fields: false,
                        is_return_type: false,
                        doc,
                        cfg: None,
                    });
                }
            }
            syn::Item::Mod(item_mod) => {
                if is_pub(&item_mod.vis) {
                    extract_module(
                        item_mod,
                        source_path,
                        crate_name,
                        module_path,
                        &reexport_map,
                        surface,
                        workspace_root,
                        visited,
                    )?;
                }
            }
            syn::Item::Use(item_use) if is_pub(&item_use.vis) => {
                resolve_use_tree(&item_use.tree, crate_name, surface, workspace_root, visited)?;
            }
            _ => {}
        }
    }

    // Build type name to index map for O(1) lookup
    let type_index: AHashMap<String, usize> = surface
        .types
        .iter()
        .enumerate()
        .map(|(idx, typ)| (typ.name.clone(), idx))
        .collect();

    // Second pass: process impl blocks using the index
    for item in items {
        if let syn::Item::Impl(item_impl) = item {
            extract_impl_block(item_impl, crate_name, module_path, surface, &type_index);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
