use alef_backend_wasm::WasmBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, WasmConfig};
use alef_core::ir::{
    ApiSurface, EnumDef, EnumVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef, TypeRef,
};

/// Helper to create a field definition with all defaults
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
        core_wrapper: alef_core::ir::CoreWrapper::None,
        vec_inner_core_wrapper: alef_core::ir::CoreWrapper::None,
    }
}

/// Helper to create minimal AlefConfig with WASM enabled
fn make_config() -> AlefConfig {
    AlefConfig {
        crate_config: CrateConfig {
            name: "test-lib".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
            features: vec![],
            path_mappings: std::collections::HashMap::new(),
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
        wasm: Some(WasmConfig {
            exclude_functions: vec![],
            exclude_types: vec![],
            type_overrides: std::collections::HashMap::new(),
            features: None,
        }),
        ffi: None,
        go: None,
        java: None,
        csharp: None,
        r: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: alef_core::config::CustomModulesConfig::default(),
        custom_registrations: alef_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
        generate: alef_core::config::GenerateConfig::default(),
        generate_overrides: std::collections::HashMap::new(),
        dto: Default::default(),
        sync: None,
        test: None,
        e2e: None,
    }
}

#[test]
fn test_basic_generation() {
    let backend = WasmBackend;

    // Create test API surface with 1 TypeDef (2 fields), 1 FunctionDef, 1 EnumDef (2 variants)
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("enabled", TypeRef::Primitive(PrimitiveType::Bool), false),
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            doc: "Test configuration".to_string(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "process".to_string(),
            rust_path: "test_lib::process".to_string(),
            params: vec![ParamDef {
                name: "input".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
            }],
            return_type: TypeRef::String,
            is_async: false,
            error_type: None,
            doc: "Process input".to_string(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
        }],
        enums: vec![EnumDef {
            name: "Mode".to_string(),
            rust_path: "test_lib::Mode".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Fast".to_string(),
                    fields: vec![],
                    doc: "Fast mode".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "Accurate".to_string(),
                    fields: vec![],
                    doc: "Accurate mode".to_string(),
                    is_default: false,
                },
            ],
            doc: "Processing mode".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    let config = make_config();

    // Generate bindings
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok(), "Generation should succeed");
    let files = result.unwrap();

    // Should generate 1 lib.rs file
    assert_eq!(files.len(), 1, "Should generate one lib.rs file");

    let lib_file = &files[0];
    assert!(
        lib_file.path.to_string_lossy().ends_with("lib.rs"),
        "File should be lib.rs"
    );

    let content = &lib_file.content;

    // Assert content contains #[wasm_bindgen] markers
    assert!(
        content.contains("#[wasm_bindgen]"),
        "Content should contain #[wasm_bindgen] attribute"
    );

    // Assert struct generation with Js prefix
    assert!(
        content.contains("pub struct JsConfig"),
        "Should generate JsConfig struct"
    );

    // Assert enum generation with Js prefix
    assert!(content.contains("pub enum JsMode"), "Should generate JsMode enum");

    // Assert function binding
    assert!(content.contains("pub fn process"), "Should generate process function");
}

#[test]
fn test_type_mapping() {
    let backend = WasmBackend;

    // Create test API with various type fields
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "TypeTest".to_string(),
            rust_path: "test_lib::TypeTest".to_string(),
            fields: vec![
                make_field("u32_field", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("i64_field", TypeRef::Primitive(PrimitiveType::I64), false),
                make_field("string_field", TypeRef::String, false),
                make_field("opt_string", TypeRef::Optional(Box::new(TypeRef::String)), true),
                make_field("vec_string", TypeRef::Vec(Box::new(TypeRef::String)), false),
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            doc: "Type mapping test".to_string(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();

    let content = &files[0].content;

    // Should contain JsTypeTest struct
    assert!(content.contains("pub struct JsTypeTest"));

    // Should have #[wasm_bindgen] on struct
    assert!(content.contains("#[wasm_bindgen]"));

    // Should have fields for all types
    assert!(content.contains("u32_field"));
    assert!(content.contains("i64_field"));
    assert!(content.contains("string_field"));
    assert!(content.contains("opt_string"));
    assert!(content.contains("vec_string"));
}

#[test]
fn test_enum_generation() {
    let backend = WasmBackend;

    // Create test API with enum containing 3 variants
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![EnumDef {
            name: "Level".to_string(),
            rust_path: "test_lib::Level".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Low".to_string(),
                    fields: vec![],
                    doc: "Low level".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "Medium".to_string(),
                    fields: vec![],
                    doc: "Medium level".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "High".to_string(),
                    fields: vec![],
                    doc: "High level".to_string(),
                    is_default: false,
                },
            ],
            doc: "Severity levels".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();

    let content = &files[0].content;

    // Should contain JsLevel enum with #[wasm_bindgen]
    assert!(content.contains("#[wasm_bindgen]"));
    assert!(content.contains("pub enum JsLevel"));

    // Should have all variants
    assert!(content.contains("Low"));
    assert!(content.contains("Medium"));
    assert!(content.contains("High"));

    // Should have #[derive] for Copy
    assert!(content.contains("Copy"));
}

#[test]
fn test_generated_header() {
    let backend = WasmBackend;

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Data".to_string(),
            rust_path: "test_lib::Data".to_string(),
            fields: vec![make_field("value", TypeRef::String, false)],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            doc: String::new(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();

    // All generated files should have generated_header: false
    // (The builder adds the header to the content string, not the flag)
    for file in &files {
        assert!(
            !file.generated_header,
            "WASM backend should have generated_header: false"
        );
    }

    // But content should contain a generated header comment
    let content = &files[0].content;
    assert!(
        content.contains("generated by alef") || content.contains("DO NOT EDIT"),
        "Content should have a generated code marker"
    );
}
