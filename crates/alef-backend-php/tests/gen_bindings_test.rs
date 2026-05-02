use alef_backend_php::PhpBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, PhpConfig};
use alef_core::ir::*;

/// Helper to create a config with a specific extension name for namespace testing.
#[allow(dead_code)]
fn make_config_with_extension(extension_name: &str) -> AlefConfig {
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
        php: Some(PhpConfig {
            extension_name: Some(extension_name.to_string()),
            namespace: None,
            feature_gate: None,
            stubs: None,
            features: None,
            serde_rename_all: None,
            exclude_functions: vec![],
            exclude_types: vec![],
            extra_dependencies: Default::default(),
            scaffold_output: Default::default(),
            rename_fields: Default::default(),
            run_wrapper: None,
            extra_lint_paths: Vec::new(),
        }),
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
        format: alef_core::config::FormatConfig::default(),
        format_overrides: std::collections::HashMap::new(),
    }
}

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
        php: Some(PhpConfig {
            extension_name: Some("test_lib".to_string()),
            namespace: None,
            feature_gate: None,
            stubs: None,
            features: None,
            serde_rename_all: None,
            exclude_functions: vec![],
            exclude_types: vec![],
            extra_dependencies: Default::default(),
            scaffold_output: Default::default(),
            rename_fields: Default::default(),
            run_wrapper: None,
            extra_lint_paths: Vec::new(),
        }),
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
        format: alef_core::config::FormatConfig::default(),
        format_overrides: std::collections::HashMap::new(),
    }
}

#[test]
fn test_basic_generation() {
    let backend = PhpBackend;

    // Create test API surface
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            original_rust_path: String::new(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true),
                make_field("backend", TypeRef::String, true),
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
            doc: "Extraction configuration".to_string(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "extract_file_sync".to_string(),
            rust_path: "test_lib::extract_file_sync".to_string(),
            original_rust_path: String::new(),
            params: vec![
                ParamDef {
                    name: "path".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                    is_ref: false,
                    is_mut: false,
                    newtype_wrapper: None,
                    original_type: None,
                },
                ParamDef {
                    name: "config".to_string(),
                    ty: TypeRef::Named("Config".to_string()),
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
            error_type: Some("Error".to_string()),
            doc: "Extract text from file".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![EnumDef {
            name: "OcrBackend".to_string(),
            rust_path: "test_lib::OcrBackend".to_string(),
            original_rust_path: String::new(),
            variants: vec![
                EnumVariant {
                    name: "Tesseract".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Tesseract OCR".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "PaddleOcr".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "PaddleOCR backend".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
            ],
            doc: "Available OCR backends".to_string(),
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
    assert!(!files.is_empty(), "Should generate files");

    // Check for lib.rs file
    let file_names: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
    assert!(
        file_names.iter().any(|f| f.contains("lib.rs")),
        "Should generate lib.rs"
    );

    // Verify content contains PHP-specific markers
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    // Should contain #[php_class] for types
    assert!(
        lib_rs.content.contains("#[php_class]"),
        "Should contain #[php_class] marker for classes"
    );

    // Functions are generated as static methods in a *Api class (avoids inventory crate issue on macOS)
    assert!(
        lib_rs.content.contains("Api") && lib_rs.content.contains("#[php_impl]"),
        "Should contain Api class with #[php_impl] for functions"
    );

    // Should contain ext_php_rs imports
    assert!(lib_rs.content.contains("ext_php_rs"), "Should import ext_php_rs");
}

#[test]
fn test_type_mapping() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Numbers".to_string(),
            rust_path: "test_lib::Numbers".to_string(),
            original_rust_path: String::new(),
            fields: vec![
                make_field("u32_val", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("i64_val", TypeRef::Primitive(PrimitiveType::I64), false),
                make_field("string_val", TypeRef::String, true),
                make_field("opt_string", TypeRef::Optional(Box::new(TypeRef::String)), false),
                make_field("list_val", TypeRef::Vec(Box::new(TypeRef::String)), false),
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
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Should have proper field definitions with types
    assert!(content.contains("u32_val"), "Should contain u32_val field");
    assert!(content.contains("i64_val"), "Should contain i64_val field");
    assert!(content.contains("string_val"), "Should contain string_val field");
    assert!(
        content.contains("opt_string") || content.contains("Option"),
        "Should handle optional types"
    );
    assert!(
        content.contains("list_val") || content.contains("Vec"),
        "Should handle vec types"
    );
}

#[test]
fn test_enum_generation() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![EnumDef {
            name: "Status".to_string(),
            rust_path: "test_lib::Status".to_string(),
            original_rust_path: String::new(),
            variants: vec![
                EnumVariant {
                    name: "Pending".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Pending status".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "Active".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Active status".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
                EnumVariant {
                    name: "Inactive".to_string(),
                    fields: vec![],
                    is_tuple: false,
                    doc: "Inactive status".to_string(),
                    is_default: false,
                    serde_rename: None,
                },
            ],
            doc: "Processing status".to_string(),
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
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Enum should generate constants for PHP
    assert!(
        content.contains("Pending") && content.contains("Active") && content.contains("Inactive"),
        "Should contain all enum variants"
    );
}

#[test]
fn test_generated_header() {
    let backend = PhpBackend;

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

    // All files should have generated_header set to false (as per PHP backend code)
    for file in &files {
        assert!(
            !file.generated_header,
            "PHP backend files should have generated_header=false"
        );
    }
}

#[test]
fn test_methods_generation() {
    let backend = PhpBackend;

    // Create a type with methods
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Processor".to_string(),
            rust_path: "test_lib::Processor".to_string(),
            original_rust_path: String::new(),
            fields: vec![make_field("id", TypeRef::String, false)],
            methods: vec![
                MethodDef {
                    name: "process".to_string(),
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
                    is_static: false,
                    error_type: None,
                    doc: "Process input".to_string(),
                    receiver: Some(ReceiverKind::Ref),
                    sanitized: false,
                    returns_ref: false,
                    returns_cow: false,
                    return_newtype_wrapper: None,
                    has_default_impl: false,
                    trait_source: None,
                },
                MethodDef {
                    name: "from_id".to_string(),
                    params: vec![ParamDef {
                        name: "id".to_string(),
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
                    return_type: TypeRef::Named("Processor".to_string()),
                    is_async: false,
                    is_static: true,
                    error_type: None,
                    doc: "Create from ID".to_string(),
                    receiver: None,
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
            doc: "Text processor".to_string(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Method generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Check for #[php_impl] attribute for method blocks
    assert!(
        content.contains("#[php_impl]"),
        "Should contain #[php_impl] for method implementation"
    );

    // Check for method names in output
    assert!(content.contains("process"), "Should contain process method");
    assert!(content.contains("from_id"), "Should contain from_id static method");
}

#[test]
fn test_error_types() {
    let backend = PhpBackend;

    // Create error types with variants
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "risky_operation".to_string(),
            rust_path: "test_lib::risky_operation".to_string(),
            original_rust_path: String::new(),
            params: vec![],
            return_type: TypeRef::String,
            is_async: false,
            error_type: Some("ProcessError".to_string()),
            doc: "Operation that can fail".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![ErrorDef {
            name: "ProcessError".to_string(),
            rust_path: "test_lib::ProcessError".to_string(),
            original_rust_path: String::new(),
            variants: vec![
                ErrorVariant {
                    name: "NotFound".to_string(),
                    fields: vec![],
                    doc: "Resource not found".to_string(),
                    message_template: Some("resource not found".to_string()),
                    has_source: false,
                    has_from: false,
                    is_unit: true,
                },
                ErrorVariant {
                    name: "InvalidInput".to_string(),
                    fields: vec![make_field("reason", TypeRef::String, false)],
                    doc: "Invalid input provided".to_string(),
                    message_template: Some("invalid input: {reason}".to_string()),
                    has_source: false,
                    has_from: false,
                    is_unit: false,
                },
            ],
            doc: "Errors during processing".to_string(),
        }],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Error type generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Check that error converter function is generated
    assert!(
        content.contains("ProcessError") || content.contains("risky_operation"),
        "Should reference error type or function with error"
    );

    // Function with error_type should generate static method in Api class
    assert!(
        content.contains("risky_operation"),
        "Should generate method for function with error"
    );
}

#[test]
fn test_async_function() {
    let backend = PhpBackend;

    // Create an async function
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
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
            error_type: Some("FetchError".to_string()),
            doc: "Fetch data asynchronously".to_string(),
            cfg: None,
            sanitized: false,
            return_sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![ErrorDef {
            name: "FetchError".to_string(),
            rust_path: "test_lib::FetchError".to_string(),
            original_rust_path: String::new(),
            variants: vec![ErrorVariant {
                name: "NetworkError".to_string(),
                fields: vec![],
                doc: "Network error".to_string(),
                message_template: Some("network failure".to_string()),
                has_source: false,
                has_from: false,
                is_unit: true,
            }],
            doc: "Fetch error".to_string(),
        }],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Async function generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Async functions should generate a WORKER_RUNTIME for blocking
    assert!(
        content.contains("WORKER_RUNTIME") || content.contains("block_on") || content.contains("_async"),
        "Should contain async runtime support or _async function"
    );

    // Functions are generated as static methods in Api class
    assert!(
        content.contains("Api") && content.contains("#[php_impl]"),
        "Should contain Api class with #[php_impl] for async function"
    );
}

#[test]
fn test_opaque_type() {
    let backend = PhpBackend;

    // Create an opaque type
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Handle".to_string(),
            rust_path: "test_lib::Handle".to_string(),
            original_rust_path: String::new(),
            fields: vec![],
            methods: vec![MethodDef {
                name: "close".to_string(),
                params: vec![],
                return_type: TypeRef::Unit,
                is_async: false,
                is_static: false,
                error_type: None,
                doc: "Close the handle".to_string(),
                receiver: Some(ReceiverKind::Owned),
                sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
                has_default_impl: false,
                trait_source: None,
            }],
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
            doc: "Opaque handle to resource".to_string(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Opaque type generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Opaque types should have Arc import
    assert!(content.contains("std::sync::Arc"), "Should import Arc for opaque types");

    // Should contain #[php_class] for opaque type
    assert!(
        content.contains("#[php_class]") && content.contains("Handle"),
        "Should contain #[php_class] for opaque Handle type"
    );

    // Should contain method implementation
    assert!(
        content.contains("close"),
        "Should contain close method for opaque Handle"
    );
}

#[test]
fn test_default_config() {
    let backend = PhpBackend;

    // Create a type with has_default: true
    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            original_rust_path: String::new(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true),
                make_field("retries", TypeRef::Primitive(PrimitiveType::U32), true),
                make_field("verbose", TypeRef::Primitive(PrimitiveType::Bool), true),
            ],
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
            doc: "Configuration with defaults".to_string(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Default config generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Type with has_default: true should derive Default or have constructor with defaults
    assert!(
        content.contains("Default") || content.contains("__construct") || content.contains("#[derive"),
        "Should handle default configuration type"
    );

    // Should contain Config type definition
    assert!(content.contains("Config"), "Should contain Config type");
}

#[test]
fn test_multiple_types_with_shared_error() {
    let backend = PhpBackend;

    // Create multiple types and functions sharing an error type
    let shared_error = ErrorDef {
        name: "SharedError".to_string(),
        rust_path: "test_lib::SharedError".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            ErrorVariant {
                name: "IoError".to_string(),
                fields: vec![],
                doc: "I/O error".to_string(),
                message_template: Some("I/O failed".to_string()),
                has_source: false,
                has_from: false,
                is_unit: true,
            },
            ErrorVariant {
                name: "ParseError".to_string(),
                fields: vec![],
                doc: "Parse error".to_string(),
                message_template: Some("Parse failed".to_string()),
                has_source: false,
                has_from: false,
                is_unit: true,
            },
        ],
        doc: "Shared error type".to_string(),
    };

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "Reader".to_string(),
                rust_path: "test_lib::Reader".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("path", TypeRef::String, false)],
                methods: vec![MethodDef {
                    name: "read".to_string(),
                    params: vec![],
                    return_type: TypeRef::String,
                    is_async: false,
                    is_static: false,
                    error_type: Some("SharedError".to_string()),
                    doc: "Read file".to_string(),
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
                doc: "File reader".to_string(),
                cfg: None,
            },
            TypeDef {
                name: "Parser".to_string(),
                rust_path: "test_lib::Parser".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("format", TypeRef::String, false)],
                methods: vec![MethodDef {
                    name: "parse".to_string(),
                    params: vec![ParamDef {
                        name: "content".to_string(),
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
                    is_static: false,
                    error_type: Some("SharedError".to_string()),
                    doc: "Parse content".to_string(),
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
                doc: "Content parser".to_string(),
                cfg: None,
            },
        ],
        functions: vec![],
        enums: vec![],
        errors: vec![shared_error],
    };

    let config = make_config();

    let result = backend.generate_bindings(&api, &config);
    assert!(
        result.is_ok(),
        "Generation with multiple types sharing error should succeed"
    );

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    let content = &lib_rs.content;

    // Should contain both types
    assert!(
        content.contains("Reader") && content.contains("Parser"),
        "Should contain both Reader and Parser types"
    );

    // Should contain #[php_class] for both
    let php_class_count = content.matches("#[php_class]").count();
    assert!(php_class_count >= 2, "Should have #[php_class] for both types");

    // Error should be referenced in both methods
    assert!(
        content.contains("SharedError") || (content.contains("read") && content.contains("parse")),
        "Should reference shared error or contain both methods"
    );
}

#[test]
fn test_generate_type_stubs_contains_exception_and_api_class() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            original_rust_path: String::new(),
            fields: vec![make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true)],
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
        functions: vec![FunctionDef {
            name: "create_thing".to_string(),
            rust_path: "test_lib::create_thing".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "name".to_string(),
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
            return_type: TypeRef::Named("Config".to_string()),
            is_async: false,
            error_type: Some("Error".to_string()),
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
    let files = backend.generate_type_stubs(&api, &config).unwrap();

    assert!(!files.is_empty(), "Should generate stubs file");
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    // Exception class must extend \RuntimeException to satisfy PHPStan as Throwable
    assert!(
        content.contains("class TestLibException extends \\RuntimeException"),
        "Exception should extend \\RuntimeException; content:\n{content}"
    );

    // Api class must exist as a static method holder for free functions
    assert!(
        content.contains("class TestLibApi"),
        "Should generate TestLibApi class; content:\n{content}"
    );

    // Api class methods must have fully-qualified return types
    assert!(
        content.contains("createThing") || content.contains("create_thing"),
        "Should have createThing method in TestLibApi; content:\n{content}"
    );

    // Stubs should be namespaced correctly
    assert!(
        content.contains("namespace Test\\Lib"),
        "Should use Test\\Lib namespace; content:\n{content}"
    );
}

#[test]
fn test_generate_public_api_delegates_to_api_class() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "do_work".to_string(),
            rust_path: "test_lib::do_work".to_string(),
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
            error_type: Some("Error".to_string()),
            doc: "Do some work".to_string(),
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
    let files = backend.generate_public_api(&api, &config).unwrap();

    assert!(!files.is_empty(), "Should generate public API file");
    let facade = files.first().unwrap();
    let content = &facade.content;

    // The facade class must delegate to TestLibApi (not TestLib directly)
    assert!(
        content.contains("TestLibApi::doWork") || content.contains("TestLibApi::do_work"),
        "Facade should delegate to TestLibApi; content:\n{content}"
    );

    // @throws annotation must reference the exception class
    assert!(
        content.contains("@throws") && content.contains("TestLibException"),
        "Should have @throws annotation for TestLibException; content:\n{content}"
    );
}

#[test]
fn test_sanitized_function_generates_stub_not_direct_call() {
    // Regression test for functions whose return types were sanitized from unknown types
    // (e.g. tuples) to String/Vec<String>/Option<String>.  The PHP backend must NOT emit a
    // direct core call (which would be a type mismatch), but instead generate an unimplemented
    // stub body — consistent with the pyo3 and napi backends.
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![
            // Mimics `extension_ambiguity`: core returns Option<(&str, &[&str])>,
            // sanitized to Option<String> in the IR.
            FunctionDef {
                name: "extension_ambiguity".to_string(),
                rust_path: "test_lib::extension_ambiguity".to_string(),
                original_rust_path: String::new(),
                params: vec![ParamDef {
                    name: "ext".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                    is_ref: true,
                    is_mut: false,
                    newtype_wrapper: None,
                    original_type: None,
                }],
                return_type: TypeRef::Optional(Box::new(TypeRef::String)),
                is_async: false,
                error_type: None,
                doc: String::new(),
                cfg: None,
                sanitized: true,
                return_sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
            },
            // Mimics `split_code`: core returns Vec<(usize, usize)>,
            // sanitized to Vec<String> in the IR.
            FunctionDef {
                name: "split_code".to_string(),
                rust_path: "test_lib::split_code".to_string(),
                original_rust_path: String::new(),
                params: vec![ParamDef {
                    name: "source".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                    is_ref: true,
                    is_mut: false,
                    newtype_wrapper: None,
                    original_type: None,
                }],
                return_type: TypeRef::Vec(Box::new(TypeRef::String)),
                is_async: false,
                error_type: None,
                doc: String::new(),
                cfg: None,
                sanitized: true,
                return_sanitized: false,
                returns_ref: false,
                returns_cow: false,
                return_newtype_wrapper: None,
            },
        ],
        enums: vec![],
        errors: vec![],
    };

    let config = make_config();
    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // The generated bodies must NOT contain a direct delegating call to the core function.
    // Sanitized functions emit unimplemented stubs instead.
    assert!(
        !content.contains("test_lib::extension_ambiguity("),
        "extension_ambiguity must not delegate to core (type mismatch); content:\n{content}"
    );
    assert!(
        !content.contains("test_lib::split_code("),
        "split_code must not delegate to core (type mismatch); content:\n{content}"
    );

    // Sanitized functions without error_type must emit type-appropriate default values,
    // NOT PhpException stubs (which would be a type mismatch for non-Result return types).
    // extension_ambiguity returns Option<String>: stub must be `None`
    assert!(
        content.contains("None"),
        "extension_ambiguity (Option<String>, no Result) should emit `None` stub; content:\n{content}"
    );
    // split_code returns Vec<String>: stub must be `Vec::new()`
    assert!(
        content.contains("Vec::new()"),
        "split_code (Vec<String>, no Result) should emit `Vec::new()` stub; content:\n{content}"
    );
    // Neither must be wrapped in a PhpException Err
    assert!(
        !content.contains("Err(ext_php_rs::exception::PhpException::default(\"Not implemented: extension_ambiguity"),
        "extension_ambiguity must not emit PhpException (no error_type); content:\n{content}"
    );
    assert!(
        !content.contains("Err(ext_php_rs::exception::PhpException::default(\"Not implemented: split_code"),
        "split_code must not emit PhpException (no error_type); content:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// PHP trait bridge helpers
// ---------------------------------------------------------------------------

fn make_trait_def_php(name: &str, methods: Vec<MethodDef>) -> TypeDef {
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

fn make_method_php(name: &str, return_type: TypeRef, has_error: bool, has_default: bool) -> MethodDef {
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

fn make_async_method_php(name: &str, return_type: TypeRef) -> MethodDef {
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

fn make_api_php() -> ApiSurface {
    ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    }
}

fn make_plugin_bridge_cfg_php(trait_name: &str) -> alef_core::config::TraitBridgeConfig {
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

fn make_visitor_bridge_cfg_php(trait_name: &str, type_alias: &str) -> alef_core::config::TraitBridgeConfig {
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
// PHP trait bridge tests
// ---------------------------------------------------------------------------

#[test]
fn test_php_visitor_bridge_produces_visitor_struct() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("PhpHtmlVisitorBridge"),
        "PHP visitor bridge struct must be named Php{{TraitName}}Bridge"
    );
    assert!(
        code.code.contains("impl my_lib::HtmlVisitor for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement the trait"
    );
}

#[test]
fn test_php_visitor_bridge_has_php_obj_field() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("php_obj: *mut ext_php_rs::types::ZendObject"),
        "PHP visitor bridge must store a raw ZendObject pointer in 'php_obj'"
    );
    assert!(
        code.code.contains("cached_name: String"),
        "PHP visitor bridge must cache the plugin name"
    );
}

#[test]
fn test_php_plugin_bridge_produces_wrapper_struct_with_inner_and_cached_name() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("pub struct PhpOcrBackendBridge"),
        "PHP plugin bridge wrapper struct must be PhpOcrBackendBridge"
    );
    assert!(
        code.code.contains("inner:"),
        "PHP plugin bridge wrapper must have an 'inner' field"
    );
    assert!(
        code.code.contains("cached_name: String"),
        "PHP plugin bridge wrapper must have a 'cached_name: String' field"
    );
}

#[test]
fn test_php_plugin_bridge_generates_super_trait_impl() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::Plugin for PhpOcrBackendBridge"),
        "PHP plugin bridge must implement Plugin super-trait"
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
fn test_php_plugin_bridge_generates_trait_impl_with_forwarded_methods() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::OcrBackend for PhpOcrBackendBridge"),
        "PHP plugin bridge must implement the trait itself"
    );
    assert!(
        code.code.contains("fn process("),
        "trait impl must forward the 'process' method"
    );
}

#[test]
fn test_php_plugin_bridge_generates_registration_fn_with_php_function_attribute() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("#[php_function]"),
        "PHP registration function must carry the #[php_function] attribute"
    );
    assert!(
        code.code.contains("pub fn register_ocrbackend("),
        "PHP registration function must use the configured name"
    );
}

#[test]
fn test_php_plugin_bridge_validates_required_methods() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "Analyzer",
        vec![
            make_method_php("analyze", TypeRef::String, true, false), // required
            make_method_php("describe", TypeRef::String, false, true), // optional
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
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Registration fn must null-check the required method "analyze" via get_property
    assert!(
        code.code.contains("\"analyze\""),
        "PHP registration fn must validate required method 'analyze'"
    );
    assert!(
        code.code.contains("try_call_method"),
        "PHP registration fn must check method presence via try_call_method"
    );
}

#[test]
fn test_php_sync_method_body_uses_try_call_method() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php("Scanner", vec![make_method_php("scan", TypeRef::String, true, false)]);
    let bridge_cfg = make_plugin_bridge_cfg_php("Scanner");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("try_call_method"),
        "PHP sync method body must use try_call_method to dispatch to PHP"
    );
}

#[test]
fn test_php_async_method_body_uses_box_pin() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php("Processor", vec![make_async_method_php("run", TypeRef::Unit)]);
    let bridge_cfg = make_plugin_bridge_cfg_php("Processor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("WORKER_RUNTIME.block_on(async"),
        "PHP async method body must use WORKER_RUNTIME.block_on(async {{ ... }})"
    );
}

#[test]
fn test_php_visitor_bridge_has_send_sync_impls() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("unsafe impl Send for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement Send"
    );
    assert!(
        code.code.contains("unsafe impl Sync for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement Sync"
    );
}

/// Regression test: tagged data enums with tuple variants holding distinct inner types
/// must produce per-variant flat field names, not a shared `_0` field that collapses all
/// variant types to the first one.  Mirrors the `Message` enum in liter-llm:
///   System(SystemMessage), User(UserMessage), Assistant(AssistantMessage)
/// The flat struct must have distinct fields `system`, `user`, `assistant` (not `_0`).
/// The From impls must reference those per-variant field names.
#[test]
fn test_tagged_data_enum_tuple_variants_get_distinct_fields() {
    let backend = PhpBackend;

    let message_enum = EnumDef {
        name: "Message".to_string(),
        rust_path: "test_lib::Message".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            EnumVariant {
                name: "System".to_string(),
                fields: vec![make_field("_0", TypeRef::Named("SystemMessage".to_string()), false)],
                is_tuple: true,
                doc: String::new(),
                is_default: false,
                serde_rename: Some("system".to_string()),
            },
            EnumVariant {
                name: "User".to_string(),
                fields: vec![make_field("_0", TypeRef::Named("UserMessage".to_string()), false)],
                is_tuple: true,
                doc: String::new(),
                is_default: false,
                serde_rename: Some("user".to_string()),
            },
            EnumVariant {
                name: "Assistant".to_string(),
                fields: vec![make_field("_0", TypeRef::Named("AssistantMessage".to_string()), false)],
                is_tuple: true,
                doc: String::new(),
                is_default: false,
                serde_rename: Some("assistant".to_string()),
            },
        ],
        doc: "Chat message".to_string(),
        cfg: None,
        is_copy: false,
        has_serde: true,
        serde_tag: Some("role".to_string()),
        serde_rename_all: None,
    };

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![message_enum],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Generation should succeed: {:?}", result.err());

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // The flat struct must NOT have a shared `_0` field.
    assert!(
        !content.contains("pub _0:"),
        "Flat struct must not have a shared `_0` field; each tuple variant needs its own field"
    );

    // The flat struct must have per-variant fields named after the variant (snake_case).
    assert!(
        content.contains("pub system:"),
        "Flat struct must have `system` field for System variant; content:\n{content}"
    );
    assert!(
        content.contains("pub user:"),
        "Flat struct must have `user` field for User variant; content:\n{content}"
    );
    assert!(
        content.contains("pub assistant:"),
        "Flat struct must have `assistant` field for Assistant variant; content:\n{content}"
    );

    // Each field must carry a distinct type.
    assert!(
        content.contains("Option<SystemMessage>"),
        "Field `system` must have type Option<SystemMessage>; content:\n{content}"
    );
    assert!(
        content.contains("Option<UserMessage>"),
        "Field `user` must have type Option<UserMessage>; content:\n{content}"
    );
    assert!(
        content.contains("Option<AssistantMessage>"),
        "Field `assistant` must have type Option<AssistantMessage>; content:\n{content}"
    );

    // The core→binding From impl must assign per-variant fields.
    assert!(
        content.contains("system: Some(_0.into())"),
        "core→binding From impl must assign to `system`; content:\n{content}"
    );
    assert!(
        content.contains("user: Some(_0.into())"),
        "core→binding From impl must assign to `user`; content:\n{content}"
    );

    // The binding→core From impl must read from per-variant flat fields.
    assert!(
        content.contains("val.system.map(Into::into)"),
        "binding→core From impl must read from `val.system`; content:\n{content}"
    );
    assert!(
        content.contains("val.user.map(Into::into)"),
        "binding→core From impl must read from `val.user`; content:\n{content}"
    );
    assert!(
        content.contains("val.assistant.map(Into::into)"),
        "binding→core From impl must read from `val.assistant`; content:\n{content}"
    );

    // From impls must be present.
    assert!(
        content.contains("impl From<test_lib::Message> for Message"),
        "Must emit core→binding From impl; content:\n{content}"
    );
    assert!(
        content.contains("impl From<Message> for test_lib::Message"),
        "Must emit binding→core From impl; content:\n{content}"
    );
}

/// Regression test: tagged data enums (struct variants) must be lowered to flat PHP classes,
/// not string constants.  A `HashMap<String, DataEnum>` field on a struct must compile:
/// there must be a `From<core::DataEnum> for DataEnum` impl (not `From<DataEnum> for String`).
#[test]
fn test_tagged_data_enum_generates_flat_class_not_string_constants() {
    let backend = PhpBackend;

    let data_enum = EnumDef {
        name: "SecuritySchemeInfo".to_string(),
        rust_path: "test_lib::SecuritySchemeInfo".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            EnumVariant {
                name: "Http".to_string(),
                fields: vec![
                    make_field("scheme", TypeRef::String, false),
                    make_field("bearer_format", TypeRef::String, true),
                ],
                is_tuple: false,
                doc: String::new(),
                is_default: false,
                serde_rename: Some("http".to_string()),
            },
            EnumVariant {
                name: "ApiKey".to_string(),
                fields: vec![
                    make_field("location", TypeRef::String, false),
                    make_field("name", TypeRef::String, false),
                ],
                is_tuple: false,
                doc: String::new(),
                is_default: false,
                serde_rename: Some("apiKey".to_string()),
            },
        ],
        doc: "Security scheme types".to_string(),
        cfg: None,
        is_copy: false,
        has_serde: true,
        serde_tag: Some("type".to_string()),
        serde_rename_all: Some("lowercase".to_string()),
    };

    let config_type = TypeDef {
        name: "OpenApiConfig".to_string(),
        rust_path: "test_lib::OpenApiConfig".to_string(),
        original_rust_path: String::new(),
        fields: vec![make_field(
            "security_schemes",
            TypeRef::Map(
                Box::new(TypeRef::String),
                Box::new(TypeRef::Named("SecuritySchemeInfo".to_string())),
            ),
            false,
        )],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: true,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true,
        super_traits: vec![],
        doc: String::new(),
        cfg: None,
    };

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![config_type],
        functions: vec![],
        enums: vec![data_enum],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok(), "Generation should succeed");

    let files = result.unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Must NOT emit string constants for the data enum
    assert!(
        !content.contains("pub const SECURITYSCHEMEINFO_HTTP"),
        "Data enum must not generate string constants"
    );

    // Must emit a flat PHP class struct
    assert!(
        content.contains("pub struct SecuritySchemeInfo"),
        "Data enum must generate a flat PHP class struct"
    );

    // The struct must have a discriminator field named after the serde tag
    assert!(
        content.contains("type_tag"),
        "Flat struct must have a type_tag discriminator field"
    );

    // The struct must have variant fields
    assert!(content.contains("scheme"), "Flat struct must have scheme field");
    assert!(content.contains("location"), "Flat struct must have location field");

    // Must emit From<core::SecuritySchemeInfo> for SecuritySchemeInfo
    assert!(
        content.contains("impl From<test_lib::SecuritySchemeInfo> for SecuritySchemeInfo"),
        "Must emit core→binding From impl"
    );

    // Must emit From<SecuritySchemeInfo> for core::SecuritySchemeInfo
    assert!(
        content.contains("impl From<SecuritySchemeInfo> for test_lib::SecuritySchemeInfo"),
        "Must emit binding→core From impl"
    );

    // The HashMap field on OpenApiConfig must use the PHP class, not String
    assert!(
        content.contains("HashMap<String, SecuritySchemeInfo>"),
        "HashMap field must use the flat PHP class type, not String"
    );
}

#[test]
fn test_stubs_non_void_methods_have_return_statements() {
    // PHPStan at level 9 rejects non-void methods with empty `{ }` bodies.
    // All stub methods with non-void return types must emit a body that
    // satisfies the static analyser — `throw new \RuntimeException(...)`.
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            original_rust_path: String::new(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("label", TypeRef::String, true),
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
            doc: String::new(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "create_config".to_string(),
            rust_path: "test_lib::create_config".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "name".to_string(),
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
            return_type: TypeRef::Named("Config".to_string()),
            is_async: false,
            error_type: Some("Error".to_string()),
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
    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    // getErrorCode(): int must NOT be bare `{ }` — PHPStan rejects it.
    assert!(
        !content.contains("getErrorCode(): int { }"),
        "getErrorCode stub must not have an empty body `{{ }}`; content:\n{content}"
    );

    // Non-void getter stubs must contain a throw or return, not bare `{ }`.
    // The pattern `): int { }` or `): string { }` or `): ?string { }` are all wrong.
    assert!(
        !content.contains("): int { }"),
        "no non-void stub method may have an empty body `{{ }}`; content:\n{content}"
    );
    assert!(
        !content.contains("): string { }"),
        "no non-void stub method may have an empty body `{{ }}`; content:\n{content}"
    );
    assert!(
        !content.contains("): ?string { }"),
        "no non-void stub method may have an empty body `{{ }}`; content:\n{content}"
    );

    // The static method stub in the Api class must also not be empty.
    assert!(
        !content.contains("): \\Test\\Lib\\Config { }"),
        "Api class stub method must not have an empty body; content:\n{content}"
    );

    // Stubs should use throw to satisfy PHPStan.
    assert!(
        content.contains("throw new \\RuntimeException"),
        "stub bodies must throw \\RuntimeException to satisfy PHPStan level 9; content:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// options-field bridge tests
// ---------------------------------------------------------------------------

/// Helper: build a visitor trait def with all-default methods (visitor pattern).
fn make_visitor_trait_def() -> TypeDef {
    TypeDef {
        name: "HtmlVisitor".to_string(),
        rust_path: "my_lib::HtmlVisitor".to_string(),
        original_rust_path: String::new(),
        fields: vec![],
        methods: vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
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

/// Helper: ConversionOptions struct with a sanitized visitor field (simulating the IR
/// representation of `Rc<RefCell<dyn HtmlVisitor>>` → sanitized `Option<String>`).
fn make_conversion_options_type() -> TypeDef {
    TypeDef {
        name: "ConversionOptions".to_string(),
        rust_path: "my_lib::ConversionOptions".to_string(),
        original_rust_path: String::new(),
        fields: vec![
            make_field("parser", TypeRef::String, true),
            FieldDef {
                name: "visitor".to_string(),
                ty: TypeRef::Optional(Box::new(TypeRef::String)),
                optional: true,
                default: None,
                doc: String::new(),
                sanitized: true, // Rc<RefCell<dyn HtmlVisitor>> sanitized to Option<String>
                is_boxed: false,
                type_rust_path: None,
                cfg: None,
                typed_default: None,
                core_wrapper: CoreWrapper::None,
                vec_inner_core_wrapper: CoreWrapper::None,
                newtype_wrapper: None,
            },
        ],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: true,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true, // needed for serde round-trip in bridge body
        super_traits: vec![],
        doc: "HTML to Markdown conversion options".to_string(),
        cfg: None,
    }
}

/// Helper: convert function with options param (options_field bridge target).
fn make_convert_function() -> FunctionDef {
    FunctionDef {
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
    }
}

/// Helper: options-field bridge config for HtmlVisitor on ConversionOptions.visitor.
fn make_options_field_bridge_cfg() -> alef_core::config::TraitBridgeConfig {
    alef_core::config::TraitBridgeConfig {
        trait_name: "HtmlVisitor".to_string(),
        super_trait: None,
        registry_getter: None,
        register_fn: None,
        type_alias: Some("HtmlVisitor".to_string()),
        param_name: Some("visitor".to_string()),
        register_extra_args: None,
        exclude_languages: Vec::new(),
        bind_via: alef_core::config::BridgeBinding::OptionsField,
        options_type: Some("ConversionOptions".to_string()),
        options_field: Some("visitor".to_string()),
    }
}

/// Build a full ApiSurface for options-field bridge tests.
fn make_h2m_api() -> ApiSurface {
    ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![make_conversion_options_type(), make_visitor_trait_def()],
        functions: vec![make_convert_function()],
        enums: vec![],
        errors: vec![],
    }
}

/// Build a config with the options-field bridge active.
fn make_h2m_config() -> alef_core::config::AlefConfig {
    let mut cfg = make_config();
    cfg.trait_bridges = vec![make_options_field_bridge_cfg()];
    cfg
}

#[test]
fn test_options_field_bridge_convert_body_contains_bridge_wrap() {
    // When bind_via = "options_field", the generated convert function body must:
    // - NOT take a standalone visitor parameter in the same position as function_param mode
    // - Include bridge wrapping code that builds PhpHtmlVisitorBridge from the extra param
    // - Call core convert with the bridged options
    let backend = PhpBackend;
    let api = make_h2m_api();
    let config = make_h2m_config();

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Must contain the bridge wrapper creation pattern
    assert!(
        content.contains("PhpHtmlVisitorBridge::new"),
        "convert body must instantiate PhpHtmlVisitorBridge; content:\n{content}"
    );

    // Must contain Rc + RefCell wrapping to produce a VisitorHandle
    assert!(
        content.contains("std::rc::Rc::new(std::cell::RefCell::new"),
        "convert body must wrap bridge in Rc<RefCell>; content:\n{content}"
    );

    // Must call the core convert function
    assert!(
        content.contains("my_lib::convert(") || content.contains("convert("),
        "convert body must delegate to core; content:\n{content}"
    );
}

#[test]
fn test_options_field_bridge_no_standalone_convert_with_visitor() {
    // When bind_via = "options_field", there must be no standalone convertWithVisitor-style
    // function — only the regular convert that extracts the visitor from options.
    let backend = PhpBackend;
    let api = make_h2m_api();
    let config = make_h2m_config();

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // The function_param bridge path would emit a separate function with _with_visitor
    // or expose visitor as a standalone parameter.  options_field must NOT do this.
    assert!(
        !content.contains("convert_with_visitor"),
        "options_field bridge must not emit convertWithVisitor; content:\n{content}"
    );
}

#[test]
fn test_options_field_bridge_visitor_property_on_conversion_options_in_stubs() {
    // The generated PHP type stub for ConversionOptions must show:
    //   public ?HtmlVisitor $visitor;
    // instead of the sanitized `?string $visitor`.
    let backend = PhpBackend;
    let api = make_h2m_api();
    let config = make_h2m_config();

    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    // Visitor property must use the PHP bridge class name, not 'string'
    assert!(
        content.contains("public ?HtmlVisitor $visitor"),
        "ConversionOptions stub must show `public ?HtmlVisitor $visitor`; content:\n{content}"
    );

    // Must NOT show visitor as a string type
    assert!(
        !content.contains("public ?string $visitor") && !content.contains("public string $visitor"),
        "ConversionOptions stub must not render visitor as string; content:\n{content}"
    );
}

#[test]
fn test_options_field_bridge_convert_getter_override_in_stubs() {
    // The getVisitor() getter in the ConversionOptions stub must return ?HtmlVisitor, not ?string.
    let backend = PhpBackend;
    let api = make_h2m_api();
    let config = make_h2m_config();

    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    assert!(
        content.contains("getVisitor(): ?HtmlVisitor"),
        "ConversionOptions stub must have getVisitor(): ?HtmlVisitor getter; content:\n{content}"
    );
}

#[test]
fn test_options_field_bridge_public_api_passes_visitor_from_options() {
    // The generated PHP facade must pass $options->visitor as the extra arg when calling
    // the native extension method.
    let backend = PhpBackend;
    let api = make_h2m_api();
    let config = make_h2m_config();

    let files = backend.generate_public_api(&api, &config).unwrap();
    let facade = files.first().unwrap();
    let content = &facade.content;

    // The call to the native extension must include $options->visitor as the last argument.
    assert!(
        content.contains("$options->visitor") || content.contains("->visitor"),
        "PHP facade must pass $options->visitor to the native extension; content:\n{content}"
    );
}

// --- Tests for Named+optional=true param_is_optional and opaque field serde(skip) fixes ---

/// Helper: VisitorHandle opaque type (simulates the real IR where VisitorHandle is opaque).
fn make_visitor_handle_opaque_type() -> TypeDef {
    TypeDef {
        name: "VisitorHandle".to_string(),
        rust_path: "my_lib::visitor::VisitorHandle".to_string(),
        original_rust_path: String::new(),
        fields: vec![],
        methods: vec![],
        is_opaque: true,
        is_clone: false,
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
    }
}

/// Helper: ConversionOptions with opaque VisitorHandle field (real IR representation).
/// In the real IR, the visitor field has ty=Named("VisitorHandle") and optional=true,
/// unlike the sanitized representation used by make_conversion_options_type().
fn make_conversion_options_with_opaque_visitor() -> TypeDef {
    TypeDef {
        name: "ConversionOptions".to_string(),
        rust_path: "my_lib::ConversionOptions".to_string(),
        original_rust_path: String::new(),
        fields: vec![
            make_field("parser", TypeRef::String, true),
            make_field("visitor", TypeRef::Named("VisitorHandle".to_string()), true),
        ],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        is_trait: false,
        has_default: true,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true,
        super_traits: vec![],
        doc: "HTML to Markdown conversion options".to_string(),
        cfg: None,
    }
}

/// Helper: convert function where options param uses Named+optional=true (real IR pattern).
/// Distinct from make_convert_function() which wraps the type in Optional<Named>.
fn make_convert_function_named_optional() -> FunctionDef {
    FunctionDef {
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
                // Real IR representation: Named with optional=true (not Optional<Named>)
                ty: TypeRef::Named("ConversionOptions".to_string()),
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
        return_type: TypeRef::Named("ConversionResult".to_string()),
        is_async: false,
        error_type: Some("ConversionError".to_string()),
        doc: "Convert HTML to Markdown".to_string(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
    }
}

#[test]
fn test_opaque_visitor_field_gets_serde_skip() {
    // When ConversionOptions has a visitor field of type Named("VisitorHandle") where
    // VisitorHandle is an opaque type, the generated struct must add #[serde(skip)]
    // to prevent compile errors (VisitorHandle does not implement serde).
    let backend = PhpBackend;
    let api = ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![
            make_conversion_options_with_opaque_visitor(),
            make_visitor_handle_opaque_type(),
            make_visitor_trait_def(),
        ],
        functions: vec![make_convert_function_named_optional()],
        enums: vec![],
        errors: vec![],
    };
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg()];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // The visitor field in ConversionOptions must be annotated with #[serde(skip)]
    assert!(
        content.contains("#[serde(skip)]"),
        "opaque VisitorHandle field must get #[serde(skip)] to avoid serde compile errors; content:\n{content}"
    );
}

#[test]
fn test_named_optional_param_generates_let_mut_options_core() {
    // When options param is Named+optional=true (the real IR pattern), the generated
    // serde binding must declare `let mut options_core` so visitor_attach can call
    // options_core.as_mut() without a borrow error.
    let backend = PhpBackend;
    let api = ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![
            make_conversion_options_with_opaque_visitor(),
            make_visitor_handle_opaque_type(),
            make_visitor_trait_def(),
        ],
        functions: vec![make_convert_function_named_optional()],
        enums: vec![],
        errors: vec![],
    };
    let mut config = make_config();
    config.trait_bridges = vec![make_options_field_bridge_cfg()];

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Must use `let mut` so .as_mut() can be called to attach the visitor.
    assert!(
        content.contains("let mut options_core"),
        "options serde binding must declare `let mut options_core`; content:\n{content}"
    );

    // Must use the correct optional visitor_attach pattern.
    assert!(
        content.contains("options_core.as_mut()"),
        "visitor_attach must call options_core.as_mut() for optional options; content:\n{content}"
    );
}

#[test]
fn test_opaque_optional_param_in_builder_uses_deref_clone() {
    // When an opaque type is passed as an optional param to a builder method,
    // gen_php_call_args must emit `v.map(|v| (*v.inner).clone())` to unwrap the Arc
    // and obtain the owned core value, rather than `v.as_ref().map(|v| &v.inner)`.
    let visitor_builder_method = MethodDef {
        name: "visitor".to_string(),
        params: vec![ParamDef {
            name: "visitor".to_string(),
            ty: TypeRef::Named("VisitorHandle".to_string()),
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
        doc: "Set the visitor used during conversion.".to_string(),
        receiver: Some(ReceiverKind::Owned),
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
        methods: vec![visitor_builder_method],
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
        doc: String::new(),
        cfg: None,
    };
    let backend = PhpBackend;
    let api = ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![
            builder_type,
            make_visitor_handle_opaque_type(),
            make_visitor_trait_def(),
        ],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };
    let config = make_config();

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib_rs = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_rs.content;

    // Must deref the Arc to get owned core value — not pass a reference.
    assert!(
        content.contains("(*v.inner).clone()"),
        "optional opaque param in builder must use (*v.inner).clone() not &v.inner; content:\n{content}"
    );

    // Must NOT use the wrong reference pattern.
    assert!(
        !content.contains("as_ref().map(|v| &v.inner)"),
        "optional opaque param must not use as_ref().map(|v| &v.inner); content:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Bug-fix regression tests
// ---------------------------------------------------------------------------

/// Bug 1 regression: tagged data enums (struct variants) must emit `class` stubs, not
/// `enum X: string` — the `enum` form is a PHP 8.1 backed enum and does not match the
/// flat `#[php_class]` struct exposed at runtime.
#[test]
fn test_stubs_tagged_data_enum_emits_class_not_backed_enum() {
    let backend = PhpBackend;

    let data_enum = EnumDef {
        name: "NodeContent".to_string(),
        rust_path: "my_lib::NodeContent".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            EnumVariant {
                name: "Heading".to_string(),
                fields: vec![make_field("level", TypeRef::Primitive(PrimitiveType::U8), false)],
                is_tuple: false,
                doc: String::new(),
                is_default: false,
                serde_rename: None,
            },
            EnumVariant {
                name: "Paragraph".to_string(),
                fields: vec![],
                is_tuple: false,
                doc: String::new(),
                is_default: false,
                serde_rename: None,
            },
        ],
        doc: String::new(),
        cfg: None,
        is_copy: false,
        has_serde: true,
        serde_tag: Some("node_type".to_string()),
        serde_rename_all: None,
    };

    let api = ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![data_enum],
        errors: vec![],
    };

    let config = make_config();
    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    // Must NOT be a PHP backed enum
    assert!(
        !content.contains("enum NodeContent: string"),
        "tagged data enum must not emit a PHP backed enum; content:\n{content}"
    );
    // Must be a PHP class matching the flat struct shape
    assert!(
        content.contains("class NodeContent"),
        "tagged data enum must emit a PHP class; content:\n{content}"
    );
    // Discriminator field must exist
    assert!(
        content.contains("public string $node_type"),
        "tagged data enum class must have discriminator property; content:\n{content}"
    );
}

/// Bug 2 regression: visitor bridge must pass typed NodeContext PHP object (not raw array).
#[test]
fn test_visitor_bridge_passes_typed_node_context_object_not_array() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = TypeDef {
        name: "HtmlVisitor".to_string(),
        rust_path: "my_lib::visitor::HtmlVisitor".to_string(),
        original_rust_path: String::new(),
        fields: vec![],
        methods: vec![MethodDef {
            name: "visit_element_start".to_string(),
            params: vec![ParamDef {
                name: "_ctx".to_string(),
                ty: TypeRef::Named("NodeContext".to_string()),
                optional: false,
                default: None,
                sanitized: false,
                typed_default: None,
                is_ref: true,
                is_mut: false,
                newtype_wrapper: None,
                original_type: None,
            }],
            return_type: TypeRef::Named("VisitResult".to_string()),
            is_async: false,
            is_static: false,
            error_type: None,
            doc: String::new(),
            receiver: Some(ReceiverKind::RefMut),
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
            has_default_impl: true,
            trait_source: None,
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
    };

    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Must use nodecontext_to_php_object, not nodecontext_to_php_array
    assert!(
        code.code.contains("nodecontext_to_php_object"),
        "visitor bridge must call nodecontext_to_php_object; content:\n{}", code.code
    );
    assert!(
        !code.code.contains("nodecontext_to_php_array"),
        "visitor bridge must not use legacy nodecontext_to_php_array; content:\n{}", code.code
    );
    // The helper function must construct NodeContext { ... }
    assert!(
        code.code.contains("NodeContext {"),
        "nodecontext_to_php_object must construct binding NodeContext struct; content:\n{}", code.code
    );
    // Must use ZendClassObject to create a typed PHP object
    assert!(
        code.code.contains("ZendClassObject::<NodeContext>::new"),
        "nodecontext_to_php_object must use ZendClassObject to create typed object; content:\n{}", code.code
    );
}

/// Bug 3 regression: VisitResult dispatch must handle array shapes
/// `['custom' => '...']` → Custom(payload) and `['error' => '...']` → Error(payload).
#[test]
fn test_visitor_bridge_visit_result_handles_array_shapes() {
    use alef_backend_php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Named("VisitResult".to_string()), false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Must dispatch on ['custom' => '...'] to Custom(payload)
    assert!(
        code.code.contains("get(\"custom\")"),
        "visitor bridge must check array key 'custom'; content:\n{}", code.code
    );
    assert!(
        code.code.contains("::Custom("),
        "visitor bridge must map 'custom' array to Custom variant; content:\n{}", code.code
    );
    // Must dispatch on ['error' => '...'] to Error(payload)
    assert!(
        code.code.contains("get(\"error\")"),
        "visitor bridge must check array key 'error'; content:\n{}", code.code
    );
    assert!(
        code.code.contains("::Error("),
        "visitor bridge must map 'error' array to Error variant; content:\n{}", code.code
    );
    // Unknown strings must fall through to Continue (not Custom)
    assert!(
        !code.code.contains("_ => ::Custom(other.to_string())"),
        "unknown visitor return must not map to Custom; content:\n{}", code.code
    );
}

/// Bug 8 regression: {ClassName}Api stub methods with error_type must have @throws annotation.
#[test]
fn test_stubs_api_class_methods_have_throws_annotation() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "convert".to_string(),
            rust_path: "test_lib::convert".to_string(),
            original_rust_path: String::new(),
            params: vec![ParamDef {
                name: "html".to_string(),
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
            error_type: Some("ConversionError".to_string()),
            doc: "Convert HTML to markdown".to_string(),
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
    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    assert!(
        content.contains("@throws"),
        "Api class stub must have @throws annotation for methods with error_type; content:\n{content}"
    );
    assert!(
        content.contains("TestLibException"),
        "Api class @throws must reference the exception class; content:\n{content}"
    );
}

/// Bug 7 regression: is_return_type structs with no defaults should emit `readonly class` in stubs.
#[test]
fn test_stubs_return_types_emit_readonly_class() {
    let backend = PhpBackend;

    let api = ApiSurface {
        crate_name: "test-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "ConversionResult".to_string(),
                rust_path: "test_lib::ConversionResult".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("content", TypeRef::String, false)],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: true, // This is a result type
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: String::new(),
                cfg: None,
            },
            TypeDef {
                name: "ConversionOptions".to_string(),
                rust_path: "test_lib::ConversionOptions".to_string(),
                original_rust_path: String::new(),
                fields: vec![make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true)],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_copy: false,
                is_trait: false,
                has_default: true, // Options have defaults — NOT readonly
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
    let files = backend.generate_type_stubs(&api, &config).unwrap();
    let stubs = files.first().unwrap();
    let content = &stubs.content;

    // Return type must be readonly class
    assert!(
        content.contains("readonly class ConversionResult"),
        "is_return_type struct must emit readonly class; content:\n{content}"
    );
    // Input/options type must remain plain class
    assert!(
        !content.contains("readonly class ConversionOptions"),
        "has_default struct must NOT emit readonly class; content:\n{content}"
    );
    assert!(
        content.contains("class ConversionOptions"),
        "input type must emit plain class; content:\n{content}"
    );
}
