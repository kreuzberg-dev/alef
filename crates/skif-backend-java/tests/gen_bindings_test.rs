use skif_backend_java::JavaBackend;
use skif_core::backend::Backend;
use skif_core::config::{CrateConfig, FfiConfig, JavaConfig, SkifConfig};
use skif_core::ir::{
    ApiSurface, EnumDef, EnumVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef, TypeRef,
};

#[test]
fn test_basic_generation() {
    let backend = JavaBackend;

    // Create test API surface
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "Config".to_string(),
            rust_path: "test_lib::Config".to_string(),
            fields: vec![FieldDef {
                name: "timeout".to_string(),
                ty: TypeRef::Primitive(PrimitiveType::U32),
                optional: false,
                default: None,
                doc: "Timeout in seconds".to_string(),
                sanitized: false,
            }],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            doc: "Test config".to_string(),
        }],
        functions: vec![FunctionDef {
            name: "extract".to_string(),
            rust_path: "test_lib::extract".to_string(),
            params: vec![ParamDef {
                name: "path".to_string(),
                ty: TypeRef::String,
                optional: false,
                default: None,
            }],
            return_type: TypeRef::String,
            is_async: false,
            error_type: Some("Error".to_string()),
            doc: "Extract text".to_string(),
        }],
        enums: vec![EnumDef {
            name: "Mode".to_string(),
            rust_path: "test_lib::Mode".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Fast".to_string(),
                    fields: vec![],
                    doc: "Fast mode".to_string(),
                },
                EnumVariant {
                    name: "Accurate".to_string(),
                    fields: vec![],
                    doc: "Accurate mode".to_string(),
                },
            ],
            doc: "Processing mode".to_string(),
        }],
        errors: vec![],
    };

    // Create test config
    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "test_lib".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
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
            prefix: Some("test".to_string()),
            error_style: "last_error".to_string(),
            header_name: None,
        }),
        go: None,
        java: Some(JavaConfig {
            package: Some("com.example".to_string()),
            ffi_style: "panama".to_string(),
        }),
        csharp: None,
        r: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
        adapters: vec![],
    };

    // Generate bindings
    let result = backend.generate_bindings(&api, &config);

    assert!(result.is_ok());
    let files = result.unwrap();

    // Should generate 5 files:
    // 1. NativeLib.java
    // 2. TestLib.java (main class)
    // 3. TestLibException.java
    // 4. Config.java (record)
    // 5. Mode.java (enum)
    assert_eq!(files.len(), 5);

    // Check NativeLib.java
    let native_lib = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("NativeLib"))
        .unwrap();
    assert!(native_lib.content.contains("class NativeLib"));
    assert!(native_lib.content.contains("TEST_EXTRACT"));
    assert!(native_lib.content.contains("MethodHandle"));

    // Check main class
    let main_class = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("TestLib.java"))
        .unwrap();
    assert!(main_class.content.contains("public final class TestLib"));
    assert!(main_class.content.contains("public static String extract"));
    assert!(main_class.content.contains("throws TestLibException"));

    // Check exception
    let exception = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("Exception"))
        .unwrap();
    assert!(
        exception
            .content
            .contains("public class TestLibException extends Exception")
    );
    assert!(exception.content.contains("private final int code"));

    // Check enum
    let enum_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("Mode"))
        .unwrap();
    assert!(enum_file.content.contains("public enum Mode"));
    assert!(enum_file.content.contains("Fast"));
    assert!(enum_file.content.contains("Accurate"));
}

#[test]
fn test_capabilities() {
    let backend = JavaBackend;
    let caps = backend.capabilities();

    assert!(caps.supports_async);
    assert!(caps.supports_classes);
    assert!(caps.supports_enums);
    assert!(caps.supports_option);
    assert!(caps.supports_result);
    assert!(!caps.supports_callbacks);
    assert!(!caps.supports_streaming);
}

#[test]
fn test_package_default() {
    let backend = JavaBackend;

    let api = ApiSurface {
        crate_name: "my_lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "my_lib".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
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
            prefix: None,
            error_style: "last_error".to_string(),
            header_name: None,
        }),
        go: None,
        java: None, // No explicit package
        csharp: None,
        r: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
        adapters: vec![],
    };

    let result = backend.generate_bindings(&api, &config);
    assert!(result.is_ok());

    let files = result.unwrap();
    let native_lib = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("NativeLib"))
        .unwrap();

    // Should use default package
    assert!(native_lib.content.contains("package dev.kreuzberg"));
}
