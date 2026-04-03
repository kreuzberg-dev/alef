use crate::type_map::Pyo3Mapper;
use ahash::AHashSet;
use skif_codegen::builder::RustFileBuilder;
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{AdapterPattern, Language, SkifConfig, detect_serde_available, resolve_output_dir};
use skif_core::ir::ApiSurface;
use std::path::PathBuf;

pub struct Pyo3Backend;

impl Pyo3Backend {
    fn binding_config(core_import: &str, has_serde: bool) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &["pyclass(frozen, from_py_object)"],
            field_attrs: &["pyo3(get)"],
            struct_derives: &["Clone"],
            method_block_attr: Some("pymethods"),
            constructor_attr: "#[new]",
            static_attr: Some("staticmethod"),
            function_attr: "#[pyfunction]",
            enum_attrs: &["pyclass(eq, eq_int, from_py_object)"],
            enum_derives: &["Clone", "PartialEq"],
            needs_signature: true,
            signature_prefix: "    #[pyo3(signature = (",
            signature_suffix: "))]",
            core_import,
            async_pattern: AsyncPattern::Pyo3FutureIntoPy,
            has_serde,
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

        // Detect serde availability from the output crate's Cargo.toml
        let output_dir = resolve_output_dir(
            config.output.python.as_ref(),
            &config.crate_config.name,
            "crates/{name}-py/src/",
        );
        let has_serde = detect_serde_available(&output_dir);
        let cfg = Self::binding_config(&core_import, has_serde);

        // Build adapter body map for method body substitution
        let adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Python)?;

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("pyo3::prelude::*");
        // Only import core crate if we generate types from it (and skip_core_import is not set)
        if !config.crate_config.skip_core_import
            && (!api.types.is_empty() || !api.functions.is_empty() || !api.enums.is_empty())
        {
            builder.add_import(&core_import);
            // Add imports for all mapped crates when path_mappings exist
            for target_crate in config.crate_config.path_mappings.values() {
                let crate_name = target_crate.split("::").next().unwrap_or(target_crate);
                builder.add_import(crate_name);
            }
        }

        // Import serde_json when available (needed for serde-based param conversion)
        if has_serde {
            builder.add_import("serde_json");
        }

        // Check if we have async functions and add imports if needed
        let has_async =
            api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));
        if has_async {
            builder.add_import("pyo3_async_runtimes");
            // PyRuntimeError is needed for async error mapping via PyErr::new::<PyRuntimeError, _>
            let has_async_error = api.functions.iter().any(|f| f.is_async && f.error_type.is_some())
                || api
                    .types
                    .iter()
                    .any(|t| t.methods.iter().any(|m| m.is_async && m.error_type.is_some()));
            if has_async_error {
                builder.add_import("pyo3::exceptions::PyRuntimeError");
            }
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

        // Clippy allows for generated FFI code
        builder.add_inner_attribute("allow(clippy::too_many_arguments)");
        builder.add_inner_attribute("allow(clippy::missing_errors_doc)");
        builder.add_inner_attribute("allow(clippy::useless_conversion)");
        builder.add_inner_attribute("allow(clippy::let_unit_value)");

        // Custom module declarations
        let custom_mods = config.custom_modules.for_language(Language::Python);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

        // Add adapter-generated standalone items (streaming iterators, callback bridges)
        for adapter in &config.adapters {
            match adapter.pattern {
                AdapterPattern::Streaming => {
                    let key = format!("{}.__stream_struct__", adapter.item_type.as_deref().unwrap_or(""));
                    if let Some(struct_code) = adapter_bodies.get(&key) {
                        builder.add_item(struct_code);
                    }
                }
                AdapterPattern::CallbackBridge => {
                    let struct_key = format!("{}.__bridge_struct__", adapter.name);
                    let impl_key = format!("{}.__bridge_impl__", adapter.name);
                    if let Some(struct_code) = adapter_bodies.get(&struct_key) {
                        builder.add_item(struct_code);
                    }
                    if let Some(impl_code) = adapter_bodies.get(&impl_key) {
                        builder.add_item(impl_code);
                    }
                }
                _ => {}
            }
        }

        for typ in &api.types {
            if typ.is_opaque {
                builder.add_item(&generators::gen_opaque_struct(typ, &cfg));
                let impl_block = generators::gen_opaque_impl_block(typ, &mapper, &cfg, &opaque_types, &adapter_bodies);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
            } else {
                builder.add_item(&generators::gen_struct(typ, &mapper, &cfg));
                let impl_block = generators::gen_impl_block(typ, &mapper, &cfg, &adapter_bodies);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
            }
        }
        for e in &api.enums {
            builder.add_item(&generators::gen_enum(e, &cfg));
        }
        for f in &api.functions {
            builder.add_item(&generators::gen_function(
                f,
                &mapper,
                &cfg,
                &adapter_bodies,
                &opaque_types,
            ));
        }

        let binding_to_core = skif_codegen::conversions::convertible_types(api);
        let core_to_binding = skif_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions — separate sets for each direction
        for typ in &api.types {
            // binding→core: strict (no sanitized fields)
            if skif_codegen::conversions::can_generate_conversion(typ, &binding_to_core) {
                builder.add_item(&skif_codegen::conversions::gen_from_binding_to_core(typ, &core_import));
            }
            // core→binding: permissive (sanitized fields use format!("{:?}"))
            if skif_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&skif_codegen::conversions::gen_from_core_to_binding(
                    typ,
                    &core_import,
                    &opaque_types,
                ));
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
        builder.add_item(&gen_module_init(&config.python_module_name(), api, config));

        let content = builder.build();

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
pub fn init_async_runtime() -> PyResult<()> {
    // Tokio runtime auto-initializes on first future_into_py call
    Ok(())
}"#
    .to_string()
}

/// Generate the module initialization function.
fn gen_module_init(module_name: &str, api: &ApiSurface, config: &SkifConfig) -> String {
    let mut lines = vec![
        "#[pymodule]".to_string(),
        format!("pub fn {module_name}(m: &Bound<'_, PyModule>) -> PyResult<()> {{"),
    ];

    // Check if we have async functions
    let has_async =
        api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));

    if has_async {
        lines.push("    m.add_function(wrap_pyfunction!(init_async_runtime, m)?)?;".to_string());
    }

    // Custom registrations (before generated ones so hand-written classes are registered first)
    if let Some(reg) = config.custom_registrations.for_language(Language::Python) {
        for class in &reg.classes {
            lines.push(format!("    m.add_class::<{class}>()?;"));
        }
        for func in &reg.functions {
            lines.push(format!("    m.add_function(wrap_pyfunction!({func}, m)?)?;"));
        }
        for call in &reg.init_calls {
            lines.push(format!("    {call}"));
        }
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
