use crate::type_map::Pyo3Mapper;
use skif_codegen::builder::RustFileBuilder;
use skif_codegen::generators::{self, RustBindingConfig};
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::ApiSurface;
use std::path::PathBuf;

pub struct Pyo3Backend;

impl Pyo3Backend {
    fn binding_config() -> RustBindingConfig<'static> {
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
        let cfg = Self::binding_config();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("pyo3::prelude::*");
        builder.add_import("pyo3::types::PyDict");
        builder.add_import("pyo3::exceptions::PyRuntimeError");
        builder.add_import("std::collections::HashMap");

        for typ in &api.types {
            if !typ.is_opaque {
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
            path: PathBuf::from(stubs_path),
            content,
            generated_header: true,
        }])
    }
}

/// Generate the module initialization function.
fn gen_module_init(module_name: &str, api: &ApiSurface) -> String {
    let mut lines = vec![
        "#[pymodule]".to_string(),
        format!("fn {module_name}(m: &Bound<'_, PyModule>) -> PyResult<()> {{"),
    ];

    for typ in &api.types {
        if !typ.is_opaque {
            lines.push(format!("    m.add_class::<{}>()?;", typ.name));
        }
    }
    for enum_def in &api.enums {
        lines.push(format!("    m.add_class::<{}>()?;", enum_def.name));
    }
    for func in &api.functions {
        lines.push(format!("    m.add_function(wrap_pyfunction!({}, m)?)?;", func.name));
    }

    lines.push("    Ok(())".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}
