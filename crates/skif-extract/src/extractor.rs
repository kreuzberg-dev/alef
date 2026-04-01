use std::path::{Path, PathBuf};

use ahash::AHashMap;
use anyhow::{Context, Result};
use skif_core::ir::{
    ApiSurface, EnumDef, EnumVariant, FieldDef, FunctionDef, MethodDef, ParamDef, ReceiverKind, TypeDef, TypeRef,
};

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
            &mut surface,
            workspace_root,
            &mut visited,
        )?;
    }

    Ok(surface)
}

/// Extract items from a parsed syn file or module.
fn extract_items(
    items: &[syn::Item],
    source_path: &Path,
    crate_name: &str,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    // First pass: collect all structs/enums (no impl blocks yet)
    for item in items {
        match item {
            syn::Item::Struct(item_struct) => {
                if is_pub(&item_struct.vis) {
                    surface.types.push(extract_struct(item_struct, crate_name));
                }
            }
            syn::Item::Enum(item_enum) => {
                if is_pub(&item_enum.vis) {
                    surface.enums.push(extract_enum(item_enum, crate_name));
                }
            }
            syn::Item::Fn(item_fn) => {
                if is_pub(&item_fn.vis) {
                    surface.functions.push(extract_function(item_fn, crate_name));
                }
            }
            syn::Item::Mod(item_mod) => {
                if is_pub(&item_mod.vis) {
                    extract_module(item_mod, source_path, crate_name, surface, workspace_root, visited)?;
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
            extract_impl_block(item_impl, crate_name, surface, &type_index);
        }
    }
    Ok(())
}

/// Extract a public struct into a `TypeDef`.
fn extract_struct(item: &syn::ItemStruct, crate_name: &str) -> TypeDef {
    let name = item.ident.to_string();
    let fields = match &item.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .filter(|f| is_pub(&f.vis))
            .map(extract_field)
            .collect(),
        _ => vec![],
    };

    let is_clone = has_derive(item.attrs.as_slice(), "Clone");
    let doc = extract_doc_comments(&item.attrs);
    let is_opaque = fields.is_empty();

    TypeDef {
        rust_path: format!("{crate_name}::{name}"),
        name,
        fields,
        methods: vec![],
        is_opaque,
        is_clone,
        doc,
    }
}

/// Extract a struct field into a `FieldDef`.
fn extract_field(field: &syn::Field) -> FieldDef {
    let name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
    let doc = extract_doc_comments(&field.attrs);

    let resolved = type_resolver::resolve_type(&field.ty);
    let (ty, optional) = unwrap_optional(resolved);

    FieldDef {
        name,
        ty,
        optional,
        default: None,
        doc,
    }
}

/// If the resolved type is `TypeRef::Optional(inner)`, unwrap it and mark as optional.
fn unwrap_optional(ty: TypeRef) -> (TypeRef, bool) {
    match ty {
        TypeRef::Optional(inner) => (*inner, true),
        other => (other, false),
    }
}

/// Extract a public enum into an `EnumDef`.
fn extract_enum(item: &syn::ItemEnum, crate_name: &str) -> EnumDef {
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
                        }
                    })
                    .collect(),
                syn::Fields::Unit => vec![],
            };
            EnumVariant {
                name: v.ident.to_string(),
                fields: variant_fields,
                doc: extract_doc_comments(&v.attrs),
            }
        })
        .collect();

    EnumDef {
        rust_path: format!("{crate_name}::{name}"),
        name,
        variants,
        doc,
    }
}

/// Extract a public free function into a `FunctionDef`.
fn extract_function(item: &syn::ItemFn, crate_name: &str) -> FunctionDef {
    let name = item.sig.ident.to_string();
    let doc = extract_doc_comments(&item.attrs);
    let mut is_async = item.sig.asyncness.is_some();

    let (mut return_type, error_type) = resolve_return_type(&item.sig.output);

    // Detect future-returning functions as async
    if !is_async {
        if let Some(inner) = unwrap_future_return(&item.sig.output) {
            is_async = true;
            return_type = inner;
        }
    }

    let params = extract_params(&item.sig.inputs);

    FunctionDef {
        rust_path: format!("{crate_name}::{name}"),
        name,
        params,
        return_type,
        is_async,
        error_type,
        doc,
    }
}

/// Extract methods from an `impl` block and attach them to the corresponding `TypeDef`.
fn extract_impl_block(
    item: &syn::ItemImpl,
    crate_name: &str,
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
                    // Skip methods named "new" that return Self — constructor already generated from fields
                    let method_name = method.sig.ident.to_string();
                    if method_name == "new" {
                        if let syn::ReturnType::Type(_, ty) = &method.sig.output {
                            if matches!(&**ty, syn::Type::Path(p) if p.path.is_ident("Self")) {
                                return None;
                            }
                        }
                    }
                    return Some(extract_method(method, crate_name));
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
        surface.types[idx].methods.extend(methods);
    } else {
        // The impl is for a type we haven't seen as a pub struct — create an opaque entry
        surface.types.push(TypeDef {
            name: type_name.clone(),
            rust_path: format!("{crate_name}::{type_name}"),
            fields: vec![],
            methods,
            is_opaque: true,
            is_clone: false,
            doc: String::new(),
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
    let type_def = &mut surface.types[idx];

    // Extract methods from the trait impl (trait methods are implicitly pub)
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let method_def = extract_method(method, crate_name);
            // Don't add duplicates
            if !type_def.methods.iter().any(|m| m.name == method_def.name) {
                type_def.methods.push(method_def);
            }
        }
    }
}

/// Extract a single method from an impl block.
fn extract_method(method: &syn::ImplItemFn, _crate_name: &str) -> MethodDef {
    let name = method.sig.ident.to_string();
    let doc = extract_doc_comments(&method.attrs);
    let mut is_async = method.sig.asyncness.is_some();

    let (mut return_type, error_type) = resolve_return_type(&method.sig.output);

    // Detect future-returning functions as async:
    // BoxFuture<'_, T>, Pin<Box<dyn Future<Output = T>>>, etc.
    if !is_async {
        if let Some(inner) = unwrap_future_return(&method.sig.output) {
            is_async = true;
            return_type = inner;
        }
    }

    let (receiver, is_static) = detect_receiver(&method.sig.inputs);
    let params = extract_params(&method.sig.inputs);

    MethodDef {
        name,
        params,
        return_type,
        is_async,
        is_static,
        error_type,
        doc,
        receiver,
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
                let optional = type_resolver::is_option_type(&pat_type.ty).is_some();
                let ty = type_resolver::resolve_type(&pat_type.ty);
                Some(ParamDef {
                    name,
                    ty,
                    optional,
                    default: None,
                })
            } else {
                None // Skip self receiver
            }
        })
        .collect()
}

/// Resolve the return type and extract error type if it's a `Result<T, E>`.
fn resolve_return_type(output: &syn::ReturnType) -> (TypeRef, Option<String>) {
    match output {
        syn::ReturnType::Default => (TypeRef::Unit, None),
        syn::ReturnType::Type(_, ty) => {
            let error_type = type_resolver::extract_result_error_type(ty);
            let resolved = if let Some(inner) = type_resolver::unwrap_result_type(ty) {
                type_resolver::resolve_type(inner)
            } else {
                type_resolver::resolve_type(ty)
            };
            (resolved, error_type)
        }
    }
}

/// Extract a `mod` declaration and recursively process its contents.
fn extract_module(
    item_mod: &syn::ItemMod,
    source_path: &Path,
    crate_name: &str,
    surface: &mut ApiSurface,
    workspace_root: Option<&Path>,
    visited: &mut Vec<PathBuf>,
) -> Result<()> {
    let mod_name = item_mod.ident.to_string();

    // Inline module: `pub mod foo { ... }`
    if let Some((_, items)) = &item_mod.content {
        return extract_items(items, source_path, crate_name, surface, workspace_root, visited);
    }

    // External module: `pub mod foo;` — resolve to file
    let parent_dir = source_path.parent().unwrap_or_else(|| Path::new("."));

    // Try `<mod_name>.rs` first, then `<mod_name>/mod.rs`
    let candidates = [
        parent_dir.join(format!("{mod_name}.rs")),
        parent_dir.join(&mod_name).join("mod.rs"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            let content = std::fs::read_to_string(candidate)
                .with_context(|| format!("Failed to read module file: {}", candidate.display()))?;
            let file = syn::parse_file(&content)
                .with_context(|| format!("Failed to parse module file: {}", candidate.display()))?;
            return extract_items(&file.items, candidate, crate_name, surface, workspace_root, visited);
        }
    }

    // Module file not found — not an error, just skip
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
    use skif_core::ir::{PrimitiveType, TypeRef};

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
            &mut surface,
            None,
            &mut visited,
        )
        .unwrap();
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
                doc: String::new(),
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
                    doc: String::new(),
                },
                TypeDef {
                    name: "NewType".into(),
                    rust_path: "test::NewType".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    doc: String::new(),
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
                    doc: String::new(),
                },
                TypeDef {
                    name: "NotWanted".into(),
                    rust_path: "test::NotWanted".into(),
                    fields: vec![],
                    methods: vec![],
                    is_opaque: true,
                    is_clone: false,
                    doc: String::new(),
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
        let tmp = std::env::temp_dir().join("skif_test_reexport");
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
        let tmp = std::env::temp_dir().join("skif_test_glob_reexport");
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
}
