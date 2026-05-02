use alef_backend_wasm::WasmBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, WasmConfig};
use alef_core::ir::{
    ApiSurface, EnumDef, EnumVariant, ErrorDef, ErrorVariant, FieldDef, FunctionDef, MethodDef, ParamDef,
    PrimitiveType, ReceiverKind, TypeDef, TypeRef,
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
        newtype_wrapper: None,
    }
}

/// Helper to create minimal AlefConfig with WASM enabled
fn make_config() -> AlefConfig {
    AlefConfig {
        version: None,
        crate_config: CrateConfig {
            name: "test-lib".to_string(),
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
        wasm: Some(WasmConfig {
            exclude_functions: vec![],
            exclude_types: vec![],
            exclude_reexports: vec![],
            env_shims: vec![],
            type_overrides: std::collections::HashMap::new(),
            features: None,
            serde_rename_all: None,
            type_prefix: None,
            extra_dependencies: std::collections::HashMap::new(),
            rename_fields: Default::default(),
            run_wrapper: None,
            extra_lint_paths: Vec::new(),
            core_crate_override: None,
            exclude_extra_dependencies: Vec::new(),
            custom_rust_modules: Vec::new(),
            exclude_fields: std::collections::HashMap::new(),
            source_crate_remaps: Vec::new(),
        }),
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
        format: alef_core::config::FormatConfig::default(),
        format_overrides: std::collections::HashMap::new(),
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
            original_rust_path: String::new(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("enabled", TypeRef::Primitive(PrimitiveType::Bool), false),
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: "Test configuration".to_string(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "process".to_string(),
            rust_path: "test_lib::process".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "input".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: false,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            }],
            return_type: TypeRef::String,
            is_async: false,
            error_type: None,
            doc: "Process input".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![EnumDef {
            name: "Mode".to_string(),
            rust_path: "test_lib::Mode".to_string(),
            original_rust_path: String::new(),
            variants: vec![
                EnumVariant {
                    name: "Fast".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Fast mode".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "Accurate".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Accurate mode".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
            ],
            doc: "Processing mode".to_string(),
            cfg: None,
            is_copy: false,
            has_serde: false,
            serde_tag: None,
            serde_rename_all: None,
        }],
        errors: vec![],
    };

    let config = make_config();

    // Generate bindings
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok(), "Generation should succeed");
    let files = result.unwrap();

    // Should generate lib.rs + Cargo.toml
    assert_eq!(files.len(), 2, "Should generate lib.rs and Cargo.toml");

    let lib_file = &files[0];
    assert!(
        lib_file.path.to_string_lossy().ends_with("lib.rs"),
        "First file should be lib.rs"
    );

    let cargo_file = &files[1];
    assert!(
        cargo_file.path.to_string_lossy().ends_with("Cargo.toml"),
        "Second file should be Cargo.toml"
    );

    let content = &lib_file.content;

    // Assert content contains #[wasm_bindgen] markers
    assert!(
        content.contains("#[wasm_bindgen]"),
        "Content should contain #[wasm_bindgen] attribute"
    );

    // Assert struct generation with Wasm prefix
    assert!(
        content.contains("pub struct WasmConfig"),
        "Should generate Wasm-prefixed Config struct"
    );

    // Assert enum generation with Wasm prefix
    assert!(content.contains("pub enum WasmMode"), "Should generate WasmMode enum");

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
            original_rust_path: String::new(),
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
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
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

    // Should contain WasmTypeTest struct
    assert!(content.contains("pub struct WasmTypeTest"));

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
            original_rust_path: String::new(),
            variants: vec![
                EnumVariant {
                    name: "Low".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Low level".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "Medium".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Medium level".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "High".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "High level".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
            ],
            doc: "Severity levels".to_string(),
            cfg: None,
            is_copy: false,
            has_serde: false,
            serde_tag: None,
            serde_rename_all: None,
        }],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();

    let content = &files[0].content;

    // Should contain WasmLevel enum with #[wasm_bindgen]
    assert!(content.contains("#[wasm_bindgen]"));
    assert!(content.contains("pub enum WasmLevel"));

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
            original_rust_path: String::new(),
            fields: vec![make_field("value", TypeRef::String, false)],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
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

    // lib.rs has generated_header: false because the builder embeds the header into the
    // content string itself (via RustFileBuilder::with_generated_header).
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("must have lib.rs");
    assert!(
        !lib_file.generated_header,
        "lib.rs should have generated_header: false (header is embedded in content)"
    );

    // Cargo.toml uses generated_header: true so that write_files always regenerates it.
    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("must have Cargo.toml");
    assert!(
        cargo_file.generated_header,
        "Cargo.toml should have generated_header: true so it is always regenerated"
    );

    // lib.rs content should contain a generated header comment
    assert!(
        lib_file.content.contains("generated by alef") || lib_file.content.contains("DO NOT EDIT"),
        "lib.rs content should have a generated code marker"
    );
}

#[test]
fn test_async_function() {
    let backend = WasmBackend;

    // Create test API with an async function
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "fetch_data".to_string(),
            rust_path: "test_lib::fetch_data".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "url".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: false,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            }],
            return_type: TypeRef::String,
            is_async: true,
            error_type: None,
            doc: "Fetch data from URL".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();
    let content = &files[0].content;

    // Should contain async keyword in function
    assert!(content.contains("pub async fn fetch_data"));
    // Should contain .await call
    assert!(content.contains(".await"));
}

#[test]
fn test_async_function_with_error() {
    let backend = WasmBackend;

    // Create test API with an async function that returns Result
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "parse_json".to_string(),
            rust_path: "test_lib::parse_json".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "json".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: false,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            }],
            return_type: TypeRef::String,
            is_async: true,
            error_type: Some("ParseError".to_string()),
            doc: "Parse JSON".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();
    let content = &files[0].content;

    // Should contain async function
    assert!(content.contains("pub async fn parse_json"));
    // Should handle error with map_err
    assert!(content.contains("map_err"));
}

#[test]
fn test_methods_generation() {
    let backend = WasmBackend;

    // Create test API with a TypeDef that has methods
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Counter".to_string(),
            rust_path: "test_lib::Counter".to_string(),
            original_rust_path: String::new(),
            fields: vec![make_field("value", TypeRef::Primitive(PrimitiveType::U32), false)],
            methods: vec![
                MethodDef {
                    name: "increment".to_string(),
                    params: vec![],
                    return_type: TypeRef::Unit,
                    is_async: false,
                    is_static: false,
                    error_type: None,
                    doc: "Increment counter".to_string(),
                    receiver: Some(ReceiverKind::RefMut),
                    sanitized: false,
                    returns_ref: false,
                    returns_cow: false,
                    return_newtype_wrapper: None,
                    has_default_impl: false,
                    trait_source: None,
                },
                MethodDef {
                    name: "get_value".to_string(),
                    params: vec![],
                    return_type: TypeRef::Primitive(PrimitiveType::U32),
                    is_async: false,
                    is_static: false,
                    error_type: None,
                    doc: "Get current value".to_string(),
                    receiver: Some(ReceiverKind::Ref),
                    sanitized: false,
                    returns_ref: false,
                    returns_cow: false,
                    return_newtype_wrapper: None,
                    has_default_impl: false,
                    trait_source: None,
                },
            ],
            is_opaque: false,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: "A simple counter".to_string(),
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

    // Should generate Counter struct with Wasm prefix
    assert!(content.contains("pub struct WasmCounter"));

    // Should have methods
    assert!(content.contains("fn increment"));
    assert!(content.contains("fn get_value"));

    // Should have #[wasm_bindgen] on impl
    assert!(content.contains("#[wasm_bindgen]"));
}

#[test]
fn test_async_methods() {
    let backend = WasmBackend;

    // Create test API with an async method
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "AsyncWorker".to_string(),
            rust_path: "test_lib::AsyncWorker".to_string(),
            original_rust_path: String::new(),
            fields: vec![make_field("name", TypeRef::String, false)],
            methods: vec![MethodDef {
                name: "process".to_string(),
                params: vec![ParamDef {
                    name: "data".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                    is_ref: false,
                    is_mut: false,
                    newtype_wrapper: None,
                    original_type: None,
                }],
                return_type: TypeRef::String,
                is_async: true,
                is_static: false,
                error_type: None,
                doc: "Process data asynchronously".to_string(),
                receiver: Some(ReceiverKind::Ref),
                sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
                has_default_impl: false,
                trait_source: None,
            }],
            is_opaque: false,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: "Async worker".to_string(),
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

    // Should contain async method
    assert!(content.contains("pub async fn process"));
    // Should have .await
    assert!(content.contains(".await"));
}

#[test]
fn test_error_types() {
    let backend = WasmBackend;

    // Create test API with error definitions
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![ErrorDef {
            name: "ValidationError".to_string(),
            rust_path: "test_lib::ValidationError".to_string(),
            original_rust_path: String::new(),
            variants: vec![
                ErrorVariant {
                    name: "InvalidInput".to_string(),
                    fields: vec![],
                    doc: "Invalid input provided".to_string(),
                    message_template: Some("invalid input".to_string()),
                    has_source: false,
                    has_from: false,
                    is_unit: true,
                },
                ErrorVariant {
                    name: "OutOfRange".to_string(),
                    fields: vec![],
                    doc: "Value out of range".to_string(),
                    message_template: Some("value out of range".to_string()),
                    has_source: false,
                    has_from: false,
                    is_unit: true,
                },
            ],
            doc: "Validation errors".to_string(),
        }],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();
    let content = &files[0].content;

    // Should have error converter function
    assert!(
        content.contains("ValidationError") || content.contains("fn"),
        "Should contain error handling"
    );
}

#[test]
fn test_opaque_type() {
    let backend = WasmBackend;

    // Create test API with an opaque type
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "OpaqueHandle".to_string(),
            rust_path: "test_lib::OpaqueHandle".to_string(),
            original_rust_path: String::new(),
            fields: vec![],
            methods: vec![],
            is_opaque: true,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: "An opaque handle".to_string(),
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

    // Should generate opaque struct with Arc
    assert!(content.contains("Arc"));
    // Should have WasmOpaqueHandle struct
    assert!(content.contains("WasmOpaqueHandle"));
}

#[test]
fn test_exclude_functions() {
    let backend = WasmBackend;

    // Create test API with multiple functions
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![
            FunctionDef {
                name: "public_func".to_string(),
                rust_path: "test_lib::public_func".to_string(),
                original_rust_path: String::new(),
                params: vec![],
                return_type: TypeRef::String,
                is_async: false,
                error_type: None,
                doc: "Public function".to_string(),
                cfg: None,
                sanitized: false,
                return_sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
            },
            FunctionDef {
                name: "hidden_func".to_string(),
                rust_path: "test_lib::hidden_func".to_string(),
                original_rust_path: String::new(),
                params: vec![],
                return_type: TypeRef::String,
                is_async: false,
                error_type: None,
                doc: "Hidden function".to_string(),
                cfg: None,
                sanitized: false,
                return_sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
            },
        ],
        enums: vec![],
        errors: vec![],
    };

    // Create config with exclude_functions set
    let mut config = make_config();
    if let Some(wasm_cfg) = &mut config.wasm {
        wasm_cfg.exclude_functions = vec!["hidden_func".to_string()];
    }

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();
    let content = &files[0].content;

    // Should contain public_func
    assert!(content.contains("pub fn public_func"));

    // Should NOT contain hidden_func
    assert!(
        !content.contains("pub fn hidden_func"),
        "excluded function should not be in output"
    );
}

#[test]
fn test_exclude_types() {
    let backend = WasmBackend;

    // Create test API with multiple types
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "PublicType".to_string(),
                rust_path: "test_lib::PublicType".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("field", TypeRef::String, false)],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: "Public type".to_string(),
                cfg: None,
            },
            TypeDef {
                name: "HiddenType".to_string(),
                rust_path: "test_lib::HiddenType".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("secret", TypeRef::String, false)],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: "Hidden type".to_string(),
                cfg: None,
            },
        ],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    // Create config with exclude_types set
    let mut config = make_config();
    if let Some(wasm_cfg) = &mut config.wasm {
        wasm_cfg.exclude_types = vec!["HiddenType".to_string()];
    }

    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();
    let content = &files[0].content;

    // Should contain PublicType
    assert!(content.contains("pub struct WasmPublicType"));

    // Should NOT contain HiddenType
    assert!(
        !content.contains("WasmHiddenType"),
        "excluded type should not be in output"
    );
}

// ---------------------------------------------------------------------------
// WASM trait bridge helpers
// ---------------------------------------------------------------------------

fn make_trait_def_wasm(name: &str, methods: Vec<MethodDef>) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        rust_path: format!("my_lib::{name}"),
        original_rust_path: String::new(),
        fields: vec![],
        methods,
        is_opaque: false,
        is_clone: false,
        is_copy: false,
        is_trait: true,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
        doc: String::new(),
        cfg: None,
    }
}

fn make_method_wasm(name: &str, return_type: TypeRef, has_error: bool, has_default: bool) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        params: vec![],
        return_type,
        is_async: false,
        is_static: false,
        error_type: if has_error {
            Some("Box<dyn std::error::Error + Send + Sync>".to_string())
        } else {
            None
        },
        doc: String::new(),
        receiver: Some(ReceiverKind::Ref),
        sanitized: false,
        trait_source: None,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        has_default_impl: has_default,
    }
}

fn make_async_method_wasm(name: &str, return_type: TypeRef) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        params: vec![],
        return_type,
        is_async: true,
        is_static: false,
        error_type: Some("Box<dyn std::error::Error + Send + Sync>".to_string()),
        doc: String::new(),
        receiver: Some(ReceiverKind::Ref),
        sanitized: false,
        trait_source: None,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        has_default_impl: false,
    }
}

fn make_api_wasm() -> ApiSurface {
    ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    }
}

fn make_plugin_bridge_cfg_wasm(trait_name: &str) -> alef_core::config::TraitBridgeConfig {
    alef_core::config::TraitBridgeConfig {
        trait_name: trait_name.to_string(),
        super_trait: Some("Plugin".to_string()),
        registry_getter: Some("my_lib::get_registry".to_string()),
        register_fn: Some(format!("register_{}", trait_name.to_lowercase())),
        type_alias: None,
        param_name: None,
        register_extra_args: None,
        exclude_languages: Vec::new(),
        bind_via: alef_core::config::BridgeBinding::FunctionParam,
        options_type: None,
        options_field: None,
    }
}

fn make_visitor_bridge_cfg_wasm(trait_name: &str, type_alias: &str) -> alef_core::config::TraitBridgeConfig {
    alef_core::config::TraitBridgeConfig {
        trait_name: trait_name.to_string(),
        super_trait: None,
        registry_getter: None,
        register_fn: None,
        type_alias: Some(type_alias.to_string()),
        param_name: None,
        register_extra_args: None,
        exclude_languages: Vec::new(),
        bind_via: alef_core::config::BridgeBinding::FunctionParam,
        options_type: None,
        options_field: None,
    }
}

// ---------------------------------------------------------------------------
// WASM trait bridge tests
// ---------------------------------------------------------------------------

#[test]
fn test_wasm_visitor_bridge_produces_visitor_struct() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "HtmlVisitor",
        vec![make_method_wasm("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_wasm("HtmlVisitor", "HtmlVisitor");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("WasmHtmlVisitorBridge"),
        "WASM visitor bridge struct must be named Wasm{{TraitName}}Bridge"
    );
    assert!(
        code.code.contains("impl my_lib::HtmlVisitor for WasmHtmlVisitorBridge"),
        "WASM visitor bridge must implement the trait"
    );
}

#[test]
fn test_wasm_visitor_bridge_has_js_obj_field() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "HtmlVisitor",
        vec![make_method_wasm("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_wasm("HtmlVisitor", "HtmlVisitor");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("js_obj: wasm_bindgen::JsValue"),
        "WASM visitor bridge must store JsValue in a 'js_obj' field"
    );
}

#[test]
fn test_wasm_plugin_bridge_produces_wrapper_struct_with_inner_and_cached_name() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "OcrBackend",
        vec![make_method_wasm("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_wasm("OcrBackend");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("pub struct WasmOcrBackendBridge"),
        "WASM plugin bridge wrapper struct must be WasmOcrBackendBridge"
    );
    assert!(
        code.code.contains("inner:"),
        "WASM plugin bridge wrapper must have an 'inner' field"
    );
    assert!(
        code.code.contains("cached_name: String"),
        "WASM plugin bridge wrapper must have a 'cached_name: String' field"
    );
}

#[test]
fn test_wasm_plugin_bridge_generates_super_trait_impl() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "OcrBackend",
        vec![make_method_wasm("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_wasm("OcrBackend");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::Plugin for WasmOcrBackendBridge"),
        "WASM plugin bridge must implement Plugin super-trait"
    );
    assert!(code.code.contains("fn name("), "Plugin impl must contain name()");
    assert!(
        code.code.contains("fn initialize("),
        "Plugin impl must contain initialize()"
    );
    assert!(
        code.code.contains("fn shutdown("),
        "Plugin impl must contain shutdown()"
    );
}

#[test]
fn test_wasm_plugin_bridge_generates_trait_impl_with_forwarded_methods() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "OcrBackend",
        vec![make_method_wasm("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_wasm("OcrBackend");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::OcrBackend for WasmOcrBackendBridge"),
        "WASM plugin bridge must implement the trait itself"
    );
    assert!(
        code.code.contains("fn process("),
        "trait impl must forward the 'process' method"
    );
}

#[test]
fn test_wasm_plugin_bridge_generates_registration_fn_with_wasm_bindgen_attribute() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "OcrBackend",
        vec![make_method_wasm("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_wasm("OcrBackend");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("#[wasm_bindgen]"),
        "WASM registration function must carry the #[wasm_bindgen] attribute"
    );
    assert!(
        code.code.contains("pub fn register_ocrbackend("),
        "WASM registration function must use the configured name"
    );
}

#[test]
fn test_wasm_plugin_bridge_validates_required_methods() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm(
        "Analyzer",
        vec![
            make_method_wasm("analyze", TypeRef::String, true, false), // required
            make_method_wasm("describe", TypeRef::String, false, true), // optional
        ],
    );
    let bridge_cfg = alef_core::config::TraitBridgeConfig {
        trait_name: "Analyzer".to_string(),
        super_trait: Some("Plugin".to_string()),
        registry_getter: Some("my_lib::get_registry".to_string()),
        register_fn: Some("register_analyzer".to_string()),
        type_alias: None,
        param_name: None,
        register_extra_args: None,
        exclude_languages: Vec::new(),
        bind_via: alef_core::config::BridgeBinding::FunctionParam,
        options_type: None,
        options_field: None,
    };
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Registration fn must check for the required camelCase method "analyze"
    assert!(
        code.code.contains("\"analyze\""),
        "WASM registration fn must validate required method 'analyze'"
    );
}

#[test]
fn test_wasm_sync_method_body_uses_js_sys_reflect() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm("Scanner", vec![make_method_wasm("scan", TypeRef::String, true, false)]);
    let bridge_cfg = make_plugin_bridge_cfg_wasm("Scanner");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("js_sys::Reflect"),
        "WASM sync method body must use js_sys::Reflect to look up JS methods"
    );
}

#[test]
fn test_wasm_async_method_body_uses_box_pin() {
    use alef_backend_wasm::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_wasm("Processor", vec![make_async_method_wasm("run", TypeRef::Unit)]);
    let bridge_cfg = make_plugin_bridge_cfg_wasm("Processor");
    let api = make_api_wasm();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Plugin bridges use #[async_trait] which wraps in Box::pin automatically, so the method body
    // should NOT contain Box::pin(async move { ... }) — that would cause double-boxing.
    // Instead, the body should directly contain the method implementation.
    assert!(
        code.code.contains("let key = wasm_bindgen::JsValue::from_str"),
        "WASM async method body must contain JS reflection code"
    );
    assert!(
        !code.code.contains("Box::pin(async move"),
        "WASM async method body with #[async_trait] must NOT use Box::pin — #[async_trait] already wraps it"
    );
}

/// When `generate_bindings` runs, it must always include a Cargo.toml with `js-sys` as a
/// dependency. The trait-bridge and visitor-bridge generated code references `js_sys::Object`,
/// `js_sys::Reflect`, and `js_sys::Function` via full paths, so the crate won't compile without
/// that dependency. Including the Cargo.toml in the `generate_bindings` output (rather than only
/// in the scaffold) ensures the dep is always present even for projects whose Cargo.toml
/// pre-dates the scaffold template change that added `js-sys`.
#[test]
fn test_generate_bindings_cargo_toml_includes_js_sys() {
    let backend = WasmBackend;
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "1.2.3".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };
    let config = make_config();

    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed");

    // Should emit lib.rs + Cargo.toml
    assert_eq!(files.len(), 2, "generate_bindings must emit lib.rs and Cargo.toml");

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("generate_bindings must include a Cargo.toml");

    assert!(
        cargo_file.generated_header,
        "wasm Cargo.toml must have generated_header = true so it is always regenerated"
    );
    assert!(
        cargo_file.content.contains("js-sys"),
        "wasm Cargo.toml must include the js-sys dependency"
    );
    assert!(
        cargo_file.content.contains("wasm-bindgen"),
        "wasm Cargo.toml must include the wasm-bindgen dependency"
    );
    assert!(
        cargo_file.content.contains("serde-wasm-bindgen"),
        "wasm Cargo.toml must include the serde-wasm-bindgen dependency"
    );
    assert!(
        cargo_file.content.contains("[lib]"),
        "wasm Cargo.toml must have a [lib] section with crate-type = [\"cdylib\"]"
    );
    assert!(
        cargo_file.content.contains("cdylib"),
        "wasm Cargo.toml must declare crate-type = [\"cdylib\"]"
    );
}

/// The js-sys dep must be present even when trait bridges ARE configured, since that is
/// precisely the code path that emits `js_sys::Reflect`, `js_sys::Object`, etc.
#[test]
fn test_generate_bindings_cargo_toml_js_sys_with_trait_bridge() {
    use alef_core::config::TraitBridgeConfig;
    use alef_core::ir::{MethodDef, ReceiverKind};

    let backend = WasmBackend;
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.5.0".to_string(),
        types: vec![TypeDef {
            name: "Visitor".to_string(),
            rust_path: "test_lib::Visitor".to_string(),
            original_rust_path: String::new(),
            fields: vec![],
            methods: vec![MethodDef {
                name: "visit".to_string(),
                params: vec![],
                return_type: TypeRef::Unit,
                is_async: false,
                is_static: false,
                receiver: Some(ReceiverKind::Ref),
                error_type: None,
                doc: String::new(),
                sanitized: false,
                trait_source: None,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
                has_default_impl: false,
            }],
            is_opaque: false,
            is_clone: false,
            is_copy: false,
            is_trait: true,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: String::new(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let mut config = make_config();
    config.trait_bridges = vec![TraitBridgeConfig {
        trait_name: "Visitor".to_string(),
        super_trait: None,
        registry_getter: None,
        register_fn: Some("register_visitor".to_string()),
        type_alias: None,
        param_name: None,
        register_extra_args: None,
        exclude_languages: vec![],
        bind_via: alef_core::config::BridgeBinding::FunctionParam,
        options_type: None,
        options_field: None,
    }];

    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings with trait bridge should succeed");

    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("generate_bindings must include a Cargo.toml");

    assert!(
        cargo_file.content.contains("js-sys"),
        "Cargo.toml must include js-sys when trait bridge code is generated that uses js_sys::Reflect"
    );
}

#[test]
fn test_vec_string_is_ref_serde_path_emits_refs_binding() {
    // Regression test: functions with Vec<String> is_ref=true params that also have an
    // error_type (putting them into the serde path) must emit a `{name}_refs` let binding.
    // Without the fix, gen_call_args_with_let_bindings emits `&names_refs` but no binding
    // was created, causing E0425: cannot find value `names_refs` in this scope.
    let backend = WasmBackend;

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "download".to_string(),
            rust_path: "test_lib::download".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "names".to_string(),
                ty: TypeRef::Vec(Box::new(TypeRef::String)),
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: true,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            }],
            return_type: TypeRef::Primitive(PrimitiveType::Usize),
            is_async: false,
            error_type: Some("TestError".to_string()),
            doc: String::new(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed");

    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("generate_bindings must include lib.rs");

    assert!(
        lib_file.content.contains("let names_refs: Vec<&str>"),
        "generated lib.rs must create names_refs intermediate binding for Vec<String> is_ref=true;\n\
         actual content:\n{}",
        &lib_file.content[lib_file.content.find("fn download").unwrap_or(0)
            ..(lib_file.content.find("fn download").unwrap_or(0) + 300).min(lib_file.content.len())]
    );
    assert!(
        lib_file.content.contains("&names_refs"),
        "generated lib.rs must pass &names_refs to core function"
    );
}

/// Regression test: a non-opaque struct with `has_default: true` and a static `default()`
/// method returning `TypeRef::Named` with the same struct name must emit `.into()` so the
/// binding wrapper type (e.g. `WasmConversionOptions`) is returned, not the bare core type.
///
/// Before the fix, `wrap_return_with_mutex` skipped `.into()` when `n == type_name`, which
/// caused `fn default() -> WasmConversionOptions { core::ConversionOptions::default() }` —
/// a type mismatch compile error.
#[test]
fn test_static_default_returns_binding_wrapper_not_core_type() {
    let backend = WasmBackend;

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "ConversionOptions".to_string(),
            rust_path: "test_lib::options::ConversionOptions".to_string(),
            original_rust_path: String::new(),
            fields: vec![make_field("enabled", TypeRef::Primitive(PrimitiveType::Bool), false)],
            methods: vec![MethodDef {
                name: "default".to_string(),
                params: vec![],
                return_type: TypeRef::Named("ConversionOptions".to_string()),
                is_async: false,
                is_static: true,
                error_type: None,
                doc: String::new(),
                receiver: None,
                sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
                has_default_impl: false,
                trait_source: None,
            }],
            is_opaque: false,
            is_clone: true,
            is_copy: false,
            is_trait: false,
            has_default: true,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            has_serde: false,
            super_traits: vec![],
            doc: String::new(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed");

    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("generate_bindings must include lib.rs");

    let content = &lib_file.content;

    // The body must call the core default() and convert with .into() so the
    // Wasm-prefixed binding wrapper is returned, not the bare inner core type.
    // The wasm backend builds the core call as `{core_import}::{type_name}::method()`.
    assert!(
        content.contains("::ConversionOptions::default().into()"),
        "static default() must wrap core call with .into() to return binding wrapper;\n\
         actual content around fn default:\n{}",
        extract_fn_snippet(content, "fn default")
    );
}

/// Regression test: a static `from_update()` method on a non-opaque struct that takes a
/// `Named` param and returns `TypeRef::Named` with the same struct name must produce
/// a body ending in `.into()`, converting the core result to the binding wrapper.
#[test]
fn test_static_from_update_returns_binding_wrapper_not_core_type() {
    let backend = WasmBackend;

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "ConversionOptions".to_string(),
                rust_path: "test_lib::options::ConversionOptions".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("enabled", TypeRef::Primitive(PrimitiveType::Bool), false)],
                methods: vec![MethodDef {
                    name: "from_update".to_string(),
                    params: vec![ParamDef {
                        name: "update".to_string(),
                        ty: TypeRef::Named("ConversionOptionsUpdate".to_string()),
                        optional: false,
                        default: None,
                        sanitized: false,
                        typed_default: None,
                        is_ref: false,
                        is_mut: false,
                        newtype_wrapper: None,
                        original_type: None,
                    }],
                    return_type: TypeRef::Named("ConversionOptions".to_string()),
                    is_async: false,
                    is_static: true,
                    error_type: None,
                    doc: String::new(),
                    receiver: None,
                    sanitized: false,
                    returns_ref: false,
                    returns_cow: false,
                    return_newtype_wrapper: None,
                    has_default_impl: false,
                    trait_source: None,
                }],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: true,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: String::new(),
                cfg: None,
            },
            TypeDef {
                name: "ConversionOptionsUpdate".to_string(),
                rust_path: "test_lib::ConversionOptionsUpdate".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field(
                    "enabled",
                    TypeRef::Optional(Box::new(TypeRef::Primitive(PrimitiveType::Bool))),
                    true,
                )],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: String::new(),
                cfg: None,
            },
        ],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed");

    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("generate_bindings must include lib.rs");

    let content = &lib_file.content;

    // The body must convert the core result with .into() so the binding wrapper is returned.
    assert!(
        content.contains("ConversionOptions::from_update(update_core).into()"),
        "static from_update() must wrap core call with .into() to return binding wrapper;\n\
         actual content around fn from_update:\n{}",
        extract_fn_snippet(content, "fn from_update")
    );
}

/// `[wasm].core_crate_override` must redirect the Rust core dep to the named crate, and
/// `[wasm].exclude_extra_dependencies` must filter the listed keys out of the merged
/// `[crate.extra_dependencies]` set so the wasm binding can target a wasm-safe sub-crate
/// while siblings (e.g. http-only crates) are kept off the link line.
#[test]
fn test_wasm_core_crate_override_and_exclude_extra_dependencies() {
    let backend = WasmBackend;
    let api = ApiSurface {
        crate_name: "mylib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };
    let mut config = make_config();
    config.crate_config.name = "mylib".to_string();
    let mut crate_extras = std::collections::HashMap::new();
    crate_extras.insert("mylib-http".to_string(), toml::Value::String("1".to_string()));
    crate_extras.insert("mylib-graphql".to_string(), toml::Value::String("1".to_string()));
    config.crate_config.extra_dependencies = crate_extras;
    let wasm = config.wasm.as_mut().expect("wasm config seeded");
    wasm.core_crate_override = Some("mylib-core".to_string());
    wasm.exclude_extra_dependencies = vec!["mylib-http".to_string(), "mylib-graphql".to_string()];

    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed");
    let cargo_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("Cargo.toml"))
        .expect("generate_bindings must include a Cargo.toml");
    let content = &cargo_file.content;

    assert!(
        content.contains(r#"mylib-core = { path = "../mylib-core""#),
        "wasm Cargo.toml must depend on the override crate via path = \"../mylib-core\";\nactual:\n{content}"
    );
    assert!(
        !content.contains(r#"mylib = { path = "../mylib""#),
        "wasm Cargo.toml must not also depend on the umbrella crate when override is set;\nactual:\n{content}"
    );
    assert!(
        !content.contains("mylib-http"),
        "wasm Cargo.toml must filter out excluded extra dep `mylib-http`;\nactual:\n{content}"
    );
    assert!(
        !content.contains("mylib-graphql"),
        "wasm Cargo.toml must filter out excluded extra dep `mylib-graphql`;\nactual:\n{content}"
    );
    // The published package name must remain `<crate.name>-wasm` regardless of override.
    assert!(
        content.contains(r#"name = "mylib-wasm""#),
        "wasm Cargo.toml package name must remain `mylib-wasm` when override is set;\nactual:\n{content}"
    );
}

/// Lock in the contract for `Map<String, NamedStruct>` fields in the WASM backend.
///
/// The WASM backend sets `map_uses_jsvalue = true`, which causes the entire
/// `HashMap<K, V>` to round-trip through `serde_wasm_bindgen` as a `JsValue`.
/// This is intentional: wasm-bindgen cannot pass a Rust `HashMap` across the
/// JS/Wasm boundary directly.  The consequence is that Named types used as map
/// values MUST have symmetric `Serialize`/`Deserialize` impls; explicit `.into()`
/// is deliberately skipped because `serde_wasm_bindgen` serialises the whole map
/// as an opaque `JsValue`.  This test locks in that emission so a future refactor
/// cannot silently switch to a `.into_iter().map(|(k, v)| (k, v.into())).collect()`
/// pattern, which would fail to compile (the binding-wrapper type does not
/// implement `Serialize`/`Deserialize` in the generated code).
#[test]
fn test_map_named_value_uses_serde_wasm_bindgen_not_into() {
    let backend = WasmBackend;

    // ChildStruct: a Named type whose values appear inside the Map.
    let child = TypeDef {
        name: "ChildStruct".to_string(),
        rust_path: "test_lib::ChildStruct".to_string(),
        original_rust_path: String::new(),
        fields: vec![make_field("label", TypeRef::String, false)],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true,
        super_traits: vec![],
        doc: "A named child type used as map values.".to_string(),
        cfg: None,
    };

    // ParentStruct: has Map<String, ChildStruct> and Option<Map<String, ChildStruct>> fields.
    let parent = TypeDef {
        name: "ParentStruct".to_string(),
        rust_path: "test_lib::ParentStruct".to_string(),
        original_rust_path: String::new(),
        fields: vec![
            make_field(
                "children",
                TypeRef::Map(
                    Box::new(TypeRef::String),
                    Box::new(TypeRef::Named("ChildStruct".to_string())),
                ),
                false,
            ),
            make_field(
                "opt_children",
                TypeRef::Optional(Box::new(TypeRef::Map(
                    Box::new(TypeRef::String),
                    Box::new(TypeRef::Named("ChildStruct".to_string())),
                ))),
                true,
            ),
        ],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true,
        super_traits: vec![],
        doc: "A struct with Map<String, NamedStruct> fields.".to_string(),
        cfg: None,
    };

    // A function that accepts ParentStruct as a parameter forces the codegen to emit a
    // binding→core From impl for ParentStruct (only input types receive From<WasmX> for X).
    let process_fn = FunctionDef {
        name: "process_parent".to_string(),
        rust_path: "test_lib::process_parent".to_string(),
        original_rust_path: String::new(),
        params: vec![ParamDef {
            name: "parent".to_string(),
            ty: TypeRef::Named("ParentStruct".to_string()),
            optional: false,
            default: None,
            sanitized: false,
            typed_default: None,
            is_ref: false,
            is_mut: false,
            newtype_wrapper: None,
            original_type: None,
        }],
        return_type: TypeRef::Unit,
        is_async: false,
        error_type: None,
        doc: String::new(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
    };

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![child, parent],
        functions: vec![process_fn],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let files = backend
        .generate_bindings(&api, &config)
        .expect("generate_bindings should succeed for Map<String, Named> types");

    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("generate_bindings must include lib.rs");

    let content = &lib_file.content;

    // The generated From impl for ParentStruct → WasmParentStruct (core→binding) must use
    // serde_wasm_bindgen::to_value for the Map fields, NOT an iterator/into pattern.
    assert!(
        content.contains("serde_wasm_bindgen::to_value"),
        "Map<String, Named> core→binding conversion must use serde_wasm_bindgen::to_value;\n\
         actual content around 'children':\n{}",
        extract_field_snippet(content, "children")
    );

    // The generated From impl for WasmParentStruct → ParentStruct (binding→core) must use
    // serde_wasm_bindgen::from_value for the Map fields.
    assert!(
        content.contains("serde_wasm_bindgen::from_value"),
        "Map<String, Named> binding→core conversion must use serde_wasm_bindgen::from_value;\n\
         actual content around 'children':\n{}",
        extract_field_snippet(content, "children")
    );

    // Confirm the Map field is typed as JsValue in the binding struct, not HashMap.
    // The WasmParentStruct struct definition must not reference HashMap.
    let wasm_struct_start = content.find("pub struct WasmParentStruct").unwrap_or(0);
    let wasm_struct_end = content[wasm_struct_start..]
        .find('}')
        .map(|i| wasm_struct_start + i + 1)
        .unwrap_or(content.len());
    let struct_body = &content[wasm_struct_start..wasm_struct_end];
    assert!(
        !struct_body.contains("HashMap"),
        "WasmParentStruct must use JsValue for Map fields, not HashMap;\nstruct body:\n{struct_body}"
    );

    // Must not use an iterator-based pattern (.into_iter().collect()) for Map fields, as that
    // would require the binding-wrapper type to implement Into<CoreType> which it does not for
    // types converted via serde_wasm_bindgen.
    assert!(
        !content.contains("into_iter().collect()"),
        "Map<String, Named> conversion must not use into_iter().collect() — \
         Named values inside a Map must be converted via serde_wasm_bindgen as whole JsValue, \
         not per-element .into();\nactual content snippet:\n{}",
        extract_field_snippet(content, "children")
    );
}

/// Extract a ~300-char snippet around the first occurrence of `marker` for assertion messages.
fn extract_field_snippet<'a>(content: &'a str, marker: &str) -> &'a str {
    let start = content.find(marker).unwrap_or(0);
    let end = (start + 300).min(content.len());
    &content[start..end]
}

/// Extract a ~200-char snippet around the first occurrence of `marker` for assertion messages.
fn extract_fn_snippet<'a>(content: &'a str, marker: &str) -> &'a str {
    let start = content.find(marker).unwrap_or(0);
    let end = (start + 200).min(content.len());
    &content[start..end]
}

// ---------------------------------------------------------------------------
// Options-field bridge (bind_via = "options_field") tests — mirrors h2m shape
// ---------------------------------------------------------------------------

/// Build a `TraitBridgeConfig` for the `options_field` pattern used by h2m's HtmlVisitor.
fn make_options_field_bridge_cfg(
    trait_name: &str,
    type_alias: &str,
    options_type: &str,
    param_name: &str,
) -> alef_core::config::TraitBridgeConfig {
    alef_core::config::TraitBridgeConfig {
        trait_name: trait_name.to_string(),
        super_trait: None,
        registry_getter: None,
        register_fn: None,
        type_alias: Some(type_alias.to_string()),
        param_name: Some(param_name.to_string()),
        register_extra_args: None,
        exclude_languages: Vec::new(),
        bind_via: alef_core::config::BridgeBinding::OptionsField,
        options_type: Some(options_type.to_string()),
        options_field: None,
    }
}

/// Build an `ApiSurface` modelling `convert(html: &str, options: Option<ConversionOptions>)`
/// with `ConversionOptions` having a `visitor: Option<VisitorHandle>` field.
fn make_h2m_api() -> ApiSurface {
    let visitor_handle_field = FieldDef {
        name: "visitor".to_string(),
        ty: TypeRef::Optional(Box::new(TypeRef::Named("VisitorHandle".to_string()))),
        optional: true,
        default: None,
        doc: "Optional visitor".to_string(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: None,
        core_wrapper: alef_core::ir::CoreWrapper::None,
        vec_inner_core_wrapper: alef_core::ir::CoreWrapper::None,
        newtype_wrapper: None,
    };
    let parser_field = FieldDef {
        name: "parser".to_string(),
        ty: TypeRef::Optional(Box::new(TypeRef::String)),
        optional: true,
        default: None,
        doc: "Parser selection".to_string(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: None,
        core_wrapper: alef_core::ir::CoreWrapper::None,
        vec_inner_core_wrapper: alef_core::ir::CoreWrapper::None,
        newtype_wrapper: None,
    };
    let conversion_options = TypeDef {
        name: "ConversionOptions".to_string(),
        rust_path: "my_lib::ConversionOptions".to_string(),
        original_rust_path: String::new(),
        fields: vec![visitor_handle_field, parser_field],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: true,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
        doc: "Conversion options".to_string(),
        cfg: None,
    };
    let convert_fn = FunctionDef {
        name: "convert".to_string(),
        rust_path: "my_lib::convert".to_string(),
        original_rust_path: String::new(),
        params: vec![
            ParamDef {
                name: "html".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: true,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            },
            ParamDef {
                name: "options".to_string(),
                ty: TypeRef::Optional(Box::new(TypeRef::Named("ConversionOptions".to_string()))),
                optional: true,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: false,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            },
        ],
        return_type: TypeRef::String,
        is_async: false,
        error_type: Some("ConversionError".to_string()),
        doc: "Convert HTML to Markdown".to_string(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
    };
    ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![conversion_options],
        functions: vec![convert_fn],
        enums: vec![],
        errors: vec![],
    }
}

#[test]
fn test_options_field_bridge_visitor_field_typed_as_jsvalue() {
    // The WasmConversionOptions struct must expose a `visitor: Option<JsValue>` field,
    // not a `visitor: Option<WasmVisitorHandle>` (the type alias doesn't exist in WASM).
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // The binding struct must have a JsValue visitor field, not the opaque handle type.
    assert!(
        content.contains("Option<wasm_bindgen::JsValue>"),
        "visitor field on WasmConversionOptions must be Option<JsValue>;\ncontent snippet:\n{}",
        extract_field_snippet(content, "visitor")
    );
    // The opaque type alias must NOT appear as a field type.
    assert!(
        !content.contains("WasmVisitorHandle"),
        "WasmVisitorHandle must not appear as a binding struct field — visitor is bridged via JsValue"
    );
}

#[test]
fn test_options_field_bridge_from_impl_defaults_visitor() {
    // The From<WasmConversionOptions> for ConversionOptions impl must default the visitor
    // field (since the JsValue cannot be auto-converted), so it must not include a
    // `visitor: val.visitor` assignment.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // The From impl must exist (ConversionOptions is an input type).
    assert!(
        content.contains("impl From<WasmConversionOptions>"),
        "must generate From<WasmConversionOptions> impl"
    );
    // The impl must NOT assign `visitor: val.visitor` — it would try to convert JsValue
    // to Option<VisitorHandle> which is not possible automatically.
    let from_start = content.find("impl From<WasmConversionOptions>").unwrap_or(0);
    let from_end = content[from_start..].find("\nimpl ").map(|i| from_start + i).unwrap_or(content.len());
    let from_block = &content[from_start..from_end];
    assert!(
        !from_block.contains("visitor: val.visitor"),
        "From impl must not directly assign val.visitor (JsValue is not VisitorHandle);\nfrom block:\n{from_block}"
    );
}

#[test]
fn test_options_field_bridge_convert_fn_uses_bridge_wrapper() {
    // The generated `convert(html, options)` function must extract the JsValue from options
    // and build the WasmHtmlVisitorBridge, then attach it to core options before calling core.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // The convert function must be present.
    assert!(
        content.contains("pub fn convert("),
        "must generate convert function;\ncontent snippet:\n{}",
        extract_fn_snippet(content, "convert")
    );
    // It must construct the bridge wrapper from the extracted JsValue.
    assert!(
        content.contains("WasmHtmlVisitorBridge::new("),
        "convert must build WasmHtmlVisitorBridge from the JsValue visitor"
    );
    // It must attach the handle to core options.
    assert!(
        content.contains("visitor_js"),
        "convert must extract the visitor JsValue from options"
    );
}

#[test]
fn test_options_field_bridge_no_standalone_convert_with_visitor() {
    // When bind_via = "options_field", NO standalone `convert_with_visitor` function
    // should be generated — the visitor is embedded in ConversionOptions.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    assert!(
        !content.contains("convert_with_visitor"),
        "options_field mode must not emit a standalone convert_with_visitor function"
    );
}

#[test]
fn test_options_field_bridge_new_constructor_excludes_visitor() {
    // The WasmConversionOptions::new() constructor must not include a `visitor` parameter —
    // callers set it via the setter `set_visitor` after construction.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // Find the new() constructor body
    let new_fn_start = content.find("fn new(").unwrap_or(0);
    let new_fn_end = content[new_fn_start..]
        .find('}')
        .map(|i| new_fn_start + i + 1)
        .unwrap_or(content.len());
    let new_fn = &content[new_fn_start..new_fn_end];

    assert!(
        !new_fn.contains("visitor"),
        "WasmConversionOptions::new() must not include a visitor parameter;\nnew fn:\n{new_fn}"
    );

    // But a setter must be generated.
    assert!(
        content.contains("fn set_visitor("),
        "must generate set_visitor setter for bridge field"
    );
    // And a getter.
    assert!(
        content.contains("fn visitor(") || content.contains("fn visitor "),
        "must generate visitor getter for bridge field"
    );
}

#[test]
fn test_options_field_bridge_constructor_uses_default_for_bridge_fields() {
    // When bridge fields are excluded from the constructor params, the generated struct
    // literal must use `..Default::default()` so the compiler can fill in those fields.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // The constructor body must contain ..Default::default() to fill in the bridge field.
    let new_fn_start = content.find("fn new(").unwrap_or(0);
    let new_fn_end = content[new_fn_start..]
        .find('}')
        .map(|i| new_fn_start + i + 1)
        .unwrap_or(content.len());
    let new_fn = &content[new_fn_start..new_fn_end];

    assert!(
        new_fn.contains("..Default::default()"),
        "constructor must include ..Default::default() to fill bridge fields;\nnew fn:\n{new_fn}"
    );
}

#[test]
fn test_options_field_bridge_core_to_binding_emits_none_for_handle_field() {
    // The From<CoreConversionOptions> for WasmConversionOptions impl must emit `visitor: None`
    // for the bridge field — there is no way to convert a Rust VisitorHandle back to JsValue.
    let backend = WasmBackend;
    let api = make_h2m_api();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // Find the core→binding From impl
    let from_core_start = content.find("impl From<my_lib::ConversionOptions>").unwrap_or(0);
    let from_core_end = content[from_core_start..]
        .find("\nimpl ")
        .map(|i| from_core_start + i)
        .unwrap_or(content.len());
    let from_core_block = &content[from_core_start..from_core_end];

    assert!(
        from_core_block.contains("visitor: None"),
        "core→binding From impl must emit visitor: None for bridge field (JsValue cannot be constructed from Rust handle);\nfrom block:\n{from_core_block}"
    );
    // Must NOT emit a format! debug fallback which produces String not JsValue.
    assert!(
        !from_core_block.contains("format!"),
        "core→binding From impl must not use format! for visitor bridge field;\nfrom block:\n{from_core_block}"
    );
}

/// Build an API surface that includes an opaque Builder type with a `visitor` method
/// taking `Option<VisitorHandle>`. This mirrors `ConversionOptionsBuilder.visitor()`.
fn make_h2m_api_with_builder() -> ApiSurface {
    let mut api = make_h2m_api();
    let visitor_method = MethodDef {
        name: "visitor".to_string(),
        params: vec![ParamDef {
            name: "visitor".to_string(),
            ty: TypeRef::Optional(Box::new(TypeRef::Named("VisitorHandle".to_string()))),
            optional: true,
            default: None,
            sanitized: false,
            typed_default: None,
            is_ref: false,
            is_mut: false,
            newtype_wrapper: None,
            original_type: None,
        }],
        return_type: TypeRef::Named("ConversionOptionsBuilder".to_string()),
        is_async: false,
        is_static: false,
        error_type: None,
        doc: "Set visitor".to_string(),
        receiver: Some(ReceiverKind::Ref),
        sanitized: false,
        trait_source: None,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        has_default_impl: false,
    };
    let builder_type = TypeDef {
        name: "ConversionOptionsBuilder".to_string(),
        rust_path: "my_lib::ConversionOptionsBuilder".to_string(),
        original_rust_path: String::new(),
        fields: vec![],
        methods: vec![visitor_method],
        is_opaque: true,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
        doc: "Builder".to_string(),
        cfg: None,
    };
    api.types.push(builder_type);
    api
}

#[test]
fn test_options_field_bridge_suppresses_opaque_builder_visitor_method() {
    // When bind_via = "options_field" and the bridge type_alias is "VisitorHandle",
    // opaque builder methods whose params include VisitorHandle must be suppressed.
    // The JS caller uses set_visitor() on WasmConversionOptions instead.
    let backend = WasmBackend;
    let api = make_h2m_api_with_builder();
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg(
        "HtmlVisitor",
        "VisitorHandle",
        "ConversionOptions",
        "visitor",
    )];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let content = &files[0].content;

    // The WasmConversionOptionsBuilder must NOT expose a visitor() method,
    // since it would try to call self.inner.visitor(Option<&Arc<VisitorHandle>>)
    // which doesn't match the core signature Option<VisitorHandle>.
    let builder_impl_start = content.find("impl WasmConversionOptionsBuilder").unwrap_or(0);
    let builder_impl_end = content[builder_impl_start..]
        .find("\nimpl ")
        .map(|i| builder_impl_start + i)
        .unwrap_or(content.len());
    let builder_impl = &content[builder_impl_start..builder_impl_end];

    // The builder impl block must not contain the visitor method.
    assert!(
        !builder_impl.contains("fn visitor("),
        "options_field bridge must suppress builder methods whose params are bridge type aliases;\nbuilder impl:\n{builder_impl}"
    );
}
