use crate::type_map::Pyo3Mapper;
use ahash::AHashSet;
use skif_codegen::builder::RustFileBuilder;
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::ApiSurface;
use std::path::PathBuf;

pub struct Pyo3Backend;

impl Pyo3Backend {
    fn binding_config(core_import: &str) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &["pyclass(frozen)"],
            field_attrs: &["pyo3(get)"],
            struct_derives: &["Clone"],
            method_block_attr: Some("pymethods"),
            constructor_attr: "#[new]",
            static_attr: Some("staticmethod"),
            function_attr: "#[pyfunction]",
            enum_attrs: &["pyclass(eq, eq_int)"],
            enum_derives: &["Clone", "PartialEq"],
            needs_signature: true,
            signature_prefix: "    #[pyo3(signature = (",
            signature_suffix: "))]",
            core_import,
            async_pattern: AsyncPattern::Pyo3FutureIntoPy,
        }
    }
}

impl Backend for Pyo3Backend {
    fn name(&self) -> &str {
        "pyo3"
    }

    fn language(&self) -> Language {
        Language::Python
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: true,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let mapper = Pyo3Mapper;
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("pyo3::prelude::*");
        builder.add_import("pyo3::types::PyDict");
        builder.add_import("pyo3::exceptions::PyRuntimeError");
        builder.add_import("std::collections::HashMap");
        builder.add_import(&core_import);

        // Check if we have async functions and add imports if needed
        let has_async =
            api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));
        if has_async {
            builder.add_import("pyo3_async_runtimes");
        }

        // Check if we have opaque types and add Arc import if needed
        let opaque_types: AHashSet<String> = api
            .types
            .iter()
            .filter(|t| t.is_opaque)
            .map(|t| t.name.clone())
            .collect();
        if !opaque_types.is_empty() {
            builder.add_import("std::sync::Arc");
        }

        for typ in &api.types {
            if typ.is_opaque {
                builder.add_item(&generators::gen_opaque_struct(typ, &cfg));
                let impl_block = generators::gen_opaque_impl_block(typ, &mapper, &cfg, &opaque_types);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
            } else {
                builder.add_item(&generators::gen_struct(typ, &mapper, &cfg));
                let impl_block = generators::gen_impl_block(typ, &mapper, &cfg);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
            }
        }
        for e in &api.enums {
            builder.add_item(&generators::gen_enum(e, &cfg));
        }
        for f in &api.functions {
            builder.add_item(&generators::gen_function(f, &mapper, &cfg));
        }

        let convertible = skif_codegen::conversions::convertible_types(api);
        // From/Into conversions
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible) {
                builder.add_item(&skif_codegen::conversions::gen_from_binding_to_core(typ, &core_import));
                builder.add_item(&skif_codegen::conversions::gen_from_core_to_binding(typ, &core_import));
            }
        }
        for e in &api.enums {
            if skif_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&skif_codegen::conversions::gen_enum_from_binding_to_core(
                    e,
                    &core_import,
                ));
                builder.add_item(&skif_codegen::conversions::gen_enum_from_core_to_binding(
                    e,
                    &core_import,
                ));
            }
        }

        // Async runtime initialization (if needed)
        if has_async {
            builder.add_item(&gen_async_runtime_init());
        }

        // Module init
        builder.add_item(&gen_module_init(&config.python_module_name(), api));

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.python.as_ref(),
            &config.crate_config.name,
            "crates/{name}-py/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }

    fn generate_type_stubs(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let stubs_config = match config.python.as_ref().and_then(|c| c.stubs.as_ref()) {
            Some(s) => s,
            None => return Ok(vec![]),
        };

        let content = crate::gen_stubs::gen_stubs(api);

        let stubs_path = resolve_output_dir(
            Some(&stubs_config.output),
            &config.crate_config.name,
            stubs_config.output.to_string_lossy().as_ref(),
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&stubs_path).join(format!("{}.pyi", config.python_module_name())),
            content,
            generated_header: true,
        }])
    }
}

/// Generate the async runtime initialization function.
fn gen_async_runtime_init() -> String {
    r#"#[pyfunction]
pub fn init_async_runtime(py: Python) -> PyResult<()> {
    pyo3_async_runtimes::tokio::init_once(py);
    Ok(())
}"#
    .to_string()
}

/// Generate the module initialization function.
fn gen_module_init(module_name: &str, api: &ApiSurface) -> String {
    let mut lines = vec![
        "#[pymodule]".to_string(),
        format!("fn {module_name}(m: &Bound<'_, PyModule>) -> PyResult<()> {{"),
    ];

    // Check if we have async functions
    let has_async =
        api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));

    if has_async {
        lines.push("    m.add_function(wrap_pyfunction!(init_async_runtime, m)?)?;".to_string());
    }

    // Deduplicate registered types and enums
    let mut registered: AHashSet<String> = AHashSet::new();
    for typ in &api.types {
        if registered.insert(typ.name.clone()) {
            lines.push(format!("    m.add_class::<{}>()?;", typ.name));
        }
    }
    for enum_def in &api.enums {
        if registered.insert(enum_def.name.clone()) {
            lines.push(format!("    m.add_class::<{}>()?;", enum_def.name));
        }
    }
    for func in &api.functions {
        lines.push(format!("    m.add_function(wrap_pyfunction!({}, m)?)?;", func.name));
    }

    lines.push("    Ok(())".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}
