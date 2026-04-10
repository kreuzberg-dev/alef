mod functions;
mod helpers;
mod types;

use alef_codegen::builder::RustFileBuilder;
use alef_codegen::generators;
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

use functions::{gen_free_function, gen_method_wrapper};
use helpers::{gen_build_rs, gen_cbindgen_toml, gen_ffi_tokio_runtime, gen_free_string, gen_last_error, gen_version};
use types::{gen_enum_from_i32, gen_enum_to_i32, gen_field_accessor, gen_type_free, gen_type_from_json};

pub struct FfiBackend;

impl FfiBackend {}

impl Backend for FfiBackend {
    fn name(&self) -> &str {
        "ffi"
    }

    fn language(&self) -> Language {
        Language::Ffi
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: false,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let prefix = config.ffi_prefix();
        let header_name = config.ffi_header_name();

        let output_dir = resolve_output_dir(
            config.output.ffi.as_ref(),
            &config.crate_config.name,
            "crates/{name}-ffi/src/",
        );

        let parent_dir = PathBuf::from(&output_dir)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let files = vec![
            GeneratedFile {
                path: PathBuf::from(&output_dir).join("lib.rs"),
                content: gen_lib_rs(api, &prefix, config),
                generated_header: false,
            },
            GeneratedFile {
                path: parent_dir.join("cbindgen.toml"),
                content: gen_cbindgen_toml(&prefix, api),
                generated_header: false,
            },
            GeneratedFile {
                path: parent_dir.join("build.rs"),
                content: gen_build_rs(&header_name),
                generated_header: false,
            },
        ];

        Ok(files)
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "cargo",
            crate_suffix: "-ffi",
            depends_on_ffi: false,
            post_build: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// lib.rs generation
// ---------------------------------------------------------------------------

fn gen_lib_rs(api: &ApiSurface, prefix: &str, config: &AlefConfig) -> String {
    let mut builder = RustFileBuilder::new().with_generated_header();

    // Imports
    builder.add_import("std::ffi::{c_char, CStr, CString}");
    builder.add_import("std::cell::RefCell");
    let core_import = config.core_import();

    // Import traits needed for trait method dispatch
    for trait_path in generators::collect_trait_imports(api) {
        builder.add_import(&trait_path);
    }

    // Only import serde_json when types need from_json deserialization or
    // when Json/Vec/Map fields/returns require serialization
    let has_from_json_types = api
        .types
        .iter()
        .any(|t| !t.is_opaque && !t.fields.iter().any(|f| f.sanitized));
    let has_serde_fields = api.types.iter().any(|t| {
        t.fields.iter().any(|f| {
            matches!(f.ty, alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _))
                || matches!(&f.ty, alef_core::ir::TypeRef::Optional(inner) if matches!(inner.as_ref(), alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _)))
        })
    });
    let has_serde_returns = api.types.iter().any(|t| {
        t.methods.iter().any(|m| {
            matches!(m.return_type, alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _))
                || matches!(&m.return_type, alef_core::ir::TypeRef::Optional(inner) if matches!(inner.as_ref(), alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _)))
        })
    }) || api.functions.iter().any(|f| {
        matches!(f.return_type, alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _))
            || matches!(&f.return_type, alef_core::ir::TypeRef::Optional(inner) if matches!(inner.as_ref(), alef_core::ir::TypeRef::Json | alef_core::ir::TypeRef::Vec(_) | alef_core::ir::TypeRef::Map(_, _)))
    });
    if has_from_json_types || has_serde_fields || has_serde_returns {
        builder.add_import("serde_json");
    }

    // Custom module declarations
    let custom_mods = config.custom_modules.for_language(Language::Ffi);
    for module in custom_mods {
        builder.add_item(&format!("pub mod {module};"));
    }

    // Thread-local last_error infrastructure
    builder.add_item(&gen_last_error(prefix));

    // free_string helper
    builder.add_item(&gen_free_string(prefix));

    // version helper
    builder.add_item(&gen_version(prefix));

    // Struct opaque-handle functions (from_json + free + field accessors + methods)
    for typ in &api.types {
        // Opaque types don't implement serde Deserialize, so skip from_json.
        // Types with sanitized fields may not implement Deserialize either
        // (the core type has non-serializable field types).
        let has_sanitized = typ.fields.iter().any(|f| f.sanitized);
        if !typ.is_opaque && !has_sanitized {
            builder.add_item(&gen_type_from_json(typ, prefix, &core_import));
        }
        builder.add_item(&gen_type_free(typ, prefix, &core_import));

        // Field accessors — skip sanitized fields (binding type differs from core)
        for field in &typ.fields {
            if !field.sanitized {
                builder.add_item(&gen_field_accessor(typ, field, prefix, &core_import));
            }
        }

        // Method wrappers
        for method in &typ.methods {
            builder.add_item(&gen_method_wrapper(typ, method, prefix, &core_import));
        }
    }

    // Enum functions (from_i32 + to_i32) — only for simple unit-variant enums
    for enum_def in &api.enums {
        if alef_codegen::conversions::can_generate_enum_conversion(enum_def) {
            builder.add_item(&gen_enum_from_i32(enum_def, prefix, &core_import));
            builder.add_item(&gen_enum_to_i32(enum_def, prefix, &core_import));
        }
    }

    // Emit tokio runtime helper if any function or method is async
    let has_async_functions =
        api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));
    if has_async_functions {
        builder.add_item(&gen_ffi_tokio_runtime());
    }

    // Free functions (async functions are wrapped with block_on via the runtime helper)
    for func in &api.functions {
        builder.add_item(&gen_free_function(func, prefix, &core_import));
    }

    // Build adapter body map (consumed by generators via body substitution)
    let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Ffi).unwrap_or_default();

    // Visitor/callback FFI support — generated when `[ffi] visitor_callbacks = true`.
    // Note: the generated code uses std::rc::Rc fully qualified, so no extra import needed.
    if config.ffi.as_ref().is_some_and(|f| f.visitor_callbacks) {
        builder.add_item(&crate::gen_visitor::gen_visitor_bindings(prefix, &core_import));
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::*;

    fn sample_api() -> ApiSurface {
        ApiSurface {
            crate_name: "my-lib".to_string(),
            version: "1.0.0".to_string(),
            types: vec![TypeDef {
                name: "Config".to_string(),
                rust_path: "my_lib::Config".to_string(),
                fields: vec![
                    FieldDef {
                        name: "timeout".to_string(),
                        ty: TypeRef::Primitive(PrimitiveType::U64),
                        optional: false,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                    FieldDef {
                        name: "name".to_string(),
                        ty: TypeRef::String,
                        optional: false,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                    FieldDef {
                        name: "verbose".to_string(),
                        ty: TypeRef::Primitive(PrimitiveType::Bool),
                        optional: true,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                ],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                doc: "Configuration struct.".to_string(),
                cfg: None,
            }],
            functions: vec![FunctionDef {
                name: "extract".to_string(),
                rust_path: "my_lib::extract".to_string(),
                params: vec![ParamDef {
                    name: "path".to_string(),
                    ty: TypeRef::Path,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                }],
                return_type: TypeRef::Named("ExtractionResult".to_string()),
                is_async: false,
                error_type: Some("MyError".to_string()),
                doc: "Extract content from a file.".to_string(),
                cfg: None,
                sanitized: false,
                returns_ref: false,
            }],
            enums: vec![EnumDef {
                name: "OutputFormat".to_string(),
                rust_path: "my_lib::OutputFormat".to_string(),
                variants: vec![
                    EnumVariant {
                        name: "Text".to_string(),
                        fields: vec![],
                        doc: String::new(),
                        is_default: false,
                    },
                    EnumVariant {
                        name: "Html".to_string(),
                        fields: vec![],
                        doc: String::new(),
                        is_default: false,
                    },
                ],
                doc: "Output format.".to_string(),
                cfg: None,
            }],
            errors: vec![],
        }
    }

    fn sample_config() -> AlefConfig {
        toml::from_str(
            r#"
            languages = ["ffi"]
            [crate]
            name = "my-lib"
            sources = ["src/lib.rs"]
            "#,
        )
        .unwrap()
    }

    #[test]
    fn test_generates_lib_rs() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        assert!(files.iter().any(|f| f.path.ends_with("lib.rs")));

        let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();
        assert!(lib.content.contains("extern \"C\""));
        assert!(lib.content.contains("my_lib_last_error_code"));
        assert!(lib.content.contains("my_lib_config_from_json"));
        assert!(lib.content.contains("my_lib_config_free"));
        assert!(lib.content.contains("my_lib_config_timeout"));
        assert!(lib.content.contains("my_lib_config_name"));
        assert!(lib.content.contains("my_lib_free_string"));
        assert!(lib.content.contains("my_lib_version"));
        assert!(lib.content.contains("my_lib_extract"));
        assert!(lib.content.contains("my_lib_output_format_from_i32"));
        assert!(lib.content.contains("my_lib_output_format_from_str"));
    }

    #[test]
    fn test_generates_cbindgen_toml() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let cbindgen = files.iter().find(|f| f.path.ends_with("cbindgen.toml")).unwrap();
        assert!(cbindgen.content.contains("MY_LIB_H"));
        assert!(cbindgen.content.contains("language = \"C\""));
    }

    #[test]
    fn test_generates_build_rs() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let build = files.iter().find(|f| f.path.ends_with("build.rs")).unwrap();
        assert!(build.content.contains("cbindgen::generate"));
        assert!(build.content.contains("my_lib.h"));
    }

    #[test]
    fn test_custom_prefix() {
        let api = sample_api();
        let config: AlefConfig = toml::from_str(
            r#"
            languages = ["ffi"]
            [crate]
            name = "my-lib"
            sources = ["src/lib.rs"]
            [ffi]
            prefix = "ml"
            header_name = "mylib.h"
            "#,
        )
        .unwrap();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();
        assert!(lib.content.contains("ml_last_error_code"));
        assert!(lib.content.contains("ml_config_from_json"));

        let build = files.iter().find(|f| f.path.ends_with("build.rs")).unwrap();
        assert!(build.content.contains("mylib.h"));
    }
}
