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
mod tests {
    use super::reexports::{UseFilter, collect_use_names, find_crate_source, merge_surface, merge_surface_filtered};
    use super::*;
    use alef_core::ir::{PrimitiveType, ReceiverKind, TypeRef};

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
