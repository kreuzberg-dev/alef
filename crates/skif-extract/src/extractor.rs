use std::path::Path;

use anyhow::{Context, Result};
use skif_core::ir::{
    ApiSurface, EnumDef, EnumVariant, FieldDef, FunctionDef, MethodDef, ParamDef, ReceiverKind, TypeDef, TypeRef,
};

use crate::type_resolver;

/// Extract the public API surface from Rust source files.
///
/// `sources` should be the root source files (e.g., `lib.rs`) of the crate.
/// Submodules referenced via `mod` declarations are resolved and extracted recursively.
pub fn extract(sources: &[&Path], crate_name: &str, version: &str) -> Result<ApiSurface> {
    let mut surface = ApiSurface {
        crate_name: crate_name.to_string(),
        version: version.to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    for source in sources {
        let content = std::fs::read_to_string(source)
            .with_context(|| format!("Failed to read source file: {}", source.display()))?;
        let file =
            syn::parse_file(&content).with_context(|| format!("Failed to parse source file: {}", source.display()))?;
        extract_items(&file.items, source, crate_name, &mut surface)?;
    }

    Ok(surface)
}

/// Extract items from a parsed syn file or module.
fn extract_items(items: &[syn::Item], source_path: &Path, crate_name: &str, surface: &mut ApiSurface) -> Result<()> {
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
            syn::Item::Impl(item_impl) => {
                extract_impl_block(item_impl, crate_name, surface);
            }
            syn::Item::Mod(item_mod) => {
                if is_pub(&item_mod.vis) {
                    extract_module(item_mod, source_path, crate_name, surface)?;
                }
            }
            _ => {}
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
    let is_async = item.sig.asyncness.is_some();

    let (return_type, error_type) = resolve_return_type(&item.sig.output);
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
fn extract_impl_block(item: &syn::ItemImpl, crate_name: &str, surface: &mut ApiSurface) {
    // Only handle inherent impls (not trait impls)
    if item.trait_.is_some() {
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

    // Find existing type and attach methods, or create a new opaque type
    if let Some(type_def) = surface.types.iter_mut().find(|t| t.name == type_name) {
        type_def.methods.extend(methods);
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

/// Extract a single method from an impl block.
fn extract_method(method: &syn::ImplItemFn, _crate_name: &str) -> MethodDef {
    let name = method.sig.ident.to_string();
    let doc = extract_doc_comments(&method.attrs);
    let is_async = method.sig.asyncness.is_some();

    let (return_type, error_type) = resolve_return_type(&method.sig.output);

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
) -> Result<()> {
    let mod_name = item_mod.ident.to_string();

    // Inline module: `pub mod foo { ... }`
    if let Some((_, items)) = &item_mod.content {
        return extract_items(items, source_path, crate_name, surface);
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
            return extract_items(&file.items, candidate, crate_name, surface);
        }
    }

    // Module file not found — not an error, just skip
    Ok(())
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
        extract_items(&file.items, Path::new("test.rs"), "test_crate", &mut surface).unwrap();
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
}
