use alef::backends::csharp::CsharpBackend;
use alef::core::backend::Backend;
use alef::core::config::{NewAlefConfig, ResolvedCrateConfig, AdapterPattern, AdapterConfig};
use alef::core::ir::{
    ApiSurface, FieldDef, MethodDef, ParamDef, PrimitiveType, ReceiverKind, TypeDef, TypeRef,
};

/// Test that opaque types with streaming methods emit static wrapper methods in the main wrapper class.
/// This ensures `gen_opaque_streaming_static_wrapper` is called during method emission.
#[test]
fn test_opaque_streaming_static_wrapper() {
    let backend = CsharpBackend;

    // Create API with an opaque type and a streaming method
    let api = ApiSurface {
        crate_name: "kreuzcrawl".to_string(),
        version: "0.1.0".to_string(),
        types: vec![
            TypeDef {
                name: "CrawlEvent".to_string(),
                rust_path: "kreuzcrawl::CrawlEvent".to_string(),
                original_rust_path: String::new(),
                fields: vec![FieldDef {
                    name: "url".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                    is_boxed: false,
                    type_rust_path: None,
                    cfg: None,
                    typed_default: None,
                    core_wrapper: alef::core::ir::CoreWrapper::None,
                    vec_inner_core_wrapper: alef::core::ir::CoreWrapper::None,
                    newtype_wrapper: None,
                    serde_rename: None,
                    serde_flatten: false,
                    binding_excluded: false,
                    binding_exclusion_reason: None,
                    original_type: None,
                }],
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
                binding_excluded: false,
                binding_exclusion_reason: None,
                is_variant_wrapper: false,
                has_lifetime_params: false,
            },
            TypeDef {
                name: "DefaultEngine".to_string(),
                rust_path: "kreuzcrawl::DefaultEngine".to_string(),
                original_rust_path: String::new(),
                fields: vec![],
                methods: vec![
                    MethodDef {
                        name: "crawl_stream".to_string(),
                        doc: "Stream crawl results".to_string(),
                        params: vec![
                            ParamDef {
                                name: "request".to_string(),
                                ty: TypeRef::Named("CrawlRequest".to_string()),
                                optional: false,
                            },
                        ],
                        return_type: TypeRef::Vec(Box::new(TypeRef::Named("CrawlEvent".to_string()))),
                        is_async: true,
                        is_static: false,
                        receiver: Some(ReceiverKind::Ref),
                        error_type: None,
                    },
                ],
                is_opaque: true,
                is_clone: false,
                is_copy: false,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: true,
                serde_rename_all: None,
                has_serde: false,
                super_traits: vec![],
                doc: "Opaque crawler engine".to_string(),
                cfg: None,
                binding_excluded: false,
                binding_exclusion_reason: None,
                is_variant_wrapper: false,
                has_lifetime_params: false,
            },
            TypeDef {
                name: "CrawlRequest".to_string(),
                rust_path: "kreuzcrawl::CrawlRequest".to_string(),
                original_rust_path: String::new(),
                fields: vec![FieldDef {
                    name: "url".to_string(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                    is_boxed: false,
                    type_rust_path: None,
                    cfg: None,
                    typed_default: None,
                    core_wrapper: alef::core::ir::CoreWrapper::None,
                    vec_inner_core_wrapper: alef::core::ir::CoreWrapper::None,
                    newtype_wrapper: None,
                    serde_rename: None,
                    serde_flatten: false,
                    binding_excluded: false,
                    binding_exclusion_reason: None,
                    original_type: None,
                }],
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
                binding_excluded: false,
                binding_exclusion_reason: None,
                is_variant_wrapper: false,
                has_lifetime_params: false,
            },
        ],
        enums: vec![],
        functions: vec![],
        errors: vec![],
        excluded_type_paths: Default::default(),
    };

    // Minimal config with adapter for streaming
    let mut config = ResolvedCrateConfig {
        name: "kreuzcrawl".to_string(),
        ..Default::default()
    };

    // Add streaming adapter that marks crawl_stream as streaming
    config.adapters.push(AdapterConfig {
        name: "crawl_stream".to_string(),
        pattern: AdapterPattern::Streaming,
        owner_type: Some("DefaultEngine".to_string()),
        item_type: Some("CrawlEvent".to_string()),
        params: vec![],
    });

    let files = backend.generate_bindings(&api, &config).expect("generation should succeed");

    // Find the wrapper class file
    let wrapper_file = files
        .iter()
        .find(|f| f.path.ends_with("KreuzcrawlLib.cs"))
        .expect("should generate KreuzcrawlLib.cs");

    let content = &wrapper_file.content;

    // Verify that static wrapper method is emitted
    assert!(
        content.contains("public static async IAsyncEnumerable<CrawlEvent> CrawlStreamAsync("),
        "wrapper class should emit static CrawlStreamAsync method; content:\n{}",
        content
    );

    // Verify the method delegation pattern
    assert!(
        content.contains("await foreach (var item in engine."),
        "static wrapper should delegate to instance method via await foreach"
    );
}
