use alef_codegen::builder::RustFileBuilder;
use alef_codegen::doc_emission;
use alef_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use alef_codegen::type_mapper::TypeMapper;
use alef_core::backend::{Backend, BuildConfig, BuildDependency, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, BridgeBinding, Language, resolve_output_dir};
use alef_core::hash::{self, CommentStyle};
use alef_core::ir::{ApiSurface, TypeDef, TypeRef};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::PathBuf;

pub struct ExtendrBackend;

impl ExtendrBackend {
    fn binding_config(core_import: &str) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &[],
            field_attrs: &[],
            struct_derives: &["Clone"],
            method_block_attr: None,
            constructor_attr: "",
            static_attr: None,
            function_attr: "#[extendr]",
            enum_attrs: &[],
            enum_derives: &["Clone", "PartialEq"],
            needs_signature: false,
            signature_prefix: "",
            signature_suffix: "",
            core_import,
            async_pattern: AsyncPattern::TokioBlockOn,
            has_serde: true,
            type_name_prefix: "",
            option_duration_on_defaults: false,
            opaque_type_names: &[],
        }
    }
}

impl TypeMapper for ExtendrBackend {
    fn primitive(&self, prim: &alef_core::ir::PrimitiveType) -> Cow<'static, str> {
        use alef_core::ir::PrimitiveType;
        match prim {
            PrimitiveType::Bool => Cow::Borrowed("bool"),
            PrimitiveType::U8
            | PrimitiveType::U16
            | PrimitiveType::U32
            | PrimitiveType::I8
            | PrimitiveType::I16
            | PrimitiveType::I32 => Cow::Borrowed("i32"),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => {
                Cow::Borrowed("f64")
            }
            PrimitiveType::F32 | PrimitiveType::F64 => Cow::Borrowed("f64"),
        }
    }

    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}

impl Backend for ExtendrBackend {
    fn name(&self) -> &str {
        "extendr"
    }

    fn language(&self) -> Language {
        Language::R
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // R is single-threaded; async funcs are blocked on a per-call tokio runtime.
            supports_async: true,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        // Build adapter body map for method body substitution
        let adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::R)?;

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_inner_attribute("allow(dead_code, unused_imports, unused_variables)");
        builder.add_inner_attribute("allow(clippy::too_many_arguments, clippy::let_unit_value, clippy::needless_borrow, clippy::map_identity, clippy::just_underscores_and_digits, clippy::unused_unit, clippy::unnecessary_cast, clippy::unwrap_or_default, clippy::derivable_impls, clippy::needless_borrows_for_generic_args, clippy::unnecessary_fallible_conversions)");
        builder.add_import("extendr_api::prelude::*");

        // Import traits needed for trait method dispatch
        for trait_path in generators::collect_trait_imports(api) {
            builder.add_import(&trait_path);
        }

        // Custom module declarations
        let custom_mods = config.custom_modules.for_language(Language::R);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

        let opaque_types: ahash::AHashSet<String> = api
            .types
            .iter()
            .filter(|t| t.is_opaque)
            .map(|t| t.name.clone())
            .collect();
        let mutex_types: ahash::AHashSet<String> = api
            .types
            .iter()
            .filter(|t| t.is_opaque && generators::type_needs_mutex(t))
            .map(|t| t.name.clone())
            .collect();

        // Import Arc when there are opaque types (builder-pattern types use Arc<CoreType>).
        if !opaque_types.is_empty() {
            builder.add_import("std::sync::Arc");
        }

        // Map of options-struct name → set of field names that carry a trait-bridge handle
        // (bind_via = "options_field"). These fields are rendered as `Option<extendr_api::Robj>`
        // in the binding struct and skipped in the standard `From` impl — the convert wrapper
        // pulls them off, builds the bridge, and attaches it to the core options.
        let bridge_fields_by_type: HashMap<&str, HashSet<String>> = config
            .trait_bridges
            .iter()
            .filter(|b| b.bind_via == BridgeBinding::OptionsField)
            .filter_map(|b| {
                let type_name = b.options_type.as_deref()?;
                let field_name = b.resolved_options_field()?.to_string();
                Some((type_name, field_name))
            })
            .fold(HashMap::new(), |mut acc, (type_name, field_name)| {
                acc.entry(type_name).or_default().insert(field_name);
                acc
            });

        // Generate type bindings
        for typ in api.types.iter().filter(|typ| !typ.is_trait) {
            if typ.is_opaque {
                // Opaque types wrap the core type in Arc<T> and delegate methods to self.inner.
                builder.add_item(&generators::gen_opaque_struct(typ, &cfg));
                let impl_block =
                    generators::gen_opaque_impl_block(typ, self, &cfg, &opaque_types, &mutex_types, &adapter_bodies);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
            } else {
                // gen_struct already emits #[derive(Default)] for all structs.
                // Emitting gen_struct_default_impl here would produce a conflicting
                // `impl Default` compile error. The derive covers all types where
                // can_generate_default_impl is true (all field types implement Default).
                let bridge_fields = bridge_fields_by_type.get(typ.name.as_str());
                if let Some(fields) = bridge_fields {
                    builder.add_item(&gen_extendr_struct_with_bridge_fields(typ, self, fields));
                } else {
                    builder.add_item(&generators::gen_struct(typ, self, &cfg));
                }
                let impl_block = generators::gen_impl_block(typ, self, &cfg, &adapter_bodies, &opaque_types);
                if !impl_block.is_empty() {
                    builder.add_item(&impl_block);
                }
                // Generate config constructor if type has Default
                if typ.has_default && !typ.fields.is_empty() {
                    let bridge_fields = bridge_fields_by_type.get(typ.name.as_str());
                    let config_fn = if let Some(fields) = bridge_fields {
                        let map_fn = |ty: &alef_core::ir::TypeRef| self.map_type(ty);
                        gen_extendr_kwargs_constructor_with_bridge_fields(typ, &map_fn, fields)
                    } else {
                        let map_fn = |ty: &alef_core::ir::TypeRef| self.map_type(ty);
                        alef_codegen::config_gen::gen_extendr_kwargs_constructor(typ, &map_fn)
                    };
                    builder.add_item(&config_fn);
                }
            }
        }

        // Generate enum bindings
        for e in &api.enums {
            builder.add_item(&generators::gen_enum(e, &cfg));
        }

        // Emit binding↔core From impls so generated bodies can use `.into()` /
        // `Type::from(core)` to bridge between the extendr-facing binding types and
        // the core Rust types.  Without these impls the generated `convert` and
        // builder methods fail with E0277 unsatisfied trait bound errors.
        let binding_to_core = alef_codegen::conversions::convertible_types(api);
        let core_to_binding = alef_codegen::conversions::core_to_binding_convertible_types(api);
        let input_types = alef_codegen::conversions::input_type_names(api);
        let extendr_conversion_cfg = alef_codegen::conversions::ConversionConfig::default();
        for typ in api.types.iter().filter(|typ| !typ.is_trait) {
            // binding→core: emit when type is used as input. Types with bridge fields are
            // not eligible for the standard convertibility check (their bridge fields
            // reference the trait type which has no binding counterpart), so emit a
            // hand-rolled From impl that skips them.
            if input_types.contains(&typ.name) {
                if let Some(skip_fields) = bridge_fields_by_type.get(typ.name.as_str()) {
                    builder.add_item(&gen_extendr_from_binding_to_core_skipping_fields(
                        typ,
                        &core_import,
                        skip_fields,
                    ));
                } else if alef_codegen::conversions::can_generate_conversion(typ, &binding_to_core) {
                    builder.add_item(&alef_codegen::conversions::gen_from_binding_to_core_cfg(
                        typ,
                        &core_import,
                        &extendr_conversion_cfg,
                    ));
                }
            }
            // core→binding: emit whenever the conversion can be generated.  Allows
            // `core_value.into()` in return positions.
            if alef_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&alef_codegen::conversions::gen_from_core_to_binding_cfg(
                    typ,
                    &core_import,
                    &opaque_types,
                    &extendr_conversion_cfg,
                ));
            }
        }
        for e in &api.enums {
            // Extendr emits enums as flat (unit-only) variants regardless of whether the
            // core enum has data — emit lossy From impls so containing structs can call
            // `.into()`.  Data is discarded across the boundary; the binding enum keeps
            // only the variant tag.
            if input_types.contains(&e.name) && alef_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&alef_codegen::conversions::gen_enum_from_binding_to_core(
                    e,
                    &core_import,
                ));
            }
            if alef_codegen::conversions::can_generate_enum_conversion_from_core(e) {
                builder.add_item(&alef_codegen::conversions::gen_enum_from_core_to_binding(
                    e,
                    &core_import,
                ));
            }
        }

        // Generate function bindings
        for func in &api.functions {
            let bridge_param = crate::trait_bridge::find_bridge_param(func, &config.trait_bridges);
            if let Some((param_idx, bridge_cfg)) = bridge_param {
                builder.add_item(&crate::trait_bridge::gen_bridge_function(
                    func,
                    param_idx,
                    bridge_cfg,
                    self,
                    &opaque_types,
                    &core_import,
                ));
                continue;
            }
            let bridge_field = crate::trait_bridge::find_bridge_field(func, &api.types, &config.trait_bridges);
            if let Some(bridge_match) = bridge_field {
                builder.add_item(&crate::trait_bridge::gen_bridge_field_function(
                    func,
                    &bridge_match,
                    self,
                    &opaque_types,
                    &core_import,
                ));
                continue;
            }
            builder.add_item(&generators::gen_function(
                func,
                self,
                &cfg,
                &adapter_bodies,
                &opaque_types,
            ));
        }

        // Trait bridge wrappers — generate extendr bridge structs that delegate to R list objects
        for bridge_cfg in &config.trait_bridges {
            if let Some(trait_type) = api.types.iter().find(|t| t.is_trait && t.name == bridge_cfg.trait_name) {
                let bridge = crate::trait_bridge::gen_trait_bridge(
                    trait_type,
                    bridge_cfg,
                    &core_import,
                    &config.error_type(),
                    &config.error_constructor(),
                    api,
                );
                for imp in &bridge.imports {
                    builder.add_import(imp);
                }
                builder.add_item(&bridge.code);
            }
        }

        // Module registration
        let module_name = config.r_package_name().replace('-', "_");
        let module_items = format!(
            "extendr_module! {{\n    mod {module};\n{types}{funcs}}}\n",
            module = module_name,
            types = api
                .types
                .iter()
                .map(|t| format!("    impl {};\n", t.name))
                .collect::<String>(),
            funcs = api
                .functions
                .iter()
                .map(|f| format!("    fn {};\n", f.name))
                .collect::<String>(),
        );
        builder.add_item(&module_items);

        let output_path = resolve_output_dir(
            config.output.r.as_ref(),
            &config.crate_config.name,
            "packages/r/src/rust/src",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_path).join("lib.rs"),
            content: builder.build(),
            generated_header: false,
        }])
    }

    fn generate_public_api(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let package_name = config.r_package_name();
        let prefix = config.ffi_prefix();

        // Generate R namespace file with wrapper functions
        let mut content = hash::header(CommentStyle::Hash);
        content.push('\n');

        // Add useDynLib directive
        content.push_str(&format!("#' @useDynLib {}, .registration = TRUE\n", package_name));
        content.push_str("NULL\n\n");

        // Generate wrapper functions for all API functions
        for func in &api.functions {
            // Emit roxygen documentation
            doc_emission::emit_roxygen(&mut content, &func.doc);
            // Add @export tag for public functions
            content.push_str("#' @export\n");

            content.push_str(&format!("{} <- function(", func.name));

            // Parameters with default values
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    if p.optional {
                        format!("{} = NULL", p.name)
                    } else {
                        p.name.clone()
                    }
                })
                .collect();
            content.push_str(&params.join(", "));

            content.push_str(") {\n");

            // Call the native function
            let param_names: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();
            content.push_str(&format!(
                "  .Call(\"{}_{}\"{})\n",
                prefix,
                func.name,
                if param_names.is_empty() {
                    String::new()
                } else {
                    format!(", {}", param_names.join(", "))
                }
            ));
            content.push_str("}\n\n");
        }

        // The R wrapper file always goes into the package's R/ directory (e.g. packages/r/R/).
        // We derive this from the rust output path: strip the conventional Rust-source suffix
        // (src/rust/src) and append R/, falling back to the hardcoded default.
        let r_wrapper_dir = if let Some(rust_out) = config.output.r.as_ref() {
            let rust_str = rust_out.to_string_lossy();
            // Strip trailing separator variants of "src/rust/src"
            let suffixes = ["src/rust/src/", "src/rust/src"];
            let base = suffixes
                .iter()
                .find_map(|s| rust_str.strip_suffix(s))
                .unwrap_or_else(|| rust_str.as_ref());
            format!("{base}R/")
        } else {
            "packages/r/R/".to_string()
        };

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&r_wrapper_dir).join(format!("{}.R", package_name)),
            content,
            generated_header: false,
        }])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "cargo",
            crate_suffix: "-extendr",
            build_dep: BuildDependency::None,
            post_build: vec![],
        })
    }
}

/// Generate an extendr binding struct where the named bridge fields are emitted as
/// `Option<extendr_api::Robj>` with `#[serde(skip)]` instead of their core IR type.
///
/// R callers attach a visitor (or other trait bridge handle) by setting the field on the
/// options list to a closure / R object. The convert wrapper extracts it before forwarding
/// the options to the core function.
fn gen_extendr_struct_with_bridge_fields(
    typ: &TypeDef,
    mapper: &dyn TypeMapper,
    bridge_fields: &HashSet<String>,
) -> String {
    let mut sb = alef_codegen::builder::StructBuilder::new(&typ.name);
    sb.add_derive("Clone");
    sb.add_derive("Default");
    sb.add_derive("serde::Serialize");
    sb.add_derive("serde::Deserialize");
    if typ.has_default {
        sb.add_attr("serde(default)");
    }
    for field in &typ.fields {
        if field.cfg.is_some() {
            continue;
        }
        if bridge_fields.contains(&field.name) {
            sb.add_field_with_doc(
                &field.name,
                "Option<extendr_api::Robj>",
                vec!["serde(skip)".to_string()],
                &field.doc,
            );
            continue;
        }
        let ty = if field.optional && !matches!(field.ty, TypeRef::Optional(_)) {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        let mut attrs: Vec<String> = Vec::new();
        if field.sanitized {
            attrs.push("serde(skip)".to_string());
        }
        sb.add_field_with_doc(&field.name, &ty, attrs, &field.doc);
    }
    sb.build()
}

/// Like `alef_codegen::config_gen::gen_extendr_kwargs_constructor` but renders bridge fields as
/// `Option<extendr_api::Robj>` so R callers can pass an R closure or list to the field.
fn gen_extendr_kwargs_constructor_with_bridge_fields(
    typ: &TypeDef,
    type_mapper: &dyn Fn(&TypeRef) -> String,
    bridge_fields: &HashSet<String>,
) -> String {
    let mut out = String::with_capacity(512);
    writeln!(out, "#[extendr]").ok();
    writeln!(out, "pub fn new_{}(", typ.name.to_lowercase()).ok();
    for (i, field) in typ.fields.iter().enumerate() {
        let comma = if i < typ.fields.len() - 1 { "," } else { "" };
        if bridge_fields.contains(&field.name) {
            writeln!(out, "    {}: Option<extendr_api::Robj>{}", field.name, comma).ok();
        } else {
            let field_type = type_mapper(&field.ty);
            writeln!(out, "    {}: Option<{}>{}", field.name, field_type, comma).ok();
        }
    }
    writeln!(out, ") -> {} {{", typ.name).ok();
    writeln!(out, "    let mut __out = <{}>::default();", typ.name).ok();
    for field in &typ.fields {
        if bridge_fields.contains(&field.name) {
            // Bridge fields are `Option<extendr_api::Robj>` on the binding struct — wrap in Some.
            writeln!(
                out,
                "    if let Some(v) = {name} {{ __out.{name} = Some(v); }}",
                name = field.name
            )
            .ok();
        } else {
            writeln!(
                out,
                "    if let Some(v) = {name} {{ __out.{name} = v; }}",
                name = field.name
            )
            .ok();
        }
    }
    writeln!(out, "    __out").ok();
    writeln!(out, "}}").ok();
    out
}

/// Custom `From<{Binding}> for {core::Type}` impl that leaves the named bridge fields at
/// their `Default` value (typically `None`). The convert wrapper sets them after building
/// the bridge from the field value.
fn gen_extendr_from_binding_to_core_skipping_fields(
    typ: &TypeDef,
    core_import: &str,
    skip_fields: &HashSet<String>,
) -> String {
    use alef_core::ir::PrimitiveType;

    let core_path = alef_codegen::conversions::core_type_path(typ, core_import);
    let binding_name = &typ.name;
    let mut out = String::with_capacity(512);
    writeln!(out, "#[allow(clippy::redundant_closure, clippy::useless_conversion)]").ok();
    writeln!(out, "impl From<{binding_name}> for {core_path} {{").ok();
    writeln!(out, "    fn from(val: {binding_name}) -> Self {{").ok();
    writeln!(out, "        let mut __result = {core_path}::default();").ok();
    for field in &typ.fields {
        if skip_fields.contains(&field.name) || field.sanitized || field.cfg.is_some() {
            continue;
        }
        let conversion = match &field.ty {
            TypeRef::Named(_) => {
                if field.optional || matches!(&field.ty, TypeRef::Optional(_)) {
                    format!("val.{0}.map(Into::into)", field.name)
                } else {
                    format!("val.{0}.into()", field.name)
                }
            }
            TypeRef::Optional(inner) => match inner.as_ref() {
                TypeRef::Named(_) => format!("val.{0}.map(Into::into)", field.name),
                _ => format!("val.{}", field.name),
            },
            TypeRef::Duration => {
                if field.optional {
                    format!("val.{0}.map(std::time::Duration::from_millis)", field.name)
                } else {
                    format!("std::time::Duration::from_millis(val.{})", field.name)
                }
            }
            TypeRef::Primitive(PrimitiveType::I64) | TypeRef::Primitive(PrimitiveType::U64) => {
                format!("val.{}", field.name)
            }
            _ => format!("val.{}", field.name),
        };
        writeln!(out, "        __result.{} = {conversion};", field.name).ok();
    }
    writeln!(out, "        __result").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}
