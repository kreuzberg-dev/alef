use skif_backend_csharp::CsharpBackend;
use skif_core::backend::Backend;
use skif_core::config::{CSharpConfig, CrateConfig, FfiConfig, SkifConfig};
use skif_core::ir::{
    ApiSurface, EnumDef, EnumVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef, TypeRef,
};

#[test]
fn test_basic_generation() {
    let backend = CsharpBackend;

    // Create test API surface
    let api = ApiSurface {
        crate_name: "kreuzberg".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "kreuzberg::Config".to_string(),
            fields: vec![
                FieldDef {
                    name: "timeout".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::U32),
                    optional: true,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "backend".to_string(),
                    ty: TypeRef::String,
                    optional: true,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            doc: "Extraction configuration".to_string(),
            cfg: None,
        }],
        functions: vec![FunctionDef {
            name: "extract_file_sync".to_string(),
            rust_path: "kreuzberg::extract_file_sync".to_string(),
            params: vec![
                ParamDef {
                    name: "path".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    sanitized: false,
                },
                ParamDef {
                    name: "config".to_string(),
                    ty: TypeRef::Named("Config".to_string()),
                    optional: true,
                    default: None,
                    sanitized: false,
                },
            ],
            return_type: TypeRef::String,
            is_async: false,
            error_type: Some("Error".to_string()),
            doc: "Extract text from file".to_string(),
            cfg: None,
            sanitized: false,
        }],
        enums: vec![EnumDef {
            name: "OcrBackend".to_string(),
            rust_path: "kreuzberg::OcrBackend".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Tesseract".to_string(),
                    fields: vec![],
                    doc: "Tesseract OCR".to_string(),
                },
                EnumVariant {
                    name: "PaddleOcr".to_string(),
                    fields: vec![],
                    doc: "PaddleOCR backend".to_string(),
                },
            ],
            doc: "Available OCR backends".to_string(),
            cfg: None,
        }],
        errors: vec![],
    };

    // Create test config
    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "kreuzberg".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
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
        wasm: None,
        ffi: Some(FfiConfig {
            prefix: Some("kreuzberg".to_string()),
            error_style: "last_error".to_string(),
            header_name: None,
            lib_name: None,
        }),
        go: None,
        java: None,
        csharp: Some(CSharpConfig {
            namespace: Some("Kreuzberg".to_string()),
            target_framework: None,
        }),
        r: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: skif_core::config::CustomModulesConfig::default(),
        custom_registrations: skif_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
    };

    // Generate bindings
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok(), "Generation should succeed");

    let files = result.unwrap();
    assert!(!files.is_empty(), "Should generate files");

    // Check for expected files
    let file_names: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();

    assert!(
        file_names.iter().any(|f| f.contains("NativeMethods.cs")),
        "Should generate NativeMethods.cs"
    );
    assert!(
        file_names.iter().any(|f| f.contains("KreuzbergException.cs")),
        "Should generate exception class"
    );
    assert!(
        file_names.iter().any(|f| f.contains("KreuzbergLib.cs")),
        "Should generate wrapper class"
    );
    assert!(
        file_names.iter().any(|f| f.contains("Config.cs")),
        "Should generate Config type"
    );
    assert!(
        file_names.iter().any(|f| f.contains("OcrBackend.cs")),
        "Should generate OcrBackend enum"
    );

    // Verify content of a generated file
    let native_methods = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("NativeMethods.cs"))
        .unwrap();
    assert!(native_methods.content.contains("DllImport"), "Should contain DllImport");
    assert!(
        native_methods.content.contains("NativeMethods"),
        "Should define NativeMethods class"
    );
    assert!(
        native_methods.content.contains("kreuzberg_ffi"),
        "Should reference kreuzberg_ffi library"
    );

    let wrapper = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("KreuzbergLib.cs"))
        .unwrap();
    assert!(
        wrapper.content.contains("public static class KreuzbergLib"),
        "Should define wrapper class"
    );
    assert!(
        wrapper.content.contains("ExtractFileSync"),
        "Should define wrapper method"
    );

    let config_type = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("Config.cs"))
        .unwrap();
    assert!(
        config_type.content.contains("public sealed class Config"),
        "Should define Config sealed class"
    );

    let enum_type = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("OcrBackend.cs"))
        .unwrap();
    assert!(
        enum_type.content.contains("public enum OcrBackend"),
        "Should define OcrBackend enum"
    );
}

#[test]
fn test_namespace_resolution() {
    let backend = CsharpBackend;

    let api = ApiSurface {
        crate_name: "my-lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "my-lib".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
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
        wasm: None,
        ffi: None,
        go: None,
        java: None,
        csharp: Some(CSharpConfig {
            namespace: Some("MyCompany.MyLib".to_string()),
            target_framework: None,
        }),
        r: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: skif_core::config::CustomModulesConfig::default(),
        custom_registrations: skif_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
    };

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();
    let file_names: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();

    // Should contain nested namespace
    assert!(
        file_names.iter().any(|f| f.contains("MyCompany/MyLib")),
        "Should create nested namespace directories"
    );
}

#[test]
fn test_generated_header() {
    let backend = CsharpBackend;

    let api = ApiSurface {
        crate_name: "test".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "test".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
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
        custom_modules: skif_core::config::CustomModulesConfig::default(),
        custom_registrations: skif_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
    };

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();

    // All files should have generated_header set to true
    for file in &files {
        assert!(
            file.generated_header,
            "All generated files should have generated_header=true"
        );
        assert!(
            file.content.contains("auto-generated"),
            "Content should contain auto-generated marker"
        );
    }
}

#[test]
fn test_type_mapping() {
    let backend = CsharpBackend;

    let api = ApiSurface {
        crate_name: "test".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Numbers".to_string(),
            rust_path: "test::Numbers".to_string(),
            fields: vec![
                FieldDef {
                    name: "u32_val".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::U32),
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "i64_val".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::I64),
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "string_val".to_string(),
                    ty: TypeRef::String,
                    optional: true,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "list_val".to_string(),
                    ty: TypeRef::Vec(Box::new(TypeRef::String)),
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            has_default: false,
            doc: String::new(),
            cfg: None,
        }],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "test".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
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
        custom_modules: skif_core::config::CustomModulesConfig::default(),
        custom_registrations: skif_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
    };

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();
    let numbers_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("Numbers.cs"))
        .unwrap();
    let content = &numbers_file.content;

    // Verify type mappings
    assert!(content.contains("uint U32Val"), "U32 should map to uint");
    assert!(content.contains("long I64Val"), "I64 should map to long");
    assert!(
        content.contains("string? StringVal"),
        "Optional string should be nullable"
    );
    assert!(
        content.contains("List<string> ListVal"),
        "Vec<String> should map to List<string>"
    );
}
