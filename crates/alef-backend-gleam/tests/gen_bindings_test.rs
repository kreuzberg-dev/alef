use alef_backend_gleam::GleamBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, GleamConfig};
use alef_core::ir::{
    ApiSurface, CoreWrapper, EnumDef, EnumVariant, ErrorDef, ErrorVariant, FieldDef, FunctionDef, ParamDef,
    PrimitiveType, TypeDef, TypeRef,
};

fn make_field(name: &str, ty: TypeRef, optional: bool) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        ty,
        optional,
        default: None,
        doc: String::new(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: None,
        core_wrapper: CoreWrapper::None,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
    }
}

fn make_param(name: &str, ty: TypeRef) -> ParamDef {
    ParamDef {
        name: name.to_string(),
        ty,
        optional: false,
        default: None,
        sanitized: false,
        typed_default: None,
        is_ref: false,
        is_mut: false,
        newtype_wrapper: None,
        original_type: None,
    }
}

fn make_type(name: &str, fields: Vec<FieldDef>) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        rust_path: format!("demo::{name}"),
        original_rust_path: String::new(),
        fields,
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        doc: String::new(),
        cfg: None,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
    }
}

fn make_config() -> AlefConfig {
    AlefConfig {
        version: None,
        crate_config: CrateConfig {
            name: "demo".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
            features: vec![],
            path_mappings: std::collections::HashMap::new(),
            auto_path_mappings: Default::default(),
            extra_dependencies: Default::default(),
            source_crates: vec![],
            error_type: None,
            error_constructor: None,
        },
        languages: vec![],
        exclude: Default::default(),
        include: Default::default(),
        output: Default::default(),
        python: None,
        node: None,
        ruby: None,
        php: None,
        elixir: None,
        wasm: None,
        ffi: None,
        gleam: None,
        go: None,
        java: None,
        kotlin: None,
        dart: None,
        swift: None,
        csharp: None,
        r: None,
        zig: None,
        scaffold: None,
        readme: None,
        lint: None,
        update: None,
        test: None,
        setup: None,
        clean: None,
        build_commands: None,
        publish: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: alef_core::config::CustomModulesConfig::default(),
        custom_registrations: alef_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
        generate: alef_core::config::GenerateConfig::default(),
        generate_overrides: std::collections::HashMap::new(),
        dto: Default::default(),
        sync: None,
        e2e: None,
        trait_bridges: vec![],
        tools: alef_core::config::ToolsConfig::default(),
    }
}

fn make_config_with_nif(nif_module: &str) -> AlefConfig {
    AlefConfig {
        version: None,
        crate_config: CrateConfig {
            name: "demo".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
            features: vec![],
            path_mappings: std::collections::HashMap::new(),
            auto_path_mappings: Default::default(),
            extra_dependencies: Default::default(),
            source_crates: vec![],
            error_type: None,
            error_constructor: None,
        },
        languages: vec![],
        exclude: Default::default(),
        include: Default::default(),
        output: Default::default(),
        python: None,
        node: None,
        ruby: None,
        php: None,
        elixir: None,
        wasm: None,
        ffi: None,
        gleam: Some(GleamConfig {
            app_name: None,
            nif_module: Some(nif_module.to_string()),
            features: None,
            serde_rename_all: None,
            rename_fields: std::collections::HashMap::new(),
            exclude_functions: Vec::new(),
            exclude_types: Vec::new(),
            run_wrapper: None,
            extra_lint_paths: Vec::new(),
        }),
        go: None,
        java: None,
        kotlin: None,
        dart: None,
        swift: None,
        csharp: None,
        r: None,
        zig: None,
        scaffold: None,
        readme: None,
        lint: None,
        update: None,
        test: None,
        setup: None,
        clean: None,
        build_commands: None,
        publish: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: alef_core::config::CustomModulesConfig::default(),
        custom_registrations: alef_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
        generate: alef_core::config::GenerateConfig::default(),
        generate_overrides: std::collections::HashMap::new(),
        dto: Default::default(),
        sync: None,
        e2e: None,
        trait_bridges: vec![],
        tools: alef_core::config::ToolsConfig::default(),
    }
}

#[test]
fn struct_emits_record_type() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![make_type(
            "Point",
            vec![
                make_field("x", TypeRef::Primitive(PrimitiveType::I32), false),
                make_field("y", TypeRef::Primitive(PrimitiveType::I32), false),
            ],
        )],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    assert!(content.contains("pub type Point {"), "missing type decl: {content}");
    assert!(content.contains("Point("), "missing constructor: {content}");
    assert!(content.contains("x: Int"));
    assert!(content.contains("y: Int"));
}

#[test]
fn function_emits_external_binding() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "greet".into(),
            rust_path: "demo::greet".into(),
            original_rust_path: String::new(),
            params: vec![make_param("who", TypeRef::String)],
            return_type: TypeRef::String,
            is_async: false,
            error_type: None,
            doc: String::new(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    let content = &files[0].content;
    assert!(
        content.contains("@external(erlang, \"Elixir.Demo.Native\", \"greet\")"),
        "missing external annotation: {content}"
    );
    assert!(content.contains("pub fn greet(who: String) -> String"));
}

#[test]
fn enum_emits_custom_type() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![EnumDef {
            name: "Status".into(),
            rust_path: "demo::Status".into(),
            original_rust_path: String::new(),
            variants: vec![
                EnumVariant {
                    name: "Active".into(),
                    fields: vec![],
                    doc: String::new(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "Inactive".into(),
                    fields: vec![],
                    doc: String::new(),
                    is_default: false,
                    serde_rename: None,
                },
            ],
            doc: String::new(),
            cfg: None,
            serde_tag: None,
            serde_rename_all: None,
        }],
        errors: vec![],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    let content = &files[0].content;
    assert!(content.contains("pub type Status {"));
    assert!(content.contains("Active"));
    assert!(content.contains("Inactive"));
}

#[test]
fn optional_field_imports_option() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![make_type(
            "Maybe",
            vec![make_field("value", TypeRef::Optional(Box::new(TypeRef::String)), false)],
        )],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    let content = &files[0].content;
    assert!(content.contains("import gleam/option.{type Option}"));
    assert!(content.contains("value: Option(String)"));
}

#[test]
fn error_emits_custom_type() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![ErrorDef {
            name: "DemoError".into(),
            rust_path: "demo::DemoError".into(),
            original_rust_path: String::new(),
            variants: vec![
                ErrorVariant {
                    name: "NotFound".into(),
                    message_template: None,
                    fields: vec![],
                    has_source: false,
                    has_from: false,
                    is_unit: true,
                    doc: String::new(),
                },
                ErrorVariant {
                    name: "InvalidInput".into(),
                    message_template: None,
                    fields: vec![make_field("details", TypeRef::String, false)],
                    has_source: false,
                    has_from: false,
                    is_unit: false,
                    doc: String::new(),
                },
            ],
            doc: String::new(),
        }],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    let content = &files[0].content;
    assert!(
        content.contains("pub type DemoError {"),
        "missing error type decl: {content}"
    );
    assert!(content.contains("NotFound"), "missing NotFound variant: {content}");
    assert!(
        content.contains("InvalidInput("),
        "missing InvalidInput constructor: {content}"
    );
    assert!(content.contains("details: String"), "missing details field: {content}");
}

#[test]
fn enum_tuple_variant_emits_unlabeled_field() {
    // Rust tuple variants like `Pdf(String)` produce fields named `_0`, `_1`, etc.
    // Gleam constructor arguments cannot have labels starting with `_`, so these
    // must be emitted as unlabeled positional arguments: `Pdf(String)` not `Pdf(_0: String)`.
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![EnumDef {
            name: "Wrapper".into(),
            rust_path: "demo::Wrapper".into(),
            original_rust_path: String::new(),
            variants: vec![EnumVariant {
                name: "Inner".into(),
                fields: vec![make_field("_0", TypeRef::String, false)],
                doc: String::new(),
                is_default: false,
                serde_rename: None,
            }],
            doc: String::new(),
            cfg: None,
            serde_tag: None,
            serde_rename_all: None,
        }],
        errors: vec![],
    };

    let files = GleamBackend.generate_bindings(&api, &make_config()).unwrap();
    let content = &files[0].content;
    assert!(
        !content.contains("_0:"),
        "positional field `_0` must not appear as a label: {content}"
    );
    assert!(
        content.contains("Inner(\n    String\n  )") || content.contains("Inner(\n    String"),
        "unlabeled String argument expected: {content}"
    );
}

#[test]
fn nif_module_override_uses_custom_name() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "greet".into(),
            rust_path: "demo::greet".into(),
            original_rust_path: String::new(),
            params: vec![make_param("who", TypeRef::String)],
            return_type: TypeRef::String,
            is_async: false,
            error_type: None,
            doc: String::new(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config_with_nif("custom_nif_atom");
    let files = GleamBackend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;
    assert!(
        content.contains("@external(erlang, \"custom_nif_atom\", \"greet\")"),
        "should use custom nif_module: {content}"
    );
}
