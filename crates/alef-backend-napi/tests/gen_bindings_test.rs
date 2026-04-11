use alef_backend_napi::NapiBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, NodeConfig};
use alef_core::ir::*;

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
    }
}

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
        node: Some(NodeConfig {
            package_name: Some("test-lib".to_string()),
            features: None,
        }),
        ruby: None,
        php: None,
        elixir: None,
        wasm: None,
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
    let backend = NapiBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true),
                make_field("backend", TypeRef::String, true),
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
            name: "extract_file".to_string(),
            rust_path: "test_lib::extract_file".to_string(),
            params: vec![
                ParamDef {
                    name: "path".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                },
                ParamDef {
                    name: "config".to_string(),
                    ty: TypeRef::Named("Config".to_string()),
                    optional: true,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                },
            ],
            return_type: TypeRef::String,
            is_async: false,
            error_type: Some("Error".to_string()),
            doc: "Extract text from file".to_string(),
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

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Generation should succeed");

    let files = result.unwrap();
    assert!(!files.is_empty(), "Should generate files");

    // Check for lib.rs file
    let lib_rs = files.iter().find(|f| f.path.to_string_lossy().ends_with("lib.rs"));
    assert!(lib_rs.is_some(), "Should generate lib.rs");

    let lib_rs_content = lib_rs.unwrap().content.as_str();

    // Assert NAPI markers are present
    assert!(
        lib_rs_content.contains("#[napi("),
        "Should contain #[napi(...)] attributes"
    );
    assert!(
        lib_rs_content.contains("napi_derive::napi"),
        "Should import napi_derive::napi"
    );
    assert!(
        lib_rs_content.contains("JsConfig"),
        "Should contain JsConfig type (Js-prefixed)"
    );
    assert!(
        lib_rs_content.contains("JsMode"),
        "Should contain JsMode enum (Js-prefixed)"
    );
    assert!(
        lib_rs_content.contains("extractFile"),
        "Should contain extractFile function (camelCase)"
    );
    assert!(
        lib_rs_content.contains("napi(object)"),
        "Non-opaque structs should use napi(object) attribute"
    );
    assert!(
        lib_rs_content.contains("napi(string_enum)"),
        "Enums should use napi(string_enum) attribute"
    );
}

#[test]
fn test_type_mapping() {
    let backend = NapiBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Numbers".to_string(),
            rust_path: "test_lib::Numbers".to_string(),
            fields: vec![
                make_field("u32_val", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("i64_val", TypeRef::Primitive(PrimitiveType::I64), false),
                make_field("string_val", TypeRef::String, true),
                make_field("string_list", TypeRef::Vec(Box::new(TypeRef::String)), false),
                make_field("opt_string", TypeRef::Optional(Box::new(TypeRef::String)), true),
            ],
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
    let lib_rs = files.iter().find(|f| f.path.to_string_lossy().ends_with("lib.rs"));
    assert!(lib_rs.is_some());

    let content = lib_rs.unwrap().content.as_str();

    // Verify the Numbers struct is defined with NAPI object attribute
    assert!(content.contains("Numbers"), "Should contain Numbers struct");
    assert!(
        content.contains("u32") || content.contains("u32_val"),
        "Should map u32 field"
    );
    assert!(
        content.contains("i64") || content.contains("i64_val"),
        "Should map i64 field"
    );
    assert!(content.contains("String"), "Should map String fields");
    assert!(
        content.contains("Vec") || content.contains("string_list"),
        "Should map Vec<String> field"
    );
}

#[test]
fn test_enum_generation() {
    let backend = NapiBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![EnumDef {
            name: "Status".to_string(),
            rust_path: "test_lib::Status".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Pending".to_string(),
                    fields: vec![],
                    doc: "Pending status".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "Active".to_string(),
                    fields: vec![],
                    doc: "Active status".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "Complete".to_string(),
                    fields: vec![],
                    doc: "Complete status".to_string(),
                    is_default: false,
                },
            ],
            doc: "Task status".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();
    let lib_rs = files.iter().find(|f| f.path.to_string_lossy().ends_with("lib.rs"));
    assert!(lib_rs.is_some());

    let content = lib_rs.unwrap().content.as_str();

    // Verify enum generation with NAPI string_enum attribute
    assert!(content.contains("Status"), "Should contain Status enum");
    assert!(content.contains("Pending"), "Should contain Pending variant");
    assert!(content.contains("Active"), "Should contain Active variant");
    assert!(content.contains("Complete"), "Should contain Complete variant");
    assert!(
        content.contains("napi(string_enum)"),
        "Should use napi(string_enum) attribute"
    );
}

#[test]
fn test_generated_header() {
    let backend = NapiBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();

    // Verify lib.rs has generated_header: false (as per source code)
    let lib_rs = files.iter().find(|f| f.path.to_string_lossy().ends_with("lib.rs"));
    assert!(lib_rs.is_some());

    let lib_rs_file = lib_rs.unwrap();
    assert!(
        !lib_rs_file.generated_header,
        "lib.rs should have generated_header: false"
    );
}
