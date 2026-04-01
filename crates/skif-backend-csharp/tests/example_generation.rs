use skif_backend_csharp::CsharpBackend;
use skif_core::backend::Backend;
use skif_core::config::{CSharpConfig, CrateConfig, FfiConfig, SkifConfig};
use skif_core::ir::*;

#[test]
fn test_generated_code_example() {
    let backend = CsharpBackend;

    let api = ApiSurface {
        crate_name: "kreuzberg".to_string(),
        version: "0.1.0".to_string(),
        types: vec![TypeDef {
            name: "ExtractionConfig".to_string(),
            rust_path: "kreuzberg::ExtractionConfig".to_string(),
            fields: vec![
                FieldDef {
                    name: "ocr_backend".to_string(),
                    ty: TypeRef::String,
                    optional: true,
                    default: None,
                    doc: "OCR backend to use".to_string(),
                    sanitized: false,
                },
                FieldDef {
                    name: "timeout".to_string(),
                    ty: TypeRef::Primitive(PrimitiveType::U64),
                    optional: true,
                    default: None,
                    doc: "Timeout in milliseconds".to_string(),
                    sanitized: false,
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            doc: "Configuration for text extraction".to_string(),
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
                },
                ParamDef {
                    name: "config".to_string(),
                    ty: TypeRef::Optional(Box::new(TypeRef::Named("ExtractionConfig".to_string()))),
                    optional: true,
                    default: None,
                },
            ],
            return_type: TypeRef::String,
            is_async: false,
            error_type: Some("Error".to_string()),
            doc: "Extract text from a file synchronously".to_string(),
        }],
        enums: vec![EnumDef {
            name: "OcrBackend".to_string(),
            rust_path: "kreuzberg::OcrBackend".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Tesseract".to_string(),
                    fields: vec![],
                    doc: "Tesseract OCR engine".to_string(),
                },
                EnumVariant {
                    name: "PaddleOcr".to_string(),
                    fields: vec![],
                    doc: "PaddleOCR engine".to_string(),
                },
            ],
            doc: "Available OCR backends".to_string(),
        }],
        errors: vec![],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "kreuzberg".to_string(),
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
            prefix: Some("kreuzberg".to_string()),
            error_style: "last_error".to_string(),
            header_name: None,
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
    };

    let files = backend.generate_bindings(&api, &config).unwrap();

    // NativeMethods.cs should contain P/Invoke declarations
    let native_methods = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("NativeMethods.cs"))
        .unwrap();

    assert!(native_methods.content.contains("[DllImport(LibName"));
    assert!(native_methods.content.contains("kreuzberg_extract_file_sync"));
    assert!(native_methods.content.contains("internal static extern"));
    assert!(native_methods.content.contains("kreuzberg_last_error_code"));
    assert!(native_methods.content.contains("kreuzberg_last_error_context"));
    assert!(native_methods.content.contains("kreuzberg_free_string"));

    // Exception class should be properly defined
    let exception = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("KreuzbergException.cs"))
        .unwrap();

    assert!(
        exception
            .content
            .contains("public class KreuzbergException : Exception")
    );
    assert!(exception.content.contains("public int Code { get; }"));
    assert!(exception.content.contains("namespace Kreuzberg"));

    // Wrapper class should have extraction methods
    let wrapper = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("KreuzbergLib.cs"))
        .unwrap();

    assert!(wrapper.content.contains("public static class KreuzbergLib"));
    assert!(wrapper.content.contains("public static string ExtractFileSync"));
    assert!(wrapper.content.contains("NativeMethods."));
    assert!(wrapper.content.contains("GetLastError()"));

    // Type definition should use records
    let config_type = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("ExtractionConfig.cs"))
        .unwrap();

    assert!(config_type.content.contains("public record ExtractionConfig"));
    assert!(config_type.content.contains("string? OcrBackend"));
    assert!(config_type.content.contains("ulong? Timeout"));
    assert!(config_type.content.contains("Configuration for text extraction"));

    // Enum definition
    let enum_type = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("OcrBackend.cs"))
        .unwrap();

    assert!(enum_type.content.contains("public enum OcrBackend"));
    assert!(enum_type.content.contains("Tesseract,"));
    assert!(enum_type.content.contains("PaddleOcr,"));
    assert!(enum_type.content.contains("Available OCR backends"));
}
