use alef_backend_magnus::MagnusBackend;
use alef_core::backend::Backend;
use alef_core::config::{AlefConfig, CrateConfig, RubyConfig};
use alef_core::ir::*;
use std::collections::HashMap;

/// Helper to create a FieldDef with all defaults.
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

/// Helper to create a basic AlefConfig with Ruby enabled.
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
            path_mappings: HashMap::new(),
        },
        languages: vec![],
        exclude: Default::default(),
        include: Default::default(),
        output: Default::default(),
        python: None,
        node: None,
        ruby: Some(RubyConfig {
            gem_name: Some("test_lib".to_string()),
            stubs: None,
            features: None,
        }),
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
        opaque_types: HashMap::new(),
        generate: alef_core::config::GenerateConfig::default(),
        generate_overrides: HashMap::new(),
        dto: Default::default(),
        sync: None,
        test: None,
        e2e: None,
    }
}

#[test]
fn test_basic_generation() {
    let backend = MagnusBackend;

    // Create test API surface with types, functions, and enums
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            fields: vec![
                make_field("timeout", TypeRef::Primitive(PrimitiveType::U32), true),
                make_field("backend", TypeRef::String, false),
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            has_stripped_cfg_fields: false,
            is_return_type: false,
            serde_rename_all: None,
            doc: "Extraction configuration".to_string(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "process".to_string(),
            rust_path: "test_lib::process".to_string(),
            params: vec![
                ParamDef {
                    name: "input".to_string(),
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
            error_type: Some("ProcessError".to_string()),
            doc: "Process input with config".to_string(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
        }],
        enums: vec![EnumDef {
            name: "Backend".to_string(),
            rust_path: "test_lib::Backend".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Tesseract".to_string(),
                    fields: vec![],
                    doc: "Tesseract OCR".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "PaddleOcr".to_string(),
                    fields: vec![],
                    doc: "PaddleOCR backend".to_string(),
                    is_default: false,
                },
            ],
            doc: "Available backends".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok(), "Generation should succeed");

    let files = result.unwrap();
    assert!(!files.is_empty(), "Should generate at least one file");

    // Check for expected file
    let file_names: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
    assert!(
        file_names.iter().any(|f| f.contains("lib.rs")),
        "Should generate lib.rs file"
    );

    // Verify content contains Magnus-specific markers
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_file.content;

    // Check for Magnus imports and macros
    assert!(
        content.contains("magnus::wrap"),
        "Should contain magnus::wrap attribute"
    );
    assert!(
        content.contains("IntoValue"),
        "Should contain IntoValue trait implementation"
    );
    assert!(
        content.contains("TryConvert"),
        "Should contain TryConvert trait implementation"
    );
    assert!(
        content.contains("TryConvertOwned"),
        "Should contain TryConvertOwned marker trait"
    );

    // Check for struct generation
    assert!(content.contains("struct Config"), "Should generate Config struct");

    // Check for enum generation
    assert!(content.contains("enum Backend"), "Should generate Backend enum");
    assert!(content.contains("Tesseract"), "Should contain Tesseract variant");
    assert!(content.contains("PaddleOcr"), "Should contain PaddleOcr variant");

    // Check for function/method generation
    assert!(content.contains("process"), "Should contain process function");
}

#[test]
fn test_type_mapping() {
    let backend = MagnusBackend;

    // Create API with various field types to test type mapping
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Numbers".to_string(),
            rust_path: "test_lib::Numbers".to_string(),
            fields: vec![
                make_field("u32_val", TypeRef::Primitive(PrimitiveType::U32), false),
                make_field("i64_val", TypeRef::Primitive(PrimitiveType::I64), false),
                make_field("string_val", TypeRef::String, true),
                make_field("vec_val", TypeRef::Vec(Box::new(TypeRef::String)), false),
                make_field("option_val", TypeRef::Optional(Box::new(TypeRef::String)), true),
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
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_file.content;

    // Check that struct is generated with proper field types
    assert!(content.contains("struct Numbers"), "Should generate Numbers struct");

    // Verify Magnus-specific type wrapping
    assert!(content.contains("magnus::wrap"), "Should have magnus::wrap attribute");
}

#[test]
fn test_enum_generation() {
    let backend = MagnusBackend;

    // Create API with a more complex enum
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
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
                    name: "Processing".to_string(),
                    fields: vec![],
                    doc: "Processing status".to_string(),
                    is_default: false,
                },
                EnumVariant {
                    name: "Complete".to_string(),
                    fields: vec![],
                    doc: "Complete status".to_string(),
                    is_default: false,
                },
            ],
            doc: "Processing status".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    let config = make_config();
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());

    let files = result.unwrap();
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();
    let content = &lib_file.content;

    // Check enum definition
    assert!(content.contains("enum Status"), "Should generate Status enum");
    assert!(content.contains("Pending"), "Should contain Pending variant");
    assert!(content.contains("Processing"), "Should contain Processing variant");
    assert!(content.contains("Complete"), "Should contain Complete variant");

    // Check for conversion traits (IntoValue, TryConvert)
    assert!(
        content.contains("impl magnus::IntoValue for Status"),
        "Should implement IntoValue for enum"
    );
    assert!(
        content.contains("impl magnus::TryConvert for Status"),
        "Should implement TryConvert for enum"
    );

    // Check for symbol conversion (Ruby symbols)
    assert!(content.contains("to_symbol"), "Should convert to Ruby symbols");
}

#[test]
fn test_generated_header() {
    let backend = MagnusBackend;

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Simple".to_string(),
            rust_path: "test_lib::Simple".to_string(),
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

    // Check that main lib.rs has auto-generated header (set by with_generated_header())
    let lib_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("lib.rs"))
        .unwrap();

    // The content should include the auto-generated marker from RustFileBuilder::with_generated_header()
    assert!(
        lib_file.content.contains("Code generated")
            || lib_file.content.contains("auto-generated")
            || lib_file.content.contains("DO NOT EDIT"),
        "Generated file should have an auto-generated header comment"
    );
}
