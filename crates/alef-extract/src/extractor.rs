use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ahash::AHashMap;
use alef_core::ir::{
    ApiSurface, DefaultValue, EnumDef, EnumVariant, ErrorDef, ErrorVariant, FieldDef, FunctionDef, MethodDef, ParamDef,
    ReceiverKind, TypeDef, TypeRef,
};
use anyhow::{Context, Result};

use crate::type_resolver;

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
                                    if let Some(inner) = unwrap_future_return(&method.sig.output) {
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

/// Represents what a `pub use` re-exports from a specific module.
#[derive(Debug)]
enum ReexportKind {
    /// `pub use module::*` — re-export everything
    Glob,
    /// `pub use module::{A, B}` — re-export specific names
    Names(HashSet<String>),
}

/// Collect pub use re-exports at the current module level, grouped by source module.
///
/// Returns a map from module name to the kind of re-export (glob or named).
/// Only tracks `pub use <ident>::...` where `<ident>` is not `self`/`super`/`crate`
/// (those are internal references handled elsewhere).
fn collect_reexport_map(items: &[syn::Item]) -> AHashMap<String, ReexportKind> {
    let mut map: AHashMap<String, ReexportKind> = AHashMap::new();
    for item in items {
        if let syn::Item::Use(item_use) = item {
            if is_pub(&item_use.vis) {
                collect_reexport_from_tree(&item_use.tree, &mut map);
            }
        }
    }
    map
}

/// Walk a use tree and populate the reexport map.
fn collect_reexport_from_tree(tree: &syn::UseTree, map: &mut AHashMap<String, ReexportKind>) {
    if let syn::UseTree::Path(use_path) = tree {
        let root_ident = use_path.ident.to_string();
        // Skip self/super/crate — those are internal
        if root_ident == "self" || root_ident == "super" || root_ident == "crate" {
            return;
        }
        collect_reexport_leaves(&root_ident, &use_path.tree, map);
    } else if let syn::UseTree::Group(group) = tree {
        for item in &group.items {
            collect_reexport_from_tree(item, map);
        }
    }
}

/// Collect leaves from a use subtree rooted at a known module name.
fn collect_reexport_leaves(module: &str, tree: &syn::UseTree, map: &mut AHashMap<String, ReexportKind>) {
    match tree {
        syn::UseTree::Glob(_) => {
            map.insert(module.to_string(), ReexportKind::Glob);
        }
        syn::UseTree::Name(use_name) => {
            let name = use_name.ident.to_string();
            match map.get_mut(module) {
                Some(ReexportKind::Glob) => {} // glob already covers everything
                Some(ReexportKind::Names(names)) => {
                    names.insert(name);
                }
                None => {
                    let mut names = HashSet::new();
                    names.insert(name);
                    map.insert(module.to_string(), ReexportKind::Names(names));
                }
            }
        }
        syn::UseTree::Rename(use_rename) => {
            let name = use_rename.rename.to_string();
            match map.get_mut(module) {
                Some(ReexportKind::Glob) => {}
                Some(ReexportKind::Names(names)) => {
                    names.insert(name);
                }
                None => {
                    let mut names = HashSet::new();
                    names.insert(name);
                    map.insert(module.to_string(), ReexportKind::Names(names));
                }
            }
        }
        syn::UseTree::Path(use_path) => {
            // Deeper path like `pub use module::submod::Thing` — treat as coming from `module`
            collect_reexport_leaves(module, &use_path.tree, map);
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_reexport_leaves(module, item, map);
            }
        }
    }
}

/// Build the fully qualified rust_path for an item, taking into account
/// the accumulated module path.
fn build_rust_path(crate_name: &str, module_path: &str, name: &str) -> String {
    if module_path.is_empty() {
        format!("{crate_name}::{name}")
    } else {
        format!("{crate_name}::{module_path}::{name}")
    }
}

/// Extract the condition string from a `#[cfg(...)]` attribute, if present.
/// Check if any attribute is a `#[cfg(...)]` — indicates feature-gated code.
fn has_cfg_attribute(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("cfg"))
}

fn extract_cfg_condition(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("cfg") {
            // Get the token stream inside cfg(...)
            if let Ok(tokens) = attr.meta.require_list() {
                return Some(tokens.tokens.to_string());
            }
        }
    }
    None
}

/// Extract a public struct into a `TypeDef`.
/// Returns `None` for generic structs — they can't be directly exposed to FFI.
fn extract_struct(item: &syn::ItemStruct, crate_name: &str, module_path: &str) -> Option<TypeDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let cfg = extract_cfg_condition(&item.attrs);
    let name = item.ident.to_string();

    // Detect single-field tuple structs (newtype wrappers like `pub struct Foo(String)`).
    // These get a single field named `_0` so the post-processing pass in `extract()`
    // can identify them and resolve `TypeRef::Named("Foo")` → inner type transparently.
    let mut fields = match &item.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .filter(|f| is_pub(&f.vis))
            .map(extract_field)
            .collect(),
        syn::Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
            let field = &unnamed.unnamed[0];
            let resolved = type_resolver::resolve_type(&field.ty);
            let (ty, optional) = unwrap_optional(resolved);
            vec![FieldDef {
                name: "_0".to_string(),
                ty,
                optional,
                default: None,
                doc: String::new(),
                sanitized: false,
                is_boxed: syn_type_is_boxed(&field.ty),
                type_rust_path: extract_field_type_rust_path(&field.ty),
                cfg: None,
                typed_default: None,
            }]
        }
        _ => vec![],
    };

    let is_clone = has_derive(item.attrs.as_slice(), "Clone");
    let has_default = has_derive(item.attrs.as_slice(), "Default");
    let doc = extract_doc_comments(&item.attrs);
    let is_opaque = fields.is_empty();
    let rust_path = build_rust_path(crate_name, module_path, &name);

    // #[derive(Default)] — all fields get DefaultValue::Empty (type's own Default)
    if has_default {
        for field in &mut fields {
            field.typed_default = Some(DefaultValue::Empty);
        }
    }

    let has_stripped_cfg_fields = fields.iter().any(|f| f.cfg.is_some());

    Some(TypeDef {
        rust_path,
        name,
        fields,
        methods: vec![],
        is_opaque,
        is_clone,
        is_trait: false,
        has_default,
        has_stripped_cfg_fields,
        is_return_type: false,
        doc,
        cfg,
    })
}

/// Extract a struct field into a `FieldDef`.
fn extract_field(field: &syn::Field) -> FieldDef {
    let name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let doc = extract_doc_comments(&field.attrs);
    let cfg = extract_cfg_condition(&field.attrs);

    let is_boxed = syn_type_is_boxed(&field.ty);
    let type_rust_path = extract_field_type_rust_path(&field.ty);

    let resolved = type_resolver::resolve_type(&field.ty);
    let (ty, optional) = unwrap_optional(resolved);

    FieldDef {
        name,
        ty,
        optional,
        default: None,
        doc,
        sanitized: false,
        is_boxed,
        type_rust_path,
        cfg,
        typed_default: None,
    }
}

/// Check if a syn::Type is `Box<T>` or `Option<Box<T>>`.
fn syn_type_is_boxed(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();
            if ident == "Box" {
                // Direct Box<T> — but not Box<dyn Trait> (those are opaque)
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            // Box<dyn Trait> is not a "boxed field" in our sense
                            if matches!(inner, syn::Type::TraitObject(_)) {
                                return false;
                            }
                            return true;
                        }
                    }
                }
            } else if ident == "Option" {
                // Option<Box<T>>
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            return syn_type_is_boxed(inner);
                        }
                    }
                }
            }
        }
    }
    false
}

/// Extract the fully qualified Rust path for a field's type when it uses a multi-segment
/// path (e.g., `crate::types::OutputFormat` → `types::OutputFormat`).
/// Returns `None` for simple single-segment types like `OutputFormat` or primitives.
fn extract_field_type_rust_path(ty: &syn::Type) -> Option<String> {
    // Unwrap Option<T> to look at inner type
    let inner_ty = if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    args.args.iter().find_map(|arg| {
                        if let syn::GenericArgument::Type(inner) = arg {
                            Some(inner)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let check_ty = inner_ty.unwrap_or(ty);

    // Unwrap Box<T> to look at inner type
    let check_ty = if let syn::Type::Path(type_path) = check_ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Box" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    args.args
                        .iter()
                        .find_map(|arg| {
                            if let syn::GenericArgument::Type(inner) = arg {
                                Some(inner)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(check_ty)
                } else {
                    check_ty
                }
            } else {
                check_ty
            }
        } else {
            check_ty
        }
    } else {
        check_ty
    };

    // Now check if the type has a multi-segment path
    if let syn::Type::Path(type_path) = check_ty {
        if type_path.path.segments.len() >= 2 {
            let first_segment = type_path.path.segments[0].ident.to_string();
            // Skip relative paths (`crate::...`, `super::...`) — these can't be resolved
            // to absolute paths without full module context and would produce invalid
            // paths like `kreuzberg::super::super::pdf::PdfConfig` in codegen.
            if first_segment == "crate" || first_segment == "super" {
                return None;
            }
            let segments: Vec<String> = type_path.path.segments.iter().map(|s| s.ident.to_string()).collect();
            return Some(segments.join("::"));
        }
    }
    None
}

/// If the resolved type is `TypeRef::Optional(inner)`, unwrap it and mark as optional.
fn unwrap_optional(ty: TypeRef) -> (TypeRef, bool) {
    match ty {
        TypeRef::Optional(inner) => (*inner, true),
        other => (other, false),
    }
}

/// Extract a public enum into an `EnumDef`.
/// Returns `None` for generic enums — they can't be directly exposed to FFI.
fn extract_enum(item: &syn::ItemEnum, crate_name: &str, module_path: &str) -> Option<EnumDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let cfg = extract_cfg_condition(&item.attrs);
    let name = item.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);

    let variants = item
        .variants
        .iter()
        .map(|v| {
            let variant_fields = match &v.fields {
                syn::Fields::Named(named) => named.named.iter().map(extract_field).collect(),
                syn::Fields::Unnamed(unnamed) => unnamed
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let ty = type_resolver::resolve_type(&f.ty);
                        let optional = type_resolver::is_option_type(&f.ty).is_some();
                        FieldDef {
                            name: format!("_{i}"),
                            ty,
                            optional,
                            default: None,
                            doc: extract_doc_comments(&f.attrs),
                            sanitized: false,
                            is_boxed: syn_type_is_boxed(&f.ty),
                            type_rust_path: extract_field_type_rust_path(&f.ty),
                            cfg: None,
                            typed_default: None,
                        }
                    })
                    .collect(),
                syn::Fields::Unit => vec![],
            };
            EnumVariant {
                name: v.ident.to_string(),
                fields: variant_fields,
                doc: extract_doc_comments(&v.attrs),
                is_default: v.attrs.iter().any(|a| a.path().is_ident("default")),
            }
        })
        .collect();

    let rust_path = build_rust_path(crate_name, module_path, &name);

    Some(EnumDef {
        rust_path,
        name,
        variants,
        doc,
        cfg,
    })
}

/// Check if an enum derives `thiserror::Error` (or just `Error` from a `use thiserror::Error`).
fn is_thiserror_enum(attrs: &[syn::Attribute]) -> bool {
    has_derive(attrs, "Error") || has_derive_path(attrs, &["thiserror", "Error"])
}

/// Check if a `#[derive(...)]` attribute contains a specific multi-segment derive path.
/// e.g. `has_derive_path(attrs, &["thiserror", "Error"])` matches `#[derive(thiserror::Error)]`.
fn has_derive_path(attrs: &[syn::Attribute], segments: &[&str]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let Ok(nested) =
                attr.parse_args_with(syn::punctuated::Punctuated::<syn::Path, syn::token::Comma>::parse_terminated)
            {
                for path in &nested {
                    if path.segments.len() == segments.len()
                        && path
                            .segments
                            .iter()
                            .zip(segments.iter())
                            .all(|(seg, expected)| seg.ident == expected)
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Extract the `#[error("...")]` message template from a variant's attributes.
fn extract_error_message_template(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("error") {
            // Parse as #[error("template string")]
            if let Ok(lit) = attr.parse_args::<syn::LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Check if a field has a specific attribute (e.g. `#[source]`, `#[from]`).
fn has_field_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|a| a.path().is_ident(name))
}

/// Extract a `#[derive(thiserror::Error)]` enum into an `ErrorDef`.
/// Returns `None` for generic enums.
fn extract_error_enum(item: &syn::ItemEnum, crate_name: &str, module_path: &str) -> Option<ErrorDef> {
    if !item.generics.params.is_empty() {
        return None;
    }
    let name = item.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);

    let variants = item
        .variants
        .iter()
        .filter(|v| !has_cfg_attribute(&v.attrs)) // Skip cfg-gated variants
        .map(|v| {
            let message_template = extract_error_message_template(&v.attrs);
            let variant_doc = extract_doc_comments(&v.attrs);

            let (fields, has_source, has_from, is_unit) = match &v.fields {
                syn::Fields::Named(named) => {
                    let mut source = false;
                    let mut from = false;
                    let fields: Vec<FieldDef> = named
                        .named
                        .iter()
                        .map(|f| {
                            if has_field_attr(&f.attrs, "source") {
                                source = true;
                            }
                            if has_field_attr(&f.attrs, "from") {
                                from = true;
                                source = true; // #[from] implies source
                            }
                            extract_field(f)
                        })
                        .collect();
                    (fields, source, from, false)
                }
                syn::Fields::Unnamed(unnamed) => {
                    let mut source = false;
                    let mut from = false;
                    let fields: Vec<FieldDef> = unnamed
                        .unnamed
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            if has_field_attr(&f.attrs, "source") {
                                source = true;
                            }
                            if has_field_attr(&f.attrs, "from") {
                                from = true;
                                source = true;
                            }
                            let ty = type_resolver::resolve_type(&f.ty);
                            let optional = type_resolver::is_option_type(&f.ty).is_some();
                            FieldDef {
                                name: format!("_{i}"),
                                ty,
                                optional,
                                default: None,
                                doc: extract_doc_comments(&f.attrs),
                                sanitized: false,
                                is_boxed: syn_type_is_boxed(&f.ty),
                                type_rust_path: extract_field_type_rust_path(&f.ty),
                                cfg: None,
                                typed_default: None,
                            }
                        })
                        .collect();
                    (fields, source, from, false)
                }
                syn::Fields::Unit => (vec![], false, false, true),
            };

            ErrorVariant {
                name: v.ident.to_string(),
                message_template,
                fields,
                has_source,
                has_from,
                is_unit,
                doc: variant_doc,
            }
        })
        .collect();

    let rust_path = build_rust_path(crate_name, module_path, &name);

    Some(ErrorDef {
        name,
        rust_path,
        variants,
        doc,
    })
}

/// Extract a public free function into a `FunctionDef`.
/// Returns `None` for generic functions — they can't be directly exposed to FFI.
fn extract_function(item: &syn::ItemFn, crate_name: &str, module_path: &str) -> Option<FunctionDef> {
    if !item.sig.generics.params.is_empty() {
        return None;
    }
    let cfg = extract_cfg_condition(&item.attrs);
    let name = item.sig.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);
    let mut is_async = item.sig.asyncness.is_some();

    let (mut return_type, error_type, returns_ref) = resolve_return_type(&item.sig.output);

    // Detect future-returning functions as async
    if !is_async {
        if let Some(inner) = unwrap_future_return(&item.sig.output) {
            is_async = true;
            return_type = inner;
        }
    }

    let params = extract_params(&item.sig.inputs);
    let rust_path = build_rust_path(crate_name, module_path, &name);

    Some(FunctionDef {
        rust_path,
        name,
        params,
        return_type,
        is_async,
        error_type,
        doc,
        cfg,
        sanitized: false,
        returns_ref,
    })
}

/// Extract methods from an `impl` block and attach them to the corresponding `TypeDef`.
fn extract_impl_block(
    item: &syn::ItemImpl,
    crate_name: &str,
    module_path: &str,
    surface: &mut ApiSurface,
    type_index: &AHashMap<String, usize>,
) {
    if item.trait_.is_some() {
        // Extract trait impl methods and attach to the type if it's in our surface
        extract_trait_impl_methods(item, crate_name, surface, type_index);
        return;
    }

    let type_name = match &*item.self_ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
        _ => return,
    };

    let methods: Vec<MethodDef> = item
        .items
        .iter()
        .filter_map(|impl_item| {
            if let syn::ImplItem::Fn(method) = impl_item {
                if is_pub(&method.vis) {
                    // Skip generic methods — they can't be directly exposed to FFI
                    if !method.sig.generics.params.is_empty() {
                        return None;
                    }
                    // Skip feature-gated methods — they may not be available in binding crates
                    if has_cfg_attribute(&method.attrs) {
                        return None;
                    }
                    // Skip methods named "new" that return Self — constructor already generated from fields
                    let method_name = method.sig.ident.to_string();
                    if method_name == "new" {
                        if let syn::ReturnType::Type(_, ty) = &method.sig.output {
                            if matches!(&**ty, syn::Type::Path(p) if p.path.is_ident("Self")) {
                                return None;
                            }
                        }
                    }
                    return Some(extract_method(method, crate_name, &type_name, None));
                }
            }
            None
        })
        .collect();

    if methods.is_empty() {
        return;
    }

    // Use index for O(1) lookup; if not found, create opaque type
    if let Some(&idx) = type_index.get(&type_name) {
        // Dedup: skip methods whose name already exists on the type
        for method in methods {
            if !surface.types[idx].methods.iter().any(|m| m.name == method.name) {
                surface.types[idx].methods.push(method);
            }
        }
    } else {
        // The impl is for a type we haven't seen as a pub struct — create an opaque entry
        let rust_path = build_rust_path(crate_name, module_path, &type_name);
        surface.types.push(TypeDef {
            name: type_name.clone(),
            rust_path,
            fields: vec![],
            methods,
            is_opaque: true,
            is_clone: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            doc: String::new(),
            cfg: None,
        });
    }
}

/// Extract methods from a trait impl and attach them to an existing type in the surface.
fn extract_trait_impl_methods(
    item: &syn::ItemImpl,
    crate_name: &str,
    surface: &mut ApiSurface,
    type_index: &AHashMap<String, usize>,
) {
    let type_name = match &*item.self_ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    };

    let Some(type_name) = type_name else { return };

    // Use index for O(1) lookup — only attach to types we already know about
    let Some(&idx) = type_index.get(&type_name) else {
        return;
    };

    // Extract the trait path from `impl TraitPath for Type`
    // Standard library traits that should NOT be imported (always in scope or from std)
    const STD_TRAITS: &[&str] = &[
        "Default",
        "Clone",
        "Copy",
        "Debug",
        "Display",
        "Drop",
        "PartialEq",
        "Eq",
        "PartialOrd",
        "Ord",
        "Hash",
        "From",
        "Into",
        "TryFrom",
        "TryInto",
        "Iterator",
        "IntoIterator",
        "Send",
        "Sync",
        "Sized",
        "Unpin",
        "Serialize",
        "Deserialize", // serde — re-exported, not crate-local
    ];
    let trait_source = item.trait_.as_ref().and_then(|(_, path, _)| {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let trait_name = segments.last().map(|s| s.as_str()).unwrap_or("");
        // Skip standard library traits — they don't need explicit imports
        if STD_TRAITS.contains(&trait_name) {
            return None;
        }
        // Only record multi-segment trait paths (e.g. "tower::CacheStore").
        // Single-segment traits (e.g. just "CacheStore") can't be reliably
        // resolved to their full path — they may be in submodules not re-exported
        // at the crate root. The binding crate's Cargo.toml should import them.
        if segments.len() == 1 {
            None // Skip — can't determine full import path
        } else {
            Some(segments.join("::").replace('-', "_"))
        }
    });

    let type_def = &mut surface.types[idx];

    // Detect `impl Default for Type` — mark type as has_default and extract default values
    if let Some((_, path, _)) = &item.trait_ {
        if path.segments.last().is_some_and(|s| s.ident == "Default") {
            type_def.has_default = true;
            extract_default_values(item, &mut type_def.fields);
        }
    }

    // Extract methods from the trait impl (trait methods are implicitly pub)
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            // Skip generic methods — they can't be directly exposed to FFI
            if !method.sig.generics.params.is_empty() {
                continue;
            }
            // Skip feature-gated methods
            if has_cfg_attribute(&method.attrs) {
                continue;
            }
            let method_def = extract_method(method, crate_name, &type_name, trait_source.clone());
            // Don't add duplicates
            if !type_def.methods.iter().any(|m| m.name == method_def.name) {
                type_def.methods.push(method_def);
            }
        }
    }
}

/// Extract concrete default values from an `impl Default for T` block.
///
/// Finds the `fn default() -> Self` method, parses its struct literal body,
/// and maps each field initializer expression to a `DefaultValue` variant.
/// Falls back to `DefaultValue::Empty` for expressions that cannot be parsed
/// into a concrete literal (e.g., method calls, complex expressions).
fn extract_default_values(item: &syn::ItemImpl, fields: &mut [FieldDef]) {
    // Find the `fn default()` method
    let default_fn = item.items.iter().find_map(|impl_item| {
        if let syn::ImplItem::Fn(method) = impl_item {
            if method.sig.ident == "default" {
                return Some(method);
            }
        }
        None
    });

    let Some(default_fn) = default_fn else {
        // No fn default() found — mark all fields as Empty
        for field in fields.iter_mut() {
            field.typed_default = Some(DefaultValue::Empty);
        }
        return;
    };

    // Build a map of field name → DefaultValue from the struct literal
    let defaults = parse_default_body(&default_fn.block);

    for field in fields.iter_mut() {
        if let Some(default_val) = defaults.get(&field.name) {
            field.typed_default = Some(default_val.clone());
        } else {
            // Field exists but wasn't in the struct literal — use Empty
            field.typed_default = Some(DefaultValue::Empty);
        }
    }
}

/// Parse the body of a `fn default()` to extract field → `DefaultValue` mappings.
///
/// Looks for a struct literal (`Self { field: expr, ... }`) in the function body
/// and maps each field initializer to a `DefaultValue`.
fn parse_default_body(block: &syn::Block) -> AHashMap<String, DefaultValue> {
    let mut defaults = AHashMap::new();

    // The body should contain a struct literal, possibly as the last expression.
    // It could be `Self { ... }` or `TypeName { ... }`.
    let struct_expr = find_struct_expr(block);

    let Some(struct_expr) = struct_expr else {
        return defaults;
    };

    for field in &struct_expr.fields {
        let Some(ident) = &field.member_named() else {
            continue;
        };
        let field_name = ident.to_string();
        let default_val = expr_to_default_value(&field.expr);
        defaults.insert(field_name, default_val);
    }

    defaults
}

/// Recursively search a block for a struct expression (`Self { ... }` or `Name { ... }`).
fn find_struct_expr(block: &syn::Block) -> Option<&syn::ExprStruct> {
    // Check the last statement (tail expression or expression statement)
    for stmt in block.stmts.iter().rev() {
        match stmt {
            syn::Stmt::Expr(expr, _) => {
                if let Some(s) = unwrap_to_struct_expr(expr) {
                    return Some(s);
                }
            }
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    if let Some(s) = unwrap_to_struct_expr(&init.expr) {
                        return Some(s);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Try to unwrap an expression to a struct expression, looking through blocks.
fn unwrap_to_struct_expr(expr: &syn::Expr) -> Option<&syn::ExprStruct> {
    match expr {
        syn::Expr::Struct(s) => Some(s),
        syn::Expr::Block(b) => find_struct_expr(&b.block),
        _ => None,
    }
}

/// Helper trait to extract the named member from a `FieldValue`.
trait FieldMemberExt {
    fn member_named(&self) -> Option<&syn::Ident>;
}

impl FieldMemberExt for syn::FieldValue {
    fn member_named(&self) -> Option<&syn::Ident> {
        match &self.member {
            syn::Member::Named(ident) => Some(ident),
            syn::Member::Unnamed(_) => None,
        }
    }
}

/// Convert an expression to a `DefaultValue`.
///
/// Recognizes:
/// - `true` / `false` → `BoolLiteral`
/// - Integer literals → `IntLiteral`
/// - Float literals → `FloatLiteral`
/// - `"str".to_string()`, `String::from("str")`, `"str".into()` → `StringLiteral`
/// - `String::new()` → `StringLiteral("")`
/// - `'c'` (char literal) → `StringLiteral("c")`
/// - `Vec::new()`, `vec![]` → `Empty`
/// - `SomeType::default()`, `Default::default()` → `Empty`
/// - `SomeEnum::Variant` → `EnumVariant("Variant")`
/// - Anything else → `Empty`
fn expr_to_default_value(expr: &syn::Expr) -> DefaultValue {
    match expr {
        // Boolean and numeric literals
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Bool(b) => DefaultValue::BoolLiteral(b.value),
            syn::Lit::Int(i) => {
                if let Ok(val) = i.base10_parse::<i64>() {
                    DefaultValue::IntLiteral(val)
                } else {
                    DefaultValue::Empty
                }
            }
            syn::Lit::Float(f) => {
                if let Ok(val) = f.base10_parse::<f64>() {
                    DefaultValue::FloatLiteral(val)
                } else {
                    DefaultValue::Empty
                }
            }
            syn::Lit::Char(c) => DefaultValue::StringLiteral(c.value().to_string()),
            syn::Lit::Str(s) => DefaultValue::StringLiteral(s.value()),
            _ => DefaultValue::Empty,
        },

        // Unary negation: `-1`, `-3.14`
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => match expr_to_default_value(&unary.expr) {
            DefaultValue::IntLiteral(v) => DefaultValue::IntLiteral(-v),
            DefaultValue::FloatLiteral(v) => DefaultValue::FloatLiteral(-v),
            _ => DefaultValue::Empty,
        },

        // Method calls: "str".to_string(), "str".into(), etc.
        syn::Expr::MethodCall(mc) => {
            let method_name = mc.method.to_string();
            match method_name.as_str() {
                "to_string" | "to_owned" | "into" => {
                    // Check if receiver is a string literal
                    if let syn::Expr::Lit(lit) = &*mc.receiver {
                        if let syn::Lit::Str(s) = &lit.lit {
                            return DefaultValue::StringLiteral(s.value());
                        }
                    }
                    DefaultValue::Empty
                }
                _ => DefaultValue::Empty,
            }
        }

        // Function/associated function calls: String::from("..."), String::new(), Vec::new(),
        // SomeType::default(), Default::default()
        syn::Expr::Call(call) => {
            if let syn::Expr::Path(path) = &*call.func {
                let segments: Vec<String> = path.path.segments.iter().map(|s| s.ident.to_string()).collect();

                // String::from("...") or String::from(lit)
                if segments == ["String", "from"] && call.args.len() == 1 {
                    if let Some(syn::Expr::Lit(lit)) = call.args.first() {
                        if let syn::Lit::Str(s) = &lit.lit {
                            return DefaultValue::StringLiteral(s.value());
                        }
                    }
                    return DefaultValue::Empty;
                }

                // String::new() → empty string
                if segments == ["String", "new"] && call.args.is_empty() {
                    return DefaultValue::StringLiteral(String::new());
                }

                // Vec::new(), HashMap::new(), HashSet::new(), etc.
                if segments.len() == 2 && segments[1] == "new" && call.args.is_empty() {
                    let type_name = &segments[0];
                    if matches!(
                        type_name.as_str(),
                        "Vec" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "AHashMap" | "AHashSet"
                    ) {
                        return DefaultValue::Empty;
                    }
                }

                // SomeType::default() or Default::default()
                if segments.last().is_some_and(|s| s == "default") {
                    return DefaultValue::Empty;
                }
            }
            DefaultValue::Empty
        }

        // Path expressions: SomeEnum::Variant (no function call)
        syn::Expr::Path(path) => {
            let segments: Vec<String> = path.path.segments.iter().map(|s| s.ident.to_string()).collect();
            if segments.len() == 2 {
                // SomeEnum::Variant → EnumVariant("Variant")
                return DefaultValue::EnumVariant(segments[1].clone());
            }
            // Single ident like `true`/`false` are handled as Lit, but just in case
            DefaultValue::Empty
        }

        // Macro calls: vec![], hashmap!{}, etc.
        syn::Expr::Macro(mac) => {
            // vec![] with empty tokens → Empty
            let macro_name = mac
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            if matches!(macro_name.as_str(), "vec" | "hashmap" | "hashset") && mac.mac.tokens.is_empty() {
                return DefaultValue::Empty;
            }
            DefaultValue::Empty
        }

        _ => DefaultValue::Empty,
    }
}

/// Extract a single method from an impl block.
/// `parent_type_name` is used to resolve `Self` references in return types and params.
/// `trait_source` is the fully qualified trait path if this method comes from a trait impl.
fn extract_method(
    method: &syn::ImplItemFn,
    _crate_name: &str,
    parent_type_name: &str,
    trait_source: Option<String>,
) -> MethodDef {
    let name = method.sig.ident.to_string();
    let doc = extract_doc_comments(&method.attrs);
    let mut is_async = method.sig.asyncness.is_some();

    let (mut return_type, error_type, returns_ref) = resolve_return_type(&method.sig.output);

    // Detect future-returning functions as async:
    // BoxFuture<'_, T>, Pin<Box<dyn Future<Output = T>>>, etc.
    if !is_async {
        if let Some(inner) = unwrap_future_return(&method.sig.output) {
            is_async = true;
            return_type = inner;
        }
    }

    // Resolve `Self` → actual parent type name in return types and params
    resolve_self_refs(&mut return_type, parent_type_name);

    let (receiver, is_static) = detect_receiver(&method.sig.inputs);
    let mut params = extract_params(&method.sig.inputs);
    for param in &mut params {
        resolve_self_refs(&mut param.ty, parent_type_name);
    }

    MethodDef {
        name,
        params,
        return_type,
        is_async,
        is_static,
        error_type,
        doc,
        receiver,
        sanitized: false,
        trait_source,
        returns_ref,
    }
}

/// Replace `TypeRef::Named("Self")` with the actual parent type name, recursively.
fn resolve_self_refs(ty: &mut TypeRef, parent_type_name: &str) {
    match ty {
        TypeRef::Named(n) if n == "Self" => *n = parent_type_name.to_string(),
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => resolve_self_refs(inner, parent_type_name),
        TypeRef::Map(k, v) => {
            resolve_self_refs(k, parent_type_name);
            resolve_self_refs(v, parent_type_name);
        }
        _ => {}
    }
}

/// Check if a return type is a future type (BoxFuture, Pin<Box<dyn Future>>, etc.)
/// and extract the inner output type.
fn unwrap_future_return(output: &syn::ReturnType) -> Option<TypeRef> {
    let ty = match output {
        syn::ReturnType::Type(_, ty) => ty,
        syn::ReturnType::Default => return None,
    };

    // Check the outermost type name
    if let syn::Type::Path(type_path) = ty.as_ref() {
        if let Some(seg) = type_path.path.segments.last() {
            let ident = seg.ident.to_string();
            match ident.as_str() {
                // BoxFuture<'_, T> or BoxStream<'_, T> → async returning T
                "BoxFuture" | "BoxStream" => {
                    return extract_future_inner_type(seg);
                }
                // Pin<Box<dyn Future<Output = T>>> → async returning T
                "Pin" => {
                    return extract_pin_future_inner(seg);
                }
                _ => {}
            }
        }
    }
    None
}

/// Extract inner type from BoxFuture<'_, T> or BoxFuture<'_, Result<T, E>>
fn extract_future_inner_type(segment: &syn::PathSegment) -> Option<TypeRef> {
    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
        // BoxFuture has lifetime + type args. Find the type arg.
        for arg in &args.args {
            if let syn::GenericArgument::Type(ty) = arg {
                let resolved = type_resolver::resolve_type(ty);
                return Some(resolved);
            }
        }
    }
    None
}

/// Extract inner type from Pin<Box<dyn Future<Output = T>>>
fn extract_pin_future_inner(segment: &syn::PathSegment) -> Option<TypeRef> {
    // Pin<Box<dyn Future<Output = T>>>
    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
        for arg in &args.args {
            if let syn::GenericArgument::Type(syn::Type::Path(inner_path)) = arg {
                if let Some(inner_seg) = inner_path.path.segments.last() {
                    if inner_seg.ident == "Box" {
                        // Box<dyn Future<Output = T>>
                        if let syn::PathArguments::AngleBracketed(box_args) = &inner_seg.arguments {
                            for box_arg in &box_args.args {
                                if let syn::GenericArgument::Type(syn::Type::TraitObject(trait_obj)) = box_arg {
                                    return extract_future_output_from_trait_obj(trait_obj);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract Output type from `dyn Future<Output = T>`
fn extract_future_output_from_trait_obj(trait_obj: &syn::TypeTraitObject) -> Option<TypeRef> {
    for bound in &trait_obj.bounds {
        if let syn::TypeParamBound::Trait(trait_bound) = bound {
            if let Some(seg) = trait_bound.path.segments.last() {
                if seg.ident == "Future" {
                    // Look for Output = T in angle-bracketed args
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        for arg in &args.args {
                            if let syn::GenericArgument::AssocType(assoc) = arg {
                                if assoc.ident == "Output" {
                                    return Some(type_resolver::resolve_type(&assoc.ty));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Detect the receiver kind from method inputs.
fn detect_receiver(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>,
) -> (Option<ReceiverKind>, bool) {
    for input in inputs {
        if let syn::FnArg::Receiver(recv) = input {
            let kind = if recv.reference.is_some() {
                if recv.mutability.is_some() {
                    ReceiverKind::RefMut
                } else {
                    ReceiverKind::Ref
                }
            } else {
                ReceiverKind::Owned
            };
            return (Some(kind), false);
        }
    }
    (None, true)
}

/// Extract function/method parameters, skipping `self` receivers.
fn extract_params(inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>) -> Vec<ParamDef> {
    inputs
        .iter()
        .filter_map(|arg| {
            if let syn::FnArg::Typed(pat_type) = arg {
                let name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                };
                let resolved = type_resolver::resolve_type(&pat_type.ty);
                let (ty, optional) = unwrap_optional(resolved);
                Some(ParamDef {
                    name,
                    ty,
                    optional,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                })
            } else {
                None // Skip self receiver
            }
        })
        .collect()
}

/// Resolve the return type, extract error type, and detect reference returns.
///
/// Returns `(resolved_type, error_type, returns_ref)`.
/// `returns_ref` is true when the core return type (after Result unwrapping) is a
/// reference — e.g. `&T`, `Option<&str>`, `&[u8]`. Code generators use this flag
/// to insert `.clone()` before type conversion in delegation code.
fn resolve_return_type(output: &syn::ReturnType) -> (TypeRef, Option<String>, bool) {
    match output {
        syn::ReturnType::Default => (TypeRef::Unit, None, false),
        syn::ReturnType::Type(_, ty) => {
            let error_type = type_resolver::extract_result_error_type(ty);
            let inner_ty = if let Some(inner) = type_resolver::unwrap_result_type(ty) {
                inner
            } else {
                ty.as_ref()
            };
            // Unwrap Box/Arc/Rc wrappers to check the actual inner type
            let unwrapped = unwrap_smart_pointer(inner_ty);
            let returns_ref = syn_type_contains_ref(unwrapped);
            let resolved = type_resolver::resolve_type(inner_ty);
            (resolved, error_type, returns_ref)
        }
    }
}

/// Unwrap Box<T>, Arc<T>, Rc<T> wrappers to get the inner syn::Type.
fn unwrap_smart_pointer(ty: &syn::Type) -> &syn::Type {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();
            if matches!(ident.as_str(), "Box" | "Arc" | "Rc") {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            return inner;
                        }
                    }
                }
            }
        }
    }
    ty
}

/// Check if a syn::Type is or contains a reference.
///
/// Detects: `&T`, `Option<&T>`, `Vec<&T>`, etc.
fn syn_type_contains_ref(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(_) => true,
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    return args.args.iter().any(|arg| {
                        if let syn::GenericArgument::Type(inner) = arg {
                            syn_type_contains_ref(inner)
                        } else {
                            false
                        }
                    });
                }
            }
            false
        }
        _ => false,
    }
}

/// Extract a `mod` declaration and recursively process its contents.
#[allow(clippy::too_many_arguments)]
fn extract_module(
    item_mod: &syn::ItemMod,
    source_path: &Path,
    crate_name: &str,
    module_path: &str,
    reexport_map: &AHashMap<String, ReexportKind>,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    let mod_name = item_mod.ident.to_string();

    // Build the new module path for items inside this module.
    // If the parent has a glob re-export (`pub use mod_name::*`), all items from this
    // submodule are available at the parent level, so they keep the parent's module_path.
    let reexport_kind = reexport_map.get(&mod_name);
    let has_glob_reexport = matches!(reexport_kind, Some(ReexportKind::Glob));

    // For glob re-exports, items keep the parent's module_path (flattened).
    // For named re-exports, items get the deep path first, then we post-process.
    let new_module_path = if has_glob_reexport {
        module_path.to_string()
    } else if module_path.is_empty() {
        mod_name.clone()
    } else {
        format!("{module_path}::{mod_name}")
    };

    // Track surface sizes before extraction so we can post-process named re-exports.
    let named_reexports = match reexport_kind {
        Some(ReexportKind::Names(names)) => Some(names),
        _ => None,
    };
    let (types_before, enums_before, fns_before) = if named_reexports.is_some() {
        (surface.types.len(), surface.enums.len(), surface.functions.len())
    } else {
        (0, 0, 0)
    };

    // Inline module: `pub mod foo { ... }`
    if let Some((_, items)) = &item_mod.content {
        extract_items(
            items,
            source_path,
            crate_name,
            &new_module_path,
            surface,
            workspace_root,
            visited,
        )?;
    } else {
        // External module: `pub mod foo;` — resolve to file
        let parent_dir = source_path.parent().unwrap_or_else(|| Path::new("."));

        // Try `<mod_name>.rs` first, then `<mod_name>/mod.rs`
        let candidates = [
            parent_dir.join(format!("{mod_name}.rs")),
            parent_dir.join(&mod_name).join("mod.rs"),
        ];

        let mut found = false;
        for candidate in &candidates {
            if candidate.exists() {
                let content = std::fs::read_to_string(candidate)
                    .with_context(|| format!("Failed to read module file: {}", candidate.display()))?;
                let file = syn::parse_file(&content)
                    .with_context(|| format!("Failed to parse module file: {}", candidate.display()))?;
                extract_items(
                    &file.items,
                    candidate,
                    crate_name,
                    &new_module_path,
                    surface,
                    workspace_root,
                    visited,
                )?;
                found = true;
                break;
            }
        }

        if !found {
            return Ok(());
        }
    }

    // Post-process named re-exports: shorten rust_path for items whose names match.
    if let Some(names) = named_reexports {
        let parent_prefix = if module_path.is_empty() {
            crate_name.to_string()
        } else {
            format!("{crate_name}::{module_path}")
        };

        for ty in &mut surface.types[types_before..] {
            if names.contains(&ty.name) {
                ty.rust_path = format!("{parent_prefix}::{}", ty.name);
            }
        }
        for en in &mut surface.enums[enums_before..] {
            if names.contains(&en.name) {
                en.rust_path = format!("{parent_prefix}::{}", en.name);
            }
        }
        for func in &mut surface.functions[fns_before..] {
            if names.contains(&func.name) {
                func.rust_path = format!("{parent_prefix}::{}", func.name);
            }
        }
    }

    Ok(())
}

// --- pub use re-export resolution ---

/// Resolve a `pub use` tree, extracting re-exported items from workspace sibling crates.
fn resolve_use_tree(
    tree: &syn::UseTree,
    crate_name: &str,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    match tree {
        syn::UseTree::Path(use_path) => {
            let root_ident = use_path.ident.to_string();

            // Skip self/super/crate references — already handled by mod resolution
            if root_ident == "self" || root_ident == "super" || root_ident == "crate" {
                return Ok(());
            }

            // This is an external crate reference like `use other_crate::...`
            resolve_external_use(
                &root_ident,
                &use_path.tree,
                crate_name,
                surface,
                workspace_root,
                visited,
            )
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                resolve_use_tree(tree, crate_name, surface, workspace_root, visited)?;
            }
            Ok(())
        }
        // `pub use something;` — a single ident, skip (not an external crate path)
        _ => Ok(()),
    }
}

/// Resolve `pub use external_crate::...` by finding the crate source and extracting named items.
fn resolve_external_use(
    ext_crate_name: &str,
    subtree: &syn::UseTree,
    crate_name: &str,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    let Some(crate_source) = find_crate_source(ext_crate_name, workspace_root) else {
        return Ok(());
    };

    let canonical = std::fs::canonicalize(&crate_source).unwrap_or_else(|_| crate_source.clone());
    if visited.contains(&canonical) {
        return Ok(());
    }
    // Push to visited BEFORE any recursive calls to prevent infinite cycles
    visited.push(canonical.clone());

    // Parse the external crate source
    let content = match std::fs::read_to_string(&crate_source) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let file = match syn::parse_file(&content) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };

    // Extract the full surface of the external crate into a temporary surface
    let mut ext_surface = ApiSurface {
        crate_name: crate_name.to_string(), // Use our crate name for the rust_path
        version: String::new(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };
    extract_items(
        &file.items,
        &canonical,
        crate_name,
        "",
        &mut ext_surface,
        workspace_root,
        visited,
    )?;

    // Collect the names we want to import
    let filter = collect_use_names(subtree);

    match filter {
        UseFilter::All => {
            merge_surface(surface, ext_surface);
        }
        UseFilter::Names(names) => {
            merge_surface_filtered(surface, ext_surface, &names);
        }
    }

    Ok(())
}

/// What names does a use subtree import?
enum UseFilter {
    /// `use crate::*` — import everything
    All,
    /// `use crate::{A, B}` or `use crate::A` — import specific names
    Names(Vec<String>),
}

/// Collect the leaf names from a use subtree.
fn collect_use_names(tree: &syn::UseTree) -> UseFilter {
    match tree {
        syn::UseTree::Glob(_) => UseFilter::All,
        syn::UseTree::Name(name) => UseFilter::Names(vec![name.ident.to_string()]),
        syn::UseTree::Rename(rename) => UseFilter::Names(vec![rename.rename.to_string()]),
        syn::UseTree::Path(path) => collect_use_names(&path.tree),
        syn::UseTree::Group(group) => {
            let mut names = Vec::new();
            for item in &group.items {
                match collect_use_names(item) {
                    UseFilter::All => return UseFilter::All,
                    UseFilter::Names(n) => names.extend(n),
                }
            }
            UseFilter::Names(names)
        }
    }
}

/// Merge all items from `src` into `dst`, skipping duplicates.
fn merge_surface(dst: &mut ApiSurface, src: ApiSurface) {
    for ty in src.types {
        if !dst.types.iter().any(|t| t.name == ty.name) {
            dst.types.push(ty);
        }
    }
    for func in src.functions {
        if !dst.functions.iter().any(|f| f.name == func.name) {
            dst.functions.push(func);
        }
    }
    for en in src.enums {
        if !dst.enums.iter().any(|e| e.name == en.name) {
            dst.enums.push(en);
        }
    }
}

/// Merge only items whose name is in `names` from `src` into `dst`.
fn merge_surface_filtered(dst: &mut ApiSurface, src: ApiSurface, names: &[String]) {
    for ty in src.types {
        if names.contains(&ty.name) && !dst.types.iter().any(|t| t.name == ty.name) {
            dst.types.push(ty);
        }
    }
    for func in src.functions {
        if names.contains(&func.name) && !dst.functions.iter().any(|f| f.name == func.name) {
            dst.functions.push(func);
        }
    }
    for en in src.enums {
        if names.contains(&en.name) && !dst.enums.iter().any(|e| e.name == en.name) {
            dst.enums.push(en);
        }
    }
}

/// Find the `src/lib.rs` of a workspace sibling crate.
fn find_crate_source(dep_crate_name: &str, workspace_root: Option<&Path>) -> Option<PathBuf> {
    let root = workspace_root?;

    // Read workspace Cargo.toml
    let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let value: toml::Value = toml::from_str(&cargo_toml).ok()?;

    // Check [dependencies] for path deps
    if let Some(deps) = value.get("dependencies").and_then(|d| d.as_table()) {
        if let Some(path) = resolve_dep_path(deps, dep_crate_name, root) {
            return Some(path);
        }
    }

    // Check [workspace.dependencies]
    if let Some(deps) = value
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        if let Some(path) = resolve_dep_path(deps, dep_crate_name, root) {
            return Some(path);
        }
    }

    // Heuristic: look for crates/{crate_name}/src/lib.rs
    let heuristic = root.join("crates").join(dep_crate_name).join("src/lib.rs");
    if heuristic.exists() {
        return Some(heuristic);
    }

    // Try with hyphens replaced by underscores and vice versa
    let alt_name = if dep_crate_name.contains('-') {
        dep_crate_name.replace('-', "_")
    } else {
        dep_crate_name.replace('_', "-")
    };
    let alt = root.join("crates").join(&alt_name).join("src/lib.rs");
    if alt.exists() {
        return Some(alt);
    }

    None
}

/// Try to resolve a dependency path from a TOML dependencies table.
fn resolve_dep_path(deps: &toml::map::Map<String, toml::Value>, dep_name: &str, root: &Path) -> Option<PathBuf> {
    let dep = deps.get(dep_name)?;
    let path = dep.get("path").and_then(|p| p.as_str())?;
    let crate_dir = root.join(path);
    let lib_rs = crate_dir.join("src/lib.rs");
    if lib_rs.exists() { Some(lib_rs) } else { None }
}

// --- Attribute helpers ---

/// Check if a visibility is `pub`.
fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

/// Extract doc comments from attributes.
fn extract_doc_comments(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        let val = lit_str.value();
                        // Doc comments typically have a leading space
                        let trimmed = val.strip_prefix(' ').unwrap_or(&val);
                        lines.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    lines.join("\n")
}

/// Check if a `#[derive(...)]` attribute contains a specific derive.
fn has_derive(attrs: &[syn::Attribute], derive_name: &str) -> bool {
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let Ok(nested) =
                attr.parse_args_with(syn::punctuated::Punctuated::<syn::Path, syn::token::Comma>::parse_terminated)
            {
                for path in &nested {
                    if path.is_ident(derive_name) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::{PrimitiveType, TypeRef};

    /// Helper: parse source and extract into an ApiSurface.
    fn extract_from_source(source: &str) -> ApiSurface {
        let file = syn::parse_str::<syn::File>(source).expect("failed to parse test source");
        let mut surface = ApiSurface {
            crate_name: "test_crate".into(),
            version: "0.1.0".into(),
            types: vec![],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        };
        let mut visited = Vec::new();
        extract_items(
            &file.items,
            Path::new("test.rs"),
            "test_crate",
            "",
            &mut surface,
            None,
            &mut visited,
        )
        .unwrap();
        resolve_newtypes(&mut surface);
        surface
    }

    #[test]
    fn test_extract_simple_struct() {
        let source = r#"
            /// A configuration struct.
            #[derive(Clone, Debug)]
            pub struct Config {
                /// The name field.
                pub name: String,
                /// Optional timeout in seconds.
                pub timeout: Option<u64>,
                // Private field, should be excluded
                secret: String,
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.types.len(), 1);

        let config = &surface.types[0];
        assert_eq!(config.name, "Config");
        assert_eq!(config.rust_path, "test_crate::Config");
        assert!(config.is_clone);
        assert!(!config.is_opaque);
        assert_eq!(config.doc, "A configuration struct.");

        assert_eq!(config.fields.len(), 2);

        let name_field = &config.fields[0];
        assert_eq!(name_field.name, "name");
        assert_eq!(name_field.ty, TypeRef::String);
        assert!(!name_field.optional);
        assert_eq!(name_field.doc, "The name field.");

        let timeout_field = &config.fields[1];
        assert_eq!(timeout_field.name, "timeout");
        assert_eq!(timeout_field.ty, TypeRef::Primitive(PrimitiveType::U64));
        assert!(timeout_field.optional);
        assert_eq!(timeout_field.doc, "Optional timeout in seconds.");
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
            /// Output format.
            pub enum Format {
                /// Plain text.
                Text,
                /// JSON output.
                Json,
                /// Custom with config.
                Custom { name: String },
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.enums.len(), 1);

        let fmt = &surface.enums[0];
        assert_eq!(fmt.name, "Format");
        assert_eq!(fmt.variants.len(), 3);
        assert_eq!(fmt.variants[0].name, "Text");
        assert!(fmt.variants[0].fields.is_empty());
        assert_eq!(fmt.variants[2].name, "Custom");
        assert_eq!(fmt.variants[2].fields.len(), 1);
        assert_eq!(fmt.variants[2].fields[0].name, "name");
    }

    #[test]
    fn test_extract_free_function() {
        let source = r#"
            /// Process the input.
            pub async fn process(input: String, count: u32) -> Result<Vec<String>, MyError> {
                todo!()
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.functions.len(), 1);

        let func = &surface.functions[0];
        assert_eq!(func.name, "process");
        assert!(func.is_async);
        assert_eq!(func.error_type.as_deref(), Some("MyError"));
        assert_eq!(func.return_type, TypeRef::Vec(Box::new(TypeRef::String)));
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "input");
        assert_eq!(func.params[0].ty, TypeRef::String);
        assert_eq!(func.params[1].name, "count");
        assert_eq!(func.params[1].ty, TypeRef::Primitive(PrimitiveType::U32));
    }

    #[test]
    fn test_extract_impl_block() {
        let source = r#"
            pub struct Server {
                pub host: String,
            }

            impl Server {
                /// Create a new server.
                pub fn new(host: String) -> Self {
                    todo!()
                }

                /// Start listening.
                pub async fn listen(&self, port: u16) -> Result<(), std::io::Error> {
                    todo!()
                }

                /// Shutdown mutably.
                pub fn shutdown(&mut self) {
                    todo!()
                }

                // Private, should be excluded
                fn internal(&self) {}
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.types.len(), 1);

        let server = &surface.types[0];
        assert_eq!(server.name, "Server");
        // `new` returning Self is skipped (constructor generated from fields)
        assert_eq!(server.methods.len(), 2);

        let listen_method = &server.methods[0];
        assert_eq!(listen_method.name, "listen");
        assert!(listen_method.is_async);
        assert!(!listen_method.is_static);
        assert_eq!(listen_method.receiver, Some(ReceiverKind::Ref));
        assert_eq!(listen_method.error_type.as_deref(), Some("std::io::Error"));
        assert_eq!(listen_method.return_type, TypeRef::Unit);

        let shutdown_method = &server.methods[1];
        assert_eq!(shutdown_method.name, "shutdown");
        assert_eq!(shutdown_method.receiver, Some(ReceiverKind::RefMut));
    }

    #[test]
    fn test_private_items_excluded() {
        let source = r#"
            struct PrivateStruct {
                pub field: u32,
            }

            pub(crate) struct CrateStruct {
                pub field: u32,
            }

            fn private_fn() {}

            pub fn public_fn() {}
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.types.len(), 0);
        assert_eq!(surface.functions.len(), 1);
        assert_eq!(surface.functions[0].name, "public_fn");
    }

    #[test]
    fn test_opaque_struct() {
        let source = r#"
            pub struct Handle {
                inner: u64,
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.types.len(), 1);
        assert!(surface.types[0].is_opaque);
        assert!(surface.types[0].fields.is_empty());
    }

    #[test]
    fn test_inline_module() {
        let source = r#"
            pub mod inner {
                pub fn helper() -> bool {
                    true
                }
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.functions.len(), 1);
        assert_eq!(surface.functions[0].name, "helper");
    }

    #[test]
    fn test_enum_with_tuple_variants() {
        let source = r#"
            pub enum Value {
                Int(i64),
                Pair(String, u32),
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.enums.len(), 1);
        let val = &surface.enums[0];
        assert_eq!(val.variants[0].fields.len(), 1);
        assert_eq!(val.variants[0].fields[0].name, "_0");
        assert_eq!(val.variants[1].fields.len(), 2);
    }

    #[test]
    fn test_method_with_owned_self() {
        let source = r#"
            pub struct Builder {}

            impl Builder {
                pub fn build(self) -> String {
                    todo!()
                }
            }
        "#;

        let surface = extract_from_source(source);
        let builder = &surface.types[0];
        assert_eq!(builder.methods.len(), 1);
        assert_eq!(builder.methods[0].receiver, Some(ReceiverKind::Owned));
        assert!(!builder.methods[0].is_static);
    }

    #[test]
    fn test_trait_impl_methods_extracted() {
        let source = r#"
            pub struct DefaultClient {
                pub base_url: String,
            }

            impl DefaultClient {
                pub fn new(base_url: String) -> DefaultClient {
                    todo!()
                }
            }

            trait LlmClient {
                async fn chat(&self, prompt: String) -> Result<String, MyError>;
                fn model(&self) -> String;
            }

            impl LlmClient for DefaultClient {
                async fn chat(&self, prompt: String) -> Result<String, MyError> {
                    todo!()
                }

                fn model(&self) -> String {
                    todo!()
                }
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.types.len(), 1);

        let client = &surface.types[0];
        assert_eq!(client.name, "DefaultClient");
        // Should have: new (not skipped because it doesn't return Self), chat, model
        // Actually new returns DefaultClient not Self, so it's included
        assert_eq!(client.methods.len(), 3);

        let method_names: Vec<&str> = client.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(method_names.contains(&"new"));
        assert!(method_names.contains(&"chat"));
        assert!(method_names.contains(&"model"));

        // Verify chat is async
        let chat = client.methods.iter().find(|m| m.name == "chat").unwrap();
        assert!(chat.is_async);
        assert_eq!(chat.receiver, Some(ReceiverKind::Ref));
        assert_eq!(chat.error_type.as_deref(), Some("MyError"));
    }

    #[test]
    fn test_trait_impl_no_duplicate_methods() {
        let source = r#"
            pub struct MyType {}

            impl MyType {
                pub fn do_thing(&self) -> String {
                    todo!()
                }
            }

            trait SomeTrait {
                fn do_thing(&self) -> String;
            }

            impl SomeTrait for MyType {
                fn do_thing(&self) -> String {
                    todo!()
                }
            }
        "#;

        let surface = extract_from_source(source);
        let my_type = &surface.types[0];
        // Should not have duplicate do_thing
        let do_thing_count = my_type.methods.iter().filter(|m| m.name == "do_thing").count();
        assert_eq!(do_thing_count, 1);
    }

    #[test]
    fn test_trait_impl_ignored_for_unknown_type() {
        let source = r#"
            trait SomeTrait {
                fn method(&self);
            }

            impl SomeTrait for UnknownType {
                fn method(&self) {
                    todo!()
                }
            }
        "#;

        let surface = extract_from_source(source);
        // UnknownType is not in the surface, so trait impl methods should be ignored
        assert_eq!(surface.types.len(), 0);
    }

    #[test]
    fn test_pub_use_self_super_skipped() {
        let source = r#"
            pub use self::inner::Helper;
            pub use super::other::Thing;
            pub use crate::root::Item;

            pub mod inner {
                pub struct Helper {
                    pub value: u32,
                }
            }
        "#;

        let surface = extract_from_source(source);
        // self/super/crate use paths are skipped (handled by mod resolution)
        // The inline module should still be extracted
        assert_eq!(surface.types.len(), 1);
        assert_eq!(surface.types[0].name, "Helper");
    }

    #[test]
    fn test_collect_use_names_single() {
        let tree: syn::UseTree = syn::parse_str("Foo").unwrap();
        match collect_use_names(&tree) {
            UseFilter::Names(names) => assert_eq!(names, vec!["Foo"]),
            UseFilter::All => panic!("expected Names"),
        }
    }

    #[test]
    fn test_collect_use_names_group() {
        let tree: syn::UseTree = syn::parse_str("{Foo, Bar, Baz}").unwrap();
        match collect_use_names(&tree) {
            UseFilter::Names(names) => {
                assert_eq!(names.len(), 3);
                assert!(names.contains(&"Foo".to_string()));
                assert!(names.contains(&"Bar".to_string()));
                assert!(names.contains(&"Baz".to_string()));
            }
            UseFilter::All => panic!("expected Names"),
        }
    }

    #[test]
    fn test_collect_use_names_glob() {
        let tree: syn::UseTree = syn::parse_str("*").unwrap();
        assert!(matches!(collect_use_names(&tree), UseFilter::All));
    }

    #[test]
    fn test_merge_surface_no_duplicates() {
        let mut dst = ApiSurface {
            crate_name: "test".into(),
            version: "0.1.0".into(),
            types: vec![TypeDef {
                name: "Existing".into(),
                rust_path: "test::Existing".into(),
                fields: vec![],
                methods: vec![],
                is_opaque: true,
                is_clone: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                doc: String::new(),
                cfg: None,
            }],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        };

        let src = ApiSurface {
            crate_name: "test".into(),
            version: "0.1.0".into(),
            types: vec![
                TypeDef {
                    name: "Existing".into(),
                    rust_path: "test::Existing".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    is_trait: false,
                    has_default: false,
                    has_stripped_cfg_fields: false,
                    is_return_type: false,
                    doc: String::new(),
                    cfg: None,
                },
                TypeDef {
                    name: "NewType".into(),
                    rust_path: "test::NewType".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    is_trait: false,
                    has_default: false,
                    has_stripped_cfg_fields: false,
                    is_return_type: false,
                    doc: String::new(),
                    cfg: None,
                },
            ],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        };

        merge_surface(&mut dst, src);
        assert_eq!(dst.types.len(), 2);
        assert_eq!(dst.types[0].name, "Existing");
        assert_eq!(dst.types[1].name, "NewType");
    }

    #[test]
    fn test_merge_surface_filtered() {
        let mut dst = ApiSurface {
            crate_name: "test".into(),
            version: "0.1.0".into(),
            types: vec![],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        };

        let src = ApiSurface {
            crate_name: "test".into(),
            version: "0.1.0".into(),
            types: vec![
                TypeDef {
                    name: "Wanted".into(),
                    rust_path: "test::Wanted".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    is_trait: false,
                    has_default: false,
                    has_stripped_cfg_fields: false,
                    is_return_type: false,
                    doc: String::new(),
                    cfg: None,
                },
                TypeDef {
                    name: "NotWanted".into(),
                    rust_path: "test::NotWanted".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    is_trait: false,
                    has_default: false,
                    has_stripped_cfg_fields: false,
                    is_return_type: false,
                    doc: String::new(),
                    cfg: None,
                },
            ],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        };

        merge_surface_filtered(&mut dst, src, &["Wanted".to_string()]);
        assert_eq!(dst.types.len(), 1);
        assert_eq!(dst.types[0].name, "Wanted");
    }

    #[test]
    fn test_find_crate_source_no_workspace() {
        // With no workspace root, should return None
        assert!(find_crate_source("some_crate", None).is_none());
    }

    #[test]
    fn test_pub_use_reexport_from_workspace_crate() {
        // Create a temporary workspace structure
        let tmp = std::env::temp_dir().join("alef_test_reexport");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("crates/other_crate/src")).unwrap();

        // Write workspace Cargo.toml
        std::fs::write(
            tmp.join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/other_crate"]

[workspace.dependencies]
other_crate = { path = "crates/other_crate" }
"#,
        )
        .unwrap();

        // Write other_crate's lib.rs with a pub struct
        std::fs::write(
            tmp.join("crates/other_crate/src/lib.rs"),
            r#"
/// Server configuration.
#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

/// CORS settings.
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

/// Internal helper, not re-exported.
pub struct InternalHelper {
    pub data: String,
}
"#,
        )
        .unwrap();

        // Write our crate's lib.rs that re-exports specific items
        let our_lib = tmp.join("crates/my_crate/src/lib.rs");
        std::fs::create_dir_all(our_lib.parent().unwrap()).unwrap();
        std::fs::write(
            &our_lib,
            r#"
pub use other_crate::{ServerConfig, CorsConfig};
"#,
        )
        .unwrap();

        let sources: Vec<&Path> = vec![our_lib.as_path()];
        let surface = extract(&sources, "my_crate", "0.1.0", Some(&tmp)).unwrap();

        // Should have extracted ServerConfig and CorsConfig but not InternalHelper
        assert_eq!(surface.types.len(), 2);
        let names: Vec<&str> = surface.types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"ServerConfig"));
        assert!(names.contains(&"CorsConfig"));
        assert!(!names.contains(&"InternalHelper"));

        // Verify they use our crate name in rust_path
        let server = surface.types.iter().find(|t| t.name == "ServerConfig").unwrap();
        assert_eq!(server.rust_path, "my_crate::ServerConfig");
        assert!(server.is_clone);

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_pub_use_glob_reexport() {
        let tmp = std::env::temp_dir().join("alef_test_glob_reexport");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("crates/other_crate/src")).unwrap();

        std::fs::write(
            tmp.join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/other_crate"]

[workspace.dependencies]
other_crate = { path = "crates/other_crate" }
"#,
        )
        .unwrap();

        std::fs::write(
            tmp.join("crates/other_crate/src/lib.rs"),
            r#"
pub struct Alpha { pub value: u32 }
pub struct Beta { pub name: String }
"#,
        )
        .unwrap();

        let our_lib = tmp.join("crates/my_crate/src/lib.rs");
        std::fs::create_dir_all(our_lib.parent().unwrap()).unwrap();
        std::fs::write(&our_lib, "pub use other_crate::*;\n").unwrap();

        let sources: Vec<&Path> = vec![our_lib.as_path()];
        let surface = extract(&sources, "my_crate", "0.1.0", Some(&tmp)).unwrap();

        assert_eq!(surface.types.len(), 2);
        let names: Vec<&str> = surface.types.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Alpha"));
        assert!(names.contains(&"Beta"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_returns_ref_detection() {
        let source = r#"
            pub struct MyType {
                inner: String,
            }

            impl MyType {
                pub fn name(&self) -> &str {
                    &self.inner
                }

                pub fn owned_name(&self) -> String {
                    self.inner.clone()
                }

                pub fn opt_name(&self) -> Option<&str> {
                    Some(&self.inner)
                }

                pub fn opt_owned(&self) -> Option<String> {
                    Some(self.inner.clone())
                }

                pub fn result_ref(&self) -> Result<&str, String> {
                    Ok(&self.inner)
                }

                pub fn result_owned(&self) -> Result<String, String> {
                    Ok(self.inner.clone())
                }
            }
        "#;

        let surface = extract_from_source(source);
        let my_type = &surface.types[0];

        let find_method = |name: &str| my_type.methods.iter().find(|m| m.name == name).unwrap();

        // &str return → returns_ref = true
        assert!(find_method("name").returns_ref, "name() should have returns_ref=true");
        // String return → returns_ref = false
        assert!(
            !find_method("owned_name").returns_ref,
            "owned_name() should have returns_ref=false"
        );
        // Option<&str> → returns_ref = true
        assert!(
            find_method("opt_name").returns_ref,
            "opt_name() should have returns_ref=true"
        );
        // Option<String> → returns_ref = false
        assert!(
            !find_method("opt_owned").returns_ref,
            "opt_owned() should have returns_ref=false"
        );
        // Result<&str, _> → returns_ref = true (after Result unwrapping)
        assert!(
            find_method("result_ref").returns_ref,
            "result_ref() should have returns_ref=true"
        );
        // Result<String, _> → returns_ref = false
        assert!(
            !find_method("result_owned").returns_ref,
            "result_owned() should have returns_ref=false"
        );
    }

    #[test]
    fn test_newtype_wrapper_resolved() {
        let source = r#"
            /// An element identifier.
            pub struct ElementId(String);

            /// A widget with an element id.
            pub struct Widget {
                pub id: ElementId,
                pub label: String,
            }
        "#;

        let surface = extract_from_source(source);

        // The newtype `ElementId` should be removed from the surface
        assert!(
            !surface.types.iter().any(|t| t.name == "ElementId"),
            "Newtype wrapper ElementId should be removed from types"
        );

        // Widget should exist with `id` resolved to String
        let widget = surface
            .types
            .iter()
            .find(|t| t.name == "Widget")
            .expect("Widget should exist");
        assert!(!widget.is_opaque);
        assert_eq!(widget.fields.len(), 2);
        assert_eq!(widget.fields[0].name, "id");
        assert_eq!(
            widget.fields[0].ty,
            TypeRef::String,
            "ElementId should resolve to String"
        );
        assert_eq!(widget.fields[1].name, "label");
        assert_eq!(widget.fields[1].ty, TypeRef::String);
    }

    #[test]
    fn test_newtype_wrapper_with_methods_not_resolved() {
        // Newtypes that have impl methods should NOT be resolved — they're real types.
        let source = r#"
            pub struct Token(String);

            impl Token {
                pub fn value(&self) -> &str {
                    &self.0
                }
            }
        "#;

        let surface = extract_from_source(source);

        // Token has methods, so it should remain in the surface (not resolved away)
        assert!(
            surface.types.iter().any(|t| t.name == "Token"),
            "Newtype with methods should be kept"
        );
    }

    #[test]
    fn test_newtype_in_optional_and_vec_resolved() {
        let source = r#"
            pub struct Id(u64);

            pub struct Container {
                pub primary: Option<Id>,
                pub all_ids: Vec<Id>,
            }
        "#;

        let surface = extract_from_source(source);

        assert!(
            !surface.types.iter().any(|t| t.name == "Id"),
            "Newtype Id should be removed"
        );

        let container = surface
            .types
            .iter()
            .find(|t| t.name == "Container")
            .expect("Container should exist");
        // primary: Option<Id> → Optional(u64)
        assert_eq!(container.fields[0].name, "primary");
        assert!(container.fields[0].optional);
        assert_eq!(container.fields[0].ty, TypeRef::Primitive(PrimitiveType::U64));

        // all_ids: Vec<Id> → Vec(u64)
        assert_eq!(container.fields[1].name, "all_ids");
        assert_eq!(
            container.fields[1].ty,
            TypeRef::Vec(Box::new(TypeRef::Primitive(PrimitiveType::U64)))
        );
    }

    #[test]
    fn test_tuple_struct_wrapping_named_type_not_resolved() {
        // A tuple struct wrapping a complex Named type (like a builder pattern)
        // should NOT be resolved as a transparent newtype.
        let source = r#"
            pub struct ConversionOptions {
                pub format: String,
            }

            pub struct ConversionOptionsBuilder(ConversionOptions);

            impl ConversionOptionsBuilder {
                pub fn format(&mut self, fmt: String) -> &mut Self {
                    self.0.format = fmt;
                    self
                }
            }
        "#;

        let surface = extract_from_source(source);

        // ConversionOptionsBuilder wraps a Named type AND has methods — should be kept
        assert!(
            surface.types.iter().any(|t| t.name == "ConversionOptionsBuilder"),
            "Tuple struct wrapping Named type should not be resolved away"
        );
    }

    #[test]
    fn test_tuple_struct_wrapping_named_type_no_methods_not_resolved() {
        // Even without methods, a tuple struct wrapping a complex Named type
        // should NOT be resolved as a transparent newtype.
        let source = r#"
            pub struct Inner {
                pub value: u32,
            }

            pub struct Wrapper(Inner);

            pub struct Consumer {
                pub item: Wrapper,
            }
        "#;

        let surface = extract_from_source(source);

        // Wrapper wraps a Named type — should be kept even without methods
        assert!(
            surface.types.iter().any(|t| t.name == "Wrapper"),
            "Tuple struct wrapping Named type should not be resolved even without methods"
        );

        // Consumer should reference Wrapper as Named, not have it inlined
        let consumer = surface
            .types
            .iter()
            .find(|t| t.name == "Consumer")
            .expect("Consumer should exist");
        assert_eq!(
            consumer.fields[0].ty,
            TypeRef::Named("Wrapper".to_string()),
            "Wrapper reference should remain as Named"
        );
    }

    #[test]
    fn test_extract_thiserror_enum() {
        let source = r#"
            #[derive(Debug, thiserror::Error)]
            pub enum MyError {
                /// An I/O error.
                #[error("I/O error: {0}")]
                Io(#[from] std::io::Error),

                /// A parsing error.
                #[error("Parsing error: {message}")]
                Parsing {
                    message: String,
                    #[source]
                    source: Option<Box<dyn std::error::Error + Send + Sync>>,
                },

                /// A timeout error.
                #[error("Extraction timed out after {elapsed_ms}ms")]
                Timeout { elapsed_ms: u64, limit_ms: u64 },

                /// A missing dependency.
                #[error("Missing dependency: {0}")]
                MissingDependency(String),

                /// An unknown error.
                #[error("Unknown error")]
                Unknown,
            }
        "#;

        let surface = extract_from_source(source);

        // Should be in errors, NOT in enums
        assert_eq!(surface.enums.len(), 0, "thiserror enum should not be in enums");
        assert_eq!(surface.errors.len(), 1, "thiserror enum should be in errors");

        let err = &surface.errors[0];
        assert_eq!(err.name, "MyError");
        assert_eq!(err.variants.len(), 5);

        // Io variant: tuple with #[from]
        let io = &err.variants[0];
        assert_eq!(io.name, "Io");
        assert_eq!(io.message_template.as_deref(), Some("I/O error: {0}"));
        assert!(io.has_from, "Io should have from");
        assert!(io.has_source, "Io should have source (implied by from)");
        assert!(!io.is_unit, "Io is not a unit variant");
        assert_eq!(io.fields.len(), 1);

        // Parsing variant: struct with #[source]
        let parsing = &err.variants[1];
        assert_eq!(parsing.name, "Parsing");
        assert_eq!(parsing.message_template.as_deref(), Some("Parsing error: {message}"));
        assert!(!parsing.has_from, "Parsing should not have from");
        assert!(parsing.has_source, "Parsing should have source");
        assert!(!parsing.is_unit);
        assert_eq!(parsing.fields.len(), 2);
        assert_eq!(parsing.fields[0].name, "message");
        assert_eq!(parsing.fields[1].name, "source");

        // Timeout variant: struct, no source/from
        let timeout = &err.variants[2];
        assert_eq!(timeout.name, "Timeout");
        assert_eq!(
            timeout.message_template.as_deref(),
            Some("Extraction timed out after {elapsed_ms}ms")
        );
        assert!(!timeout.has_from);
        assert!(!timeout.has_source);
        assert!(!timeout.is_unit);
        assert_eq!(timeout.fields.len(), 2);

        // MissingDependency: tuple variant, no source/from
        let missing = &err.variants[3];
        assert_eq!(missing.name, "MissingDependency");
        assert_eq!(missing.message_template.as_deref(), Some("Missing dependency: {0}"));
        assert!(!missing.has_from);
        assert!(!missing.has_source);
        assert!(!missing.is_unit);
        assert_eq!(missing.fields.len(), 1);

        // Unknown: unit variant
        let unknown = &err.variants[4];
        assert_eq!(unknown.name, "Unknown");
        assert_eq!(unknown.message_template.as_deref(), Some("Unknown error"));
        assert!(!unknown.has_from);
        assert!(!unknown.has_source);
        assert!(unknown.is_unit);
        assert_eq!(unknown.fields.len(), 0);
    }

    #[test]
    fn test_extract_thiserror_with_use_import() {
        // When Error is imported via `use thiserror::Error`, the derive is just `Error`
        let source = r#"
            #[derive(Debug, Error)]
            pub enum AppError {
                #[error("not found")]
                NotFound,

                #[error("invalid input: {0}")]
                InvalidInput(String),
            }
        "#;

        let surface = extract_from_source(source);

        assert_eq!(surface.enums.len(), 0);
        assert_eq!(surface.errors.len(), 1);

        let err = &surface.errors[0];
        assert_eq!(err.name, "AppError");
        assert_eq!(err.variants.len(), 2);

        assert!(err.variants[0].is_unit);
        assert_eq!(err.variants[0].message_template.as_deref(), Some("not found"));

        assert!(!err.variants[1].is_unit);
        assert_eq!(err.variants[1].fields.len(), 1);
    }

    #[test]
    fn test_non_thiserror_enum_not_in_errors() {
        let source = r#"
            #[derive(Debug, Clone)]
            pub enum Format {
                Pdf,
                Html,
            }
        "#;

        let surface = extract_from_source(source);
        assert_eq!(surface.enums.len(), 1);
        assert_eq!(surface.errors.len(), 0, "non-thiserror enum should not be in errors");
    }
}
