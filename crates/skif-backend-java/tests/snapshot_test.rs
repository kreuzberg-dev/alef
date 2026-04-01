/// Test that generates and displays sample Java code output.
use skif_backend_java::JavaBackend;
use skif_core::backend::Backend;
use skif_core::config::{CrateConfig, FfiConfig, JavaConfig, SkifConfig};
use skif_core::ir::{
    ApiSurface, EnumDef, EnumVariant, ErrorDef, ErrorVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef,
    TypeRef,
};

#[test]
#[ignore] // Run with: cargo test -- --ignored --nocapture
fn print_generated_java_code() {
    let backend = JavaBackend;

    // Create a comprehensive test API surface
    let api = ApiSurface {
        crate_name: "kreuzberg".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "ExtractionConfig".to_string(),
                rust_path: "kreuzberg::ExtractionConfig".to_string(),
                fields: vec![
                    FieldDef {
                        name: "ocrBackend".to_string(),
                        ty: TypeRef::String,
                        optional: false,
                        default: Some("\"tesseract\"".to_string()),
                        doc: "OCR backend to use".to_string(),
                    },
                    FieldDef {
                        name: "timeout".to_string(),
                        ty: TypeRef::Optional(Box::new(TypeRef::Primitive(PrimitiveType::U64))),
                        optional: true,
                        default: None,
                        doc: "Optional timeout in milliseconds".to_string(),
                    },
                ],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                doc: "Configuration for extraction".to_string(),
            },
            TypeDef {
                name: "ExtractionResult".to_string(),
                rust_path: "kreuzberg::ExtractionResult".to_string(),
                fields: vec![
                    FieldDef {
                        name: "text".to_string(),
                        ty: TypeRef::String,
                        optional: false,
                        default: None,
                        doc: "Extracted text".to_string(),
                    },
                    FieldDef {
                        name: "confidence".to_string(),
                        ty: TypeRef::Primitive(PrimitiveType::F32),
                        optional: false,
                        default: None,
                        doc: "Confidence score".to_string(),
                    },
                ],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                doc: "Result of extraction".to_string(),
            },
        ],
        functions: vec![
            FunctionDef {
                name: "extractFileSync".to_string(),
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
                return_type: TypeRef::Named("ExtractionResult".to_string()),
                is_async: false,
                error_type: Some("Error".to_string()),
                doc: "Extract text from a file synchronously".to_string(),
            },
            FunctionDef {
                name: "extractFileAsync".to_string(),
                rust_path: "kreuzberg::extract_file_async".to_string(),
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
                return_type: TypeRef::Named("ExtractionResult".to_string()),
                is_async: true,
                error_type: Some("Error".to_string()),
                doc: "Extract text from a file asynchronously".to_string(),
            },
        ],
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
                    doc: "PaddleOCR".to_string(),
                },
                EnumVariant {
                    name: "EasyOcr".to_string(),
                    fields: vec![],
                    doc: "EasyOCR".to_string(),
                },
            ],
            doc: "Available OCR backends".to_string(),
        }],
        errors: vec![ErrorDef {
            name: "Error".to_string(),
            rust_path: "kreuzberg::Error".to_string(),
            variants: vec![
                ErrorVariant {
                    name: "IoError".to_string(),
                    message: Some("I/O error".to_string()),
                    doc: "File I/O error".to_string(),
                },
                ErrorVariant {
                    name: "OcrError".to_string(),
                    message: Some("OCR processing failed".to_string()),
                    doc: "OCR processing error".to_string(),
                },
            ],
            doc: "Error types".to_string(),
        }],
    };

    let config = SkifConfig {
        crate_config: CrateConfig {
            name: "kreuzberg".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
        },
        languages: vec![],
        exclude: Default::default(),
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
        java: Some(JavaConfig {
            package: Some("dev.kreuzberg.extraction".to_string()),
            ffi_style: "panama".to_string(),
        }),
        csharp: None,
        scaffold: None,
        readme: None,
        lint: None,
        custom_files: None,
    };

    let result = backend.generate_bindings(&api, &config).unwrap();

    println!("\n\n=== GENERATED JAVA FILES ===\n");

    for file in &result {
        let filename = file.path.to_string_lossy();
        println!("--- {} ---\n", filename);
        println!("{}\n", file.content);
    }
}
