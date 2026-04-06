use crate::type_map::Pyo3Mapper;
use ahash::AHashSet;
use eisberg_codegen::builder::RustFileBuilder;
use eisberg_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use eisberg_core::backend::{Backend, Capabilities, GeneratedFile};
use eisberg_core::config::{AdapterPattern, Language, SkifConfig, detect_serde_available, resolve_output_dir};
use eisberg_core::ir::ApiSurface;
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
            type_name_prefix: "",
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
        let adapter_bodies = eisberg_adapters::build_adapter_bodies(config, Language::Python)?;

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("pyo3::prelude::*");
        // Note: core_import and path_mapping crates are referenced via fully-qualified paths
        // in generated code (e.g. `core_import::TypeName`), so no bare `use crate_name;`
        // import is needed — that would trigger clippy::single_component_path_imports.

        // Import serde_json when available (needed for serde-based param conversion)
        if has_serde {
            builder.add_import("serde_json");
        }

        // Import traits needed for trait method dispatch
        for trait_path in generators::collect_trait_imports(api) {
            builder.add_import(&trait_path);
        }

        // Check if we have non-sanitized async functions (sanitized async methods produce stubs, not async code)
        let has_async = api.functions.iter().any(|f| f.is_async && !f.sanitized)
            || api
                .types
                .iter()
                .any(|t| t.methods.iter().any(|m| m.is_async && !m.sanitized));
        if has_async {
            builder.add_import("pyo3_async_runtimes");
            // PyRuntimeError is needed for async error mapping via PyErr::new::<PyRuntimeError, _>
            let has_async_error = api
                .functions
                .iter()
                .any(|f| f.is_async && !f.sanitized && f.error_type.is_some())
                || api.types.iter().any(|t| {
                    t.methods
                        .iter()
                        .any(|m| m.is_async && !m.sanitized && m.error_type.is_some())
                });
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

        // Check if we have Map types and add HashMap import if needed
        let has_maps = api.types.iter().any(|t| {
            t.fields
                .iter()
                .any(|f| matches!(&f.ty, eisberg_core::ir::TypeRef::Map(_, _)))
        }) || api.functions.iter().any(|f| {
            f.params
                .iter()
                .any(|p| matches!(&p.ty, eisberg_core::ir::TypeRef::Map(_, _)))
                || matches!(&f.return_type, eisberg_core::ir::TypeRef::Map(_, _))
        });
        if has_maps {
            builder.add_import("std::collections::HashMap");
        }

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
                if typ.has_default {
                    builder.add_item(&generators::gen_struct_default_impl(typ, ""));
                }
                let impl_block = generators::gen_impl_block(typ, &mapper, &cfg, &adapter_bodies, &opaque_types);
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

        // Error types (create_exception! macros + converter functions)
        let module_name = config.python_module_name();
        for error in &api.errors {
            builder.add_item(&eisberg_codegen::error_gen::gen_pyo3_error_types(error, &module_name));
            builder.add_item(&eisberg_codegen::error_gen::gen_pyo3_error_converter(
                error,
                &core_import,
            ));
        }

        let binding_to_core = eisberg_codegen::conversions::convertible_types(api);
        let core_to_binding = eisberg_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions — separate sets for each direction
        for typ in &api.types {
            // binding→core: strict (no sanitized fields)
            if eisberg_codegen::conversions::can_generate_conversion(typ, &binding_to_core) {
                builder.add_item(&eisberg_codegen::conversions::gen_from_binding_to_core(
                    typ,
                    &core_import,
                ));
            }
            // core→binding: permissive (sanitized fields use format!("{:?}"))
            if eisberg_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&eisberg_codegen::conversions::gen_from_core_to_binding(
                    typ,
                    &core_import,
                    &opaque_types,
                ));
            }
        }
        for e in &api.enums {
            // Binding→core: only for enums with simple fields (Default::default() must work)
            if eisberg_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&eisberg_codegen::conversions::gen_enum_from_binding_to_core(
                    e,
                    &core_import,
                ));
            }
            // Core→binding: always possible (data variants discarded with `..`)
            if eisberg_codegen::conversions::can_generate_enum_conversion_from_core(e) {
                builder.add_item(&eisberg_codegen::conversions::gen_enum_from_core_to_binding(
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

    fn generate_public_api(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let module_name = config.python_module_name();

        // Use stubs output path as the package directory (e.g., packages/python/html_to_markdown/)
        // This ensures we write to the correct Python package, not the Rust crate name.
        let output_base = config
            .python
            .as_ref()
            .and_then(|p| p.stubs.as_ref())
            .map(|s| PathBuf::from(&s.output))
            .unwrap_or_else(|| {
                let package_name = config.crate_config.name.replace('-', "_");
                PathBuf::from(format!("packages/python/{}", package_name))
            });
        let package_name = output_base
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| config.crate_config.name.replace('-', "_"));

        let mut files = vec![];

        // 1. Generate options.py (enums and dataclasses)
        let options_content = gen_options_py(api, &package_name);
        files.push(GeneratedFile {
            path: output_base.join("options.py"),
            content: options_content,
            generated_header: true,
        });

        // 2. Generate api.py (wrapper functions)
        let api_content = gen_api_py(api, &module_name);
        files.push(GeneratedFile {
            path: output_base.join("api.py"),
            content: api_content,
            generated_header: true,
        });

        // 3. Generate exceptions.py (exception hierarchy)
        let exceptions_content = gen_exceptions_py(api);
        files.push(GeneratedFile {
            path: output_base.join("exceptions.py"),
            content: exceptions_content,
            generated_header: true,
        });

        // 4. Generate __init__.py (re-exports)
        let init_content = gen_init_py(api, &module_name, &api.version);
        files.push(GeneratedFile {
            path: output_base.join("__init__.py"),
            content: init_content,
            generated_header: true,
        });

        Ok(files)
    }
}

/// Generate options.py with enums and dataclasses for types with defaults.
fn gen_options_py(api: &ApiSurface, _package_name: &str) -> String {
    use crate::type_map::python_type;
    use eisberg_core::ir::{DefaultValue, TypeRef};

    let mut lines = vec![
        "\"\"\"Options and enums for conversion.\"\"\"".to_string(),
        "".to_string(),
        "from dataclasses import dataclass, field".to_string(),
        "from enum import Enum".to_string(),
        "".to_string(),
    ];

    // Generate enums
    for enum_def in &api.enums {
        lines.push(format!("class {}(str, Enum):", enum_def.name));
        lines.push("    \"\"\"\"".to_string() + enum_def.doc.lines().next().unwrap_or("") + "\"\"\"");
        for variant in &enum_def.variants {
            // Use the variant name in lowercase as the string value
            let variant_value = variant.name.to_lowercase();
            lines.push(format!("    {} = \"{}\"", variant.name, variant_value));
        }
        lines.push("".to_string());
    }

    // Generate dataclasses for types with has_default=true
    for typ in &api.types {
        if typ.has_default && !typ.fields.is_empty() {
            lines.push("@dataclass".to_string());
            lines.push(format!("class {}:", typ.name));
            lines.push("    \"\"\"\"".to_string() + typ.doc.lines().next().unwrap_or("") + "\"\"\"");

            for field in &typ.fields {
                let field_type = python_type(&field.ty);

                // Generate type hint
                let type_hint = if field.optional {
                    format!("{} | None", python_type(&field.ty.clone()))
                } else {
                    field_type
                };

                // Generate default value
                if let Some(typed_default) = &field.typed_default {
                    let default_val = match typed_default {
                        DefaultValue::BoolLiteral(b) => {
                            if *b {
                                "True".to_string()
                            } else {
                                "False".to_string()
                            }
                        }
                        DefaultValue::StringLiteral(s) => format!("\"{}\"", s.escape_default()),
                        DefaultValue::IntLiteral(i) => i.to_string(),
                        DefaultValue::FloatLiteral(f) => f.to_string(),
                        DefaultValue::EnumVariant(v) => {
                            // For enum defaults, use the enum class and variant
                            format!("\"{}\".lower()", v)
                        }
                        DefaultValue::Empty => match &field.ty {
                            TypeRef::Vec(_) => "field(default_factory=list)".to_string(),
                            TypeRef::Map(_, _) => "field(default_factory=dict)".to_string(),
                            TypeRef::String => "\"\"".to_string(),
                            _ => "None".to_string(),
                        },
                        DefaultValue::None => "None".to_string(),
                    };

                    lines.push(format!("    {}: {} = {}", field.name, type_hint, default_val));
                } else {
                    lines.push(format!("    {}: {}", field.name, type_hint));
                }
            }
            lines.push("".to_string());
        }
    }

    lines.join("\n")
}

/// Generate api.py with wrapper functions that convert Python types to Rust bindings.
fn gen_api_py(api: &ApiSurface, module_name: &str) -> String {
    use crate::type_map::python_type;

    let mut lines = vec![
        "\"\"\"Public API surface for conversion.\"\"\"".to_string(),
        "".to_string(),
        format!("import {} as _rust", module_name),
        "from .options import *  # noqa: F401, F403".to_string(),
        "".to_string(),
    ];

    // Generate wrapper functions for each top-level function
    for func in &api.functions {
        let return_type = python_type(&func.return_type);

        // Build function signature
        let mut params = vec!["self".to_string()];
        for param in &func.params {
            let param_type = python_type(&param.ty);
            params.push(format!("{}: {}", param.name, param_type));
        }

        lines.push(format!("def {}({}) -> {}:", func.name, params.join(", "), return_type));

        if !func.doc.is_empty() {
            lines.push(format!("    \"\"\"{}\"\"\"", func.doc.lines().next().unwrap_or("")));
        }

        // Build the Rust call
        let mut rust_args = vec![];
        for param in &func.params {
            rust_args.push(format!("_rust.{}={}", param.name, param.name));
        }

        lines.push(format!("    return _rust.{}({})", func.name, rust_args.join(", ")));
        lines.push("".to_string());
    }

    lines.join("\n")
}

/// Generate exceptions.py with exception hierarchy from IR errors.
fn gen_exceptions_py(api: &ApiSurface) -> String {
    let mut lines = vec![
        "\"\"\"Exception hierarchy for errors.\"\"\"".to_string(),
        "".to_string(),
    ];

    // Generate base exception for each error
    for error in &api.errors {
        lines.push(format!("class {}(Exception):", error.name));
        lines.push(format!("    \"\"\"{}\"\"\"", error.doc));
        lines.push("    pass".to_string());
        lines.push("".to_string());

        // Generate specific exceptions for each variant
        for variant in &error.variants {
            lines.push(format!("class {}({}Exception):", variant.name, error.name));
            lines.push(format!("    \"\"\"{}\"\"\"", variant.doc));
            lines.push("    pass".to_string());
            lines.push("".to_string());
        }
    }

    lines.join("\n")
}

/// Generate __init__.py with re-exports of public API.
fn gen_init_py(api: &ApiSurface, _module_name: &str, version: &str) -> String {
    let mut lines = vec![
        "\"\"\"Public API for the conversion library.\"\"\"".to_string(),
        "".to_string(),
    ];

    // Import and re-export from api module
    if !api.functions.is_empty() {
        let func_names: Vec<_> = api.functions.iter().map(|f| f.name.clone()).collect();
        lines.push(format!("from .api import {}", func_names.join(", ")));
    }

    // Import and re-export from options module
    let mut option_names = vec![];
    for enum_def in &api.enums {
        option_names.push(enum_def.name.clone());
    }
    for typ in &api.types {
        if typ.has_default {
            option_names.push(typ.name.clone());
        }
    }

    if !option_names.is_empty() {
        lines.push(format!("from .options import {}", option_names.join(", ")));
    }

    // Import and re-export from exceptions module
    let mut exception_names = vec![];
    for error in &api.errors {
        exception_names.push(error.name.clone());
        for variant in &error.variants {
            exception_names.push(variant.name.clone());
        }
    }

    if !exception_names.is_empty() {
        lines.push(format!("from .exceptions import {}", exception_names.join(", ")));
    }

    lines.push("".to_string());

    // Build __all__ list
    let mut all_items = vec![];
    for func in &api.functions {
        all_items.push(func.name.clone());
    }
    for enum_def in &api.enums {
        all_items.push(enum_def.name.clone());
    }
    for typ in &api.types {
        if typ.has_default {
            all_items.push(typ.name.clone());
        }
    }
    for error in &api.errors {
        all_items.push(error.name.clone());
        for variant in &error.variants {
            all_items.push(variant.name.clone());
        }
    }

    all_items.sort();
    all_items.push("__version__".to_string());

    if !all_items.is_empty() {
        lines.push(format!("__all__ = [\n{}\n]", {
            all_items
                .iter()
                .map(|name| format!("    \"{}\",", name))
                .collect::<Vec<_>>()
                .join("\n")
        }));
    } else {
        lines.push("__all__: list[str] = []".to_string());
    }

    lines.push("".to_string());
    lines.push(format!("__version__ = \"{}\"", version));

    lines.join("\n")
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

    // Register error exception types
    for error in &api.errors {
        for reg_line in eisberg_codegen::error_gen::gen_pyo3_error_registration(error) {
            lines.push(reg_line);
        }
    }

    lines.push("    Ok(())".to_string());
    lines.push("}".to_string());
    lines.join("\n")
}
