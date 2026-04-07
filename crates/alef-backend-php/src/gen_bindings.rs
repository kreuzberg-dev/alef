use crate::type_map::PhpMapper;
use ahash::AHashSet;
use alef_codegen::builder::{ImplBuilder, RustFileBuilder};
use alef_codegen::conversions::ConversionConfig;
use alef_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use alef_codegen::shared::{self, constructor_parts, partition_methods};
use alef_codegen::type_mapper::TypeMapper;
use alef_core::backend::{Backend, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, detect_serde_available, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, PrimitiveType, TypeDef, TypeRef};
use heck::ToPascalCase;
use std::fmt::Write;
use std::path::PathBuf;

pub struct PhpBackend;

impl PhpBackend {
    fn binding_config(core_import: &str, has_serde: bool) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &["php_class"],
            field_attrs: &[],
            struct_derives: &["Clone"],
            method_block_attr: Some("php_impl"),
            constructor_attr: "",
            static_attr: None,
            function_attr: "#[php_function]",
            enum_attrs: &[],
            enum_derives: &[],
            needs_signature: false,
            signature_prefix: "",
            signature_suffix: "",
            core_import,
            async_pattern: AsyncPattern::TokioBlockOn,
            has_serde,
            type_name_prefix: "",
        }
    }
}

impl Backend for PhpBackend {
    fn name(&self) -> &str {
        "php"
    }

    fn language(&self) -> Language {
        Language::Php
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

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let enum_names = api.enums.iter().map(|e| e.name.clone()).collect();
        let mapper = PhpMapper { enum_names };
        let core_import = config.core_import();

        let output_dir = resolve_output_dir(
            config.output.php.as_ref(),
            &config.crate_config.name,
            "crates/{name}-php/src/",
        );
        let has_serde = detect_serde_available(&output_dir);
        let cfg = Self::binding_config(&core_import, has_serde);

        // Build the inner module content (types, methods, conversions)
        let mut builder = RustFileBuilder::new();
        builder.add_import("ext_php_rs::prelude::*");

        // Import serde_json when available (needed for serde-based param conversion)
        if has_serde {
            builder.add_import("serde_json");
        }

        // Import traits needed for trait method dispatch
        for trait_path in generators::collect_trait_imports(api) {
            builder.add_import(&trait_path);
        }

        // Only import HashMap when Map-typed fields or returns are present
        let has_maps = api
            .types
            .iter()
            .any(|t| t.fields.iter().any(|f| matches!(&f.ty, TypeRef::Map(_, _))))
            || api
                .functions
                .iter()
                .any(|f| matches!(&f.return_type, TypeRef::Map(_, _)));
        if has_maps {
            builder.add_import("std::collections::HashMap");
        }

        // Custom module declarations
        let custom_mods = config.custom_modules.for_language(Language::Php);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

        // Check if any function or method is async
        let has_async =
            api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));

        if has_async {
            builder.add_item(&gen_tokio_runtime());
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
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &opaque_types, &core_import));
            } else {
                builder.add_item(&gen_php_struct(typ, &mapper, &cfg));
                if typ.has_default {
                    builder.add_item(&generators::gen_struct_default_impl(typ, ""));
                }
                builder.add_item(&gen_struct_methods(
                    typ,
                    &mapper,
                    has_serde,
                    &core_import,
                    &opaque_types,
                ));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum_constants(enum_def));
        }

        for func in &api.functions {
            if func.is_async {
                builder.add_item(&gen_async_function(func, &mapper, &opaque_types, &core_import));
            } else {
                builder.add_item(&gen_function(func, &mapper, &opaque_types, &core_import));
            }
        }

        let convertible = alef_codegen::conversions::convertible_types(api);
        let core_to_binding = alef_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions with PHP-specific i64 casts.
        // Types with enum Named fields (or that reference such types transitively) can't
        // have binding→core From impls because PHP maps enums to String and there's no
        // From<String> for the core enum type. Core→binding is always safe.
        let enum_names_ref = &mapper.enum_names;
        let php_conv_config = ConversionConfig {
            cast_large_ints_to_i64: true,
            enum_string_names: Some(enum_names_ref),
            json_to_string: true,
            ..Default::default()
        };
        // Build transitive set of types that can't have binding→core From
        let mut enum_tainted: AHashSet<String> = AHashSet::new();
        for typ in &api.types {
            if has_enum_named_field(typ, enum_names_ref) {
                enum_tainted.insert(typ.name.clone());
            }
        }
        // Transitively mark types that reference enum-tainted types
        let mut changed = true;
        while changed {
            changed = false;
            for typ in &api.types {
                if !enum_tainted.contains(&typ.name)
                    && typ.fields.iter().any(|f| references_named_type(&f.ty, &enum_tainted))
                {
                    enum_tainted.insert(typ.name.clone());
                    changed = true;
                }
            }
        }
        // Compute which enum-tainted types can have binding→core From generated
        // (excludes types referencing enums with data variants).
        let convertible_tainted = gen_convertible_enum_tainted(&api.types, &enum_tainted, enum_names_ref, &api.enums);
        for typ in &api.types {
            // binding→core: only when not enum-tainted
            if !enum_tainted.contains(&typ.name)
                && alef_codegen::conversions::can_generate_conversion(typ, &convertible)
            {
                builder.add_item(&alef_codegen::conversions::gen_from_binding_to_core_cfg(
                    typ,
                    &core_import,
                    &php_conv_config,
                ));
            } else if enum_tainted.contains(&typ.name) && has_serde {
                // Enum-tainted types can't use field-by-field From (no From<String> for core enum),
                // but when serde is available we bridge via JSON serialization round-trip.
                builder.add_item(&gen_serde_bridge_from(typ, &core_import));
            } else if convertible_tainted.contains(&typ.name) {
                // Enum-tainted types with only unit-variant enums: generate From with
                // string→enum parsing for enum-Named fields, using first variant as fallback.
                builder.add_item(&gen_enum_tainted_from_binding_to_core(
                    typ,
                    &core_import,
                    enum_names_ref,
                    &enum_tainted,
                    &php_conv_config,
                    &api.enums,
                ));
            }
            // core→binding: always (enum→String via format, sanitized fields via format)
            if alef_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&alef_codegen::conversions::gen_from_core_to_binding_cfg(
                    typ,
                    &core_import,
                    &opaque_types,
                    &php_conv_config,
                ));
            }
        }

        // Error converter functions
        for error in &api.errors {
            builder.add_item(&alef_codegen::error_gen::gen_php_error_converter(error, &core_import));
        }

        let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Php)?;

        // Add feature gate as inner attribute — entire crate is gated
        let php_config = config.php.as_ref();
        if let Some(feature_name) = php_config.and_then(|c| c.feature_gate.as_deref()) {
            builder.add_inner_attribute(&format!("cfg(feature = \"{feature_name}\")"));
            builder.add_inner_attribute(&format!(
                "cfg_attr(all(windows, target_env = \"msvc\", feature = \"{feature_name}\"), feature(abi_vectorcall))"
            ));
        }

        let content = builder.build();

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }

    fn generate_public_api(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let extension_name = config.php_extension_name();
        let class_name = extension_name.to_pascal_case();

        // Generate PHP wrapper class
        let mut content = String::from("<?php\n");
        content.push_str("// This file is auto-generated by alef. DO NOT EDIT.\n");
        content.push_str("declare(strict_types=1);\n\n");

        // Determine namespace
        let namespace = if extension_name.contains('_') {
            let parts: Vec<&str> = extension_name.split('_').collect();
            let ns_parts: Vec<String> = parts.iter().map(|p| p.to_pascal_case()).collect();
            ns_parts.join("\\")
        } else {
            class_name.clone()
        };

        content.push_str(&format!("namespace {};\n\n", namespace));
        content.push_str(&format!("final class {}\n", class_name));
        content.push_str("{\n");

        // Generate wrapper methods for functions
        for func in &api.functions {
            content.push_str("    /**\n");
            content.push_str(&format!("     * {}\n", func.doc.lines().next().unwrap_or("Function")));
            content.push_str("     */\n");
            content.push_str(&format!("    public static function {}(", func.name));

            // Parameters
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    if p.optional {
                        format!("?${} = null", p.name)
                    } else {
                        format!("${}", p.name)
                    }
                })
                .collect();
            content.push_str(&params.join(", "));
            content.push_str(")\n");
            content.push_str("    {\n");
            content.push_str(&format!(
                "        return \\{}({}); // delegate to extension function\n",
                func.name,
                func.params
                    .iter()
                    .map(|p| format!("${}", p.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            content.push_str("    }\n\n");
        }

        content.push_str("}\n");

        let output_dir = resolve_output_dir(
            config.output.php.as_ref(),
            &config.crate_config.name,
            "packages/php/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join(format!("{}.php", class_name)),
            content,
            generated_header: false,
        }])
    }
}

/// Generate ext-php-rs methods for an opaque struct (delegates to self.inner).
fn gen_opaque_struct_methods(
    typ: &TypeDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(
                method,
                mapper,
                true,
                &typ.name,
                opaque_types,
            ));
        } else {
            impl_builder.add_method(&gen_instance_method(method, mapper, true, &typ.name, opaque_types));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper, opaque_types));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper, opaque_types, typ, core_import));
        }
    }

    impl_builder.build()
}

/// Generate a PHP struct, adding `serde::Deserialize` when serde is available.
/// All structs need Deserialize (not just those with Named params) because
/// structs with from_json may reference other structs that also need Deserialize.
fn gen_php_struct(typ: &TypeDef, mapper: &PhpMapper, cfg: &RustBindingConfig<'_>) -> String {
    if cfg.has_serde {
        // Build a modified config that also derives Deserialize so from_json can work.
        let mut extra_derives: Vec<&str> = cfg.struct_derives.to_vec();
        extra_derives.push("serde::Deserialize");
        let modified_cfg = RustBindingConfig {
            struct_attrs: cfg.struct_attrs,
            field_attrs: cfg.field_attrs,
            struct_derives: &extra_derives,
            method_block_attr: cfg.method_block_attr,
            constructor_attr: cfg.constructor_attr,
            static_attr: cfg.static_attr,
            function_attr: cfg.function_attr,
            enum_attrs: cfg.enum_attrs,
            enum_derives: cfg.enum_derives,
            needs_signature: cfg.needs_signature,
            signature_prefix: cfg.signature_prefix,
            signature_suffix: cfg.signature_suffix,
            core_import: cfg.core_import,
            async_pattern: cfg.async_pattern,
            has_serde: cfg.has_serde,
            type_name_prefix: cfg.type_name_prefix,
        };
        generators::gen_struct(typ, mapper, &modified_cfg)
    } else {
        generators::gen_struct(typ, mapper, cfg)
    }
}

/// Return true if a TypeRef contains a Named type (another struct/class that
/// ext-php-rs cannot deserialize from a PHP value as an owned parameter).
fn type_ref_has_named(ty: &alef_core::ir::TypeRef) -> bool {
    use alef_core::ir::TypeRef;
    match ty {
        TypeRef::Named(_) => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_named(inner),
        TypeRef::Map(k, v) => type_ref_has_named(k) || type_ref_has_named(v),
        _ => false,
    }
}

/// Generate ext-php-rs methods for a struct.
fn gen_struct_methods(
    typ: &TypeDef,
    mapper: &PhpMapper,
    has_serde: bool,
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    if !typ.fields.is_empty() {
        let has_named_params = typ.fields.iter().any(|f| type_ref_has_named(&f.ty));
        if has_named_params {
            if has_serde {
                let constructor = "pub fn from_json(json: String) -> PhpResult<Self> {\n    \
                     serde_json::from_str(&json)\n        \
                     .map_err(|e| PhpException::default(e.to_string()).into())\n\
                     }"
                .to_string();
                impl_builder.add_method(&constructor);
            } else {
                let constructor = format!(
                    "pub fn __construct() -> PhpResult<Self> {{\n    \
                     Err(PhpException::default(\"Not implemented: constructor for {} requires complex params\".to_string()).into())\n\
                     }}",
                    typ.name
                );
                impl_builder.add_method(&constructor);
            }
        } else {
            let map_fn = |ty: &alef_core::ir::TypeRef| mapper.map_type(ty);
            if typ.has_default {
                // kwargs-style constructor: all fields optional with defaults
                let config_method = alef_codegen::config_gen::gen_php_kwargs_constructor(typ, &map_fn);
                impl_builder.add_method(&config_method);
            } else {
                // Normal positional constructor
                let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
                let constructor = format!(
                    "pub fn __construct({param_list}) -> Self {{\n    \
                     Self {{ {assignments} }}\n\
                     }}"
                );
                impl_builder.add_method(&constructor);
            }
        }
    }

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(
                method,
                mapper,
                false,
                &typ.name,
                opaque_types,
            ));
        } else {
            impl_builder.add_method(&gen_instance_method_non_opaque(
                method,
                mapper,
                typ,
                core_import,
                opaque_types,
            ));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper, opaque_types));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper, opaque_types, typ, core_import));
        }
    }

    impl_builder.build()
}

/// Generate an instance method binding for an opaque struct.
fn gen_instance_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    is_opaque: bool,
    type_name: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let params_str = if params.is_empty() { String::new() } else { params };

    // Exclude methods with non-opaque Named params: ext-php-rs can't pass #[php_class]
    // types by value (no FromZvalMut impl for owned php_class types).
    let has_non_opaque_named_params = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if !opaque_types.contains(n.as_str())));

    let body = if can_delegate && is_opaque && !has_non_opaque_named_params {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let is_owned_receiver = matches!(method.receiver.as_ref(), Some(alef_core::ir::ReceiverKind::Owned));
        let core_call = if is_owned_receiver {
            format!("(*self.inner).clone().{}({})", method.name, call_args)
        } else {
            format!("self.inner.{}({})", method.name, call_args)
        };
        if method.error_type.is_some() {
            if matches!(method.return_type, TypeRef::Unit) {
                format!(
                    "{core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(())"
                )
            } else {
                let wrap = generators::wrap_return(
                    "result",
                    &method.return_type,
                    type_name,
                    opaque_types,
                    true,
                    method.returns_ref,
                );
                format!(
                    "let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
                )
            }
        } else {
            generators::wrap_return(
                &core_call,
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            )
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params_str.is_empty() {
        format!(
            "{trait_allow}pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate an instance method binding for a non-opaque struct (uses gen_lossy_binding_to_core_fields).
fn gen_instance_method_non_opaque(
    method: &MethodDef,
    mapper: &PhpMapper,
    typ: &TypeDef,
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = !method.sanitized
        && method
            .params
            .iter()
            .all(|p| !p.sanitized && generators::is_simple_non_opaque_param(&p.ty))
        && shared::is_delegatable_return(&method.return_type);

    let params_str = if params.is_empty() { String::new() } else { params };

    let body = if can_delegate {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let field_conversions = gen_php_lossy_binding_to_core_fields(typ, core_import);
        let core_call = format!("core_self.{}({})", method.name, call_args);
        let result_wrap = match &method.return_type {
            TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()) => {
                // Enum return type: PHP maps enums to String via format!("{:?}")
                String::new()
            }
            TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => ".into()".to_string(),
            _ => String::new(),
        };
        // For enum return types, wrap with format!("{:?}", ...)
        let format_enum_return =
            matches!(&method.return_type, TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()));
        if method.error_type.is_some() {
            if format_enum_return {
                format!(
                    "{field_conversions}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(format!(\"{{:?}}\", result))"
                )
            } else {
                format!(
                    "{field_conversions}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(result{result_wrap})"
                )
            }
        } else if format_enum_return {
            format!("{field_conversions}format!(\"{{:?}}\", {core_call})")
        } else {
            format!("{field_conversions}{core_call}{result_wrap}")
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params_str.is_empty() {
        format!(
            "{trait_allow}pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate a static method binding.
fn gen_static_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    typ: &TypeDef,
    _core_import: &str,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);
    let core_type_path = typ.rust_path.replace('-', "_");
    let call_args = gen_php_call_args(&method.params, opaque_types);

    // Exclude methods with non-opaque Named params (FromZvalMut issue with php_class)
    let has_non_opaque_named_params = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if !opaque_types.contains(n.as_str())));

    let body = if can_delegate && !has_non_opaque_named_params {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        let is_enum_return = matches!(&method.return_type, TypeRef::Named(n) if mapper.enum_names.contains(n.as_str()));
        if method.error_type.is_some() {
            if is_enum_return {
                format!(
                    "{core_call}.map(|val| format!(\"{{:?}}\", val)).map_err(|e| PhpException::default(e.to_string()))"
                )
            } else {
                let wrap = generators::wrap_return(
                    "val",
                    &method.return_type,
                    &typ.name,
                    opaque_types,
                    typ.is_opaque,
                    method.returns_ref,
                );
                if wrap == "val" {
                    format!("{core_call}.map_err(|e| PhpException::default(e.to_string()))")
                } else {
                    format!("{core_call}.map(|val| {wrap}).map_err(|e| PhpException::default(e.to_string()))")
                }
            }
        } else if is_enum_return {
            format!("format!(\"{{:?}}\", {core_call})")
        } else {
            generators::wrap_return(
                &core_call,
                &method.return_type,
                &typ.name,
                opaque_types,
                typ.is_opaque,
                method.returns_ref,
            )
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let trait_allow = if generators::is_trait_method_name(&method.name) {
        "#[allow(clippy::should_implement_trait)]\n"
    } else {
        ""
    };
    if params.is_empty() {
        format!(
            "{trait_allow}pub fn {}() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "{trait_allow}pub fn {}({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate PHP enum constants (enums as string constants).
fn gen_enum_constants(enum_def: &EnumDef) -> String {
    let mut lines = vec![format!("// {} enum values", enum_def.name)];

    for variant in &enum_def.variants {
        let const_name = format!("{}_{}", enum_def.name.to_uppercase(), variant.name.to_uppercase());
        lines.push(format!("pub const {}: &str = \"{}\";", const_name, variant.name));
    }

    lines.join("\n")
}

/// Generate a free function binding.
fn gen_function(func: &FunctionDef, mapper: &PhpMapper, opaque_types: &AHashSet<String>, core_import: &str) -> String {
    let params = gen_php_function_params(&func.params, mapper, opaque_types);
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let let_bindings = gen_php_named_let_bindings(&func.params, opaque_types, core_import);
        let call_args = gen_php_call_args_with_let_bindings(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        if func.error_type.is_some() {
            let wrap = generators::wrap_return("result", &func.return_type, "", opaque_types, false, func.returns_ref);
            format!(
                "{let_bindings}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
            )
        } else {
            format!(
                "{let_bindings}{}",
                generators::wrap_return(&core_call, &func.return_type, "", opaque_types, false, func.returns_ref)
            )
        }
    } else {
        gen_php_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some())
    };

    if params.is_empty() {
        format!(
            "#[php_function]\npub fn {}() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    } else {
        format!(
            "#[php_function]\npub fn {}({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    }
}

/// Generate an async free function binding for PHP (block on runtime).
fn gen_async_function(
    func: &FunctionDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let params = gen_php_function_params(&func.params, mapper, opaque_types);
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let let_bindings = gen_php_named_let_bindings(&func.params, opaque_types, core_import);
        let call_args = gen_php_call_args_with_let_bindings(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let result_wrap =
            generators::wrap_return("result", &func.return_type, "", opaque_types, false, func.returns_ref);
        if func.error_type.is_some() {
            format!(
                "{let_bindings}WORKER_RUNTIME.block_on(async {{\n        \
                 let result = {core_call}.await.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n        \
                 Ok({result_wrap})\n    }})"
            )
        } else {
            format!(
                "{let_bindings}let result = WORKER_RUNTIME.block_on(async {{ {core_call}.await }});\n    {result_wrap}"
            )
        }
    } else {
        gen_php_unimplemented_body(
            &func.return_type,
            &format!("{}_async", func.name),
            func.error_type.is_some(),
        )
    };

    if params.is_empty() {
        format!(
            "#[php_function]\npub fn {}_async() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    } else {
        format!(
            "#[php_function]\npub fn {}_async({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            func.name
        )
    }
}

/// Generate an async instance method binding for PHP (block on runtime).
fn gen_async_instance_method(
    method: &MethodDef,
    mapper: &PhpMapper,
    is_opaque: bool,
    type_name: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate && is_opaque {
        let call_args = gen_php_call_args(&method.params, opaque_types);
        let inner_clone = "let inner = self.inner.clone();\n    ";
        let core_call = format!("inner.{}({})", method.name, call_args);
        let result_wrap = generators::wrap_return(
            "result",
            &method.return_type,
            type_name,
            opaque_types,
            true,
            method.returns_ref,
        );
        if method.error_type.is_some() {
            format!(
                "{inner_clone}WORKER_RUNTIME.block_on(async {{\n        \
                 let result = {core_call}.await.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n        \
                 Ok({result_wrap})\n    }})"
            )
        } else {
            format!(
                "{inner_clone}let result = WORKER_RUNTIME.block_on(async {{ {core_call}.await }});\n    {result_wrap}"
            )
        }
    } else {
        gen_php_unimplemented_body(
            &method.return_type,
            &format!("{}_async", method.name),
            method.error_type.is_some(),
        )
    };

    if params.is_empty() {
        format!(
            "pub fn {}_async(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}_async(&self, {params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate an async static method binding for PHP (block on runtime).
fn gen_async_static_method(method: &MethodDef, mapper: &PhpMapper, opaque_types: &AHashSet<String>) -> String {
    let params = gen_php_function_params(&method.params, mapper, opaque_types);
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_php_unimplemented_body(
        &method.return_type,
        &format!("{}_async", method.name),
        method.error_type.is_some(),
    );

    if params.is_empty() {
        format!(
            "pub fn {}_async() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}_async({params}) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    }
}

/// Generate a type-appropriate unimplemented body for PHP (no todo!()).
fn gen_php_unimplemented_body(return_type: &alef_core::ir::TypeRef, fn_name: &str, has_error: bool) -> String {
    use alef_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(ext_php_rs::exception::PhpException::default(\"{err_msg}\".to_string()).into())")
    } else {
        match return_type {
            TypeRef::Unit => "()".to_string(),
            TypeRef::String | TypeRef::Path => format!("String::from(\"[unimplemented: {fn_name}]\")"),
            TypeRef::Bytes => "Vec::new()".to_string(),
            TypeRef::Primitive(p) => match p {
                alef_core::ir::PrimitiveType::Bool => "false".to_string(),
                _ => "0".to_string(),
            },
            TypeRef::Optional(_) => "None".to_string(),
            TypeRef::Vec(_) => "Vec::new()".to_string(),
            TypeRef::Map(_, _) => "Default::default()".to_string(),
            TypeRef::Named(_) | TypeRef::Json => format!("panic!(\"alef: {fn_name} not auto-delegatable\")"),
            TypeRef::Duration => "std::time::Duration::default()".to_string(),
        }
    }
}

/// Generate a serde JSON bridge `impl From<BindingType> for core::Type`.
/// Used for enum-tainted types where field-by-field From can't work (no From<String> for core enums),
/// but serde can round-trip through JSON since the binding type derives Serialize and the core type
/// derives Deserialize.
fn gen_serde_bridge_from(typ: &TypeDef, core_import: &str) -> String {
    let core_path = alef_codegen::conversions::core_type_path(typ, core_import);
    format!(
        "impl From<{}> for {} {{\n    \
         fn from(val: {}) -> Self {{\n        \
         let json = serde_json::to_string(&val).expect(\"alef: serialize binding type\");\n        \
         serde_json::from_str(&json).expect(\"alef: deserialize to core type\")\n    \
         }}\n\
         }}",
        typ.name, core_path, typ.name
    )
}

/// Return true if any field of the type (recursively through Optional/Vec) is a Named type
/// that is an enum. PHP maps enum Named types to String, so From/Into impls would need
/// From<String> for the core enum which doesn't exist — skip generation for such types.
/// Check if a TypeRef references any type in the given set (transitively through containers).
fn references_named_type(ty: &alef_core::ir::TypeRef, names: &AHashSet<String>) -> bool {
    use alef_core::ir::TypeRef;
    match ty {
        TypeRef::Named(name) => names.contains(name.as_str()),
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => references_named_type(inner, names),
        TypeRef::Map(k, v) => references_named_type(k, names) || references_named_type(v, names),
        _ => false,
    }
}

fn has_enum_named_field(typ: &alef_core::ir::TypeDef, enum_names: &AHashSet<String>) -> bool {
    fn type_ref_has_enum_named(ty: &alef_core::ir::TypeRef, enum_names: &AHashSet<String>) -> bool {
        use alef_core::ir::TypeRef;
        match ty {
            TypeRef::Named(name) => enum_names.contains(name.as_str()),
            TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_enum_named(inner, enum_names),
            TypeRef::Map(k, v) => type_ref_has_enum_named(k, enum_names) || type_ref_has_enum_named(v, enum_names),
            _ => false,
        }
    }
    typ.fields.iter().any(|f| type_ref_has_enum_named(&f.ty, enum_names))
}

/// Generate PHP-specific function parameter list.
/// Non-opaque Named types use `&T` (ext-php-rs only provides `FromZvalMut` for `&mut T`/`&T`,
/// not owned `T`, when `T` is a `#[php_class]`).
fn gen_php_function_params(
    params: &[alef_core::ir::ParamDef],
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
) -> String {
    params
        .iter()
        .map(|p| {
            let base_ty = mapper.map_type(&p.ty);
            let ty = match &p.ty {
                TypeRef::Named(name) if !opaque_types.contains(name.as_str()) => {
                    // Non-opaque php_class type: use &T for ext-php-rs compatibility
                    if p.optional {
                        format!("Option<&{base_ty}>")
                    } else {
                        format!("&{base_ty}")
                    }
                }
                _ => {
                    if p.optional {
                        format!("Option<{base_ty}>")
                    } else {
                        base_ty
                    }
                }
            };
            format!("{}: {}", p.name, ty)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate PHP-specific call arguments.
/// Non-opaque Named types are passed as `&T`, so we clone before `.into()`.
fn gen_php_call_args(params: &[alef_core::ir::ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if p.optional {
                    format!("{}.as_ref().map(|v| &v.inner)", p.name)
                } else {
                    format!("&{}.inner", p.name)
                }
            }
            TypeRef::Named(_) => {
                // Non-opaque: param is &T, clone then convert
                if p.optional {
                    format!("{}.map(|v| v.clone().into())", p.name)
                } else {
                    format!("{}.clone().into()", p.name)
                }
            }
            TypeRef::String => format!("&{}", p.name),
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            TypeRef::Duration => format!("std::time::Duration::from_secs({})", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate let bindings for non-opaque Named params in free functions.
/// Creates `let {name}_core: {core_import}::{TypeName} = {name}.clone().into();`
/// so the function body can pass `&{name}_core` instead of `{name}.clone().into()`.
fn gen_php_named_let_bindings(
    params: &[alef_core::ir::ParamDef],
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let mut out = String::new();
    for p in params {
        if let TypeRef::Named(name) = &p.ty {
            if !opaque_types.contains(name.as_str()) {
                if p.optional {
                    writeln!(
                        out,
                        "let {}_core: Option<{core_import}::{name}> = {}.map(|v| v.clone().into());",
                        p.name, p.name
                    )
                    .ok();
                } else {
                    writeln!(
                        out,
                        "let {}_core: {core_import}::{name} = {}.clone().into();",
                        p.name, p.name
                    )
                    .ok();
                }
            }
        }
    }
    out
}

/// Generate call args using pre-bound let bindings for non-opaque Named params.
fn gen_php_call_args_with_let_bindings(params: &[alef_core::ir::ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if p.optional {
                    format!("{}.as_ref().map(|v| &v.inner)", p.name)
                } else {
                    format!("&{}.inner", p.name)
                }
            }
            TypeRef::Named(_) => {
                format!("{}_core", p.name)
            }
            TypeRef::String => format!("&{}", p.name),
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            TypeRef::Duration => format!("std::time::Duration::from_secs({})", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns true if a primitive type needs i64→core casting in PHP.
fn needs_i64_cast(p: &PrimitiveType) -> bool {
    matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)
}

/// Returns the core primitive type string for i64-cast primitives.
fn core_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 => "u64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
        _ => unreachable!(),
    }
}

/// PHP-specific lossy binding→core struct literal.
/// Like `gen_lossy_binding_to_core_fields` but adds i64→usize casts for large-int primitives.
fn gen_php_lossy_binding_to_core_fields(typ: &TypeDef, core_import: &str) -> String {
    let core_path = alef_codegen::conversions::core_type_path(typ, core_import);
    let mut out = format!("let core_self = {core_path} {{\n");
    for field in &typ.fields {
        let name = &field.name;
        if field.sanitized {
            writeln!(out, "            {name}: Default::default(),").ok();
        } else {
            let expr = match &field.ty {
                TypeRef::Primitive(p) if needs_i64_cast(p) => {
                    let core_ty = core_prim_str(p);
                    if field.optional {
                        format!("self.{name}.map(|v| v as {core_ty})")
                    } else {
                        format!("self.{name} as {core_ty}")
                    }
                }
                TypeRef::Primitive(_) => format!("self.{name}"),
                TypeRef::Duration => {
                    if field.optional {
                        format!("self.{name}.map(|v| std::time::Duration::from_secs(v as u64))")
                    } else {
                        format!("std::time::Duration::from_secs(self.{name} as u64)")
                    }
                }
                TypeRef::String | TypeRef::Bytes => format!("self.{name}.clone()"),
                TypeRef::Path => {
                    if field.optional {
                        format!("self.{name}.clone().map(Into::into)")
                    } else {
                        format!("self.{name}.clone().into()")
                    }
                }
                TypeRef::Named(_) => {
                    if field.optional {
                        format!("self.{name}.clone().map(Into::into)")
                    } else {
                        format!("self.{name}.clone().into()")
                    }
                }
                TypeRef::Vec(inner) => match inner.as_ref() {
                    TypeRef::Named(_) => {
                        if field.optional {
                            format!("self.{name}.clone().map(|v| v.into_iter().map(Into::into).collect())")
                        } else {
                            format!("self.{name}.clone().into_iter().map(Into::into).collect()")
                        }
                    }
                    TypeRef::Primitive(p) if needs_i64_cast(p) => {
                        let core_ty = core_prim_str(p);
                        if field.optional {
                            format!("self.{name}.clone().map(|v| v.into_iter().map(|x| x as {core_ty}).collect())")
                        } else {
                            format!("self.{name}.clone().into_iter().map(|v| v as {core_ty}).collect()")
                        }
                    }
                    _ => format!("self.{name}.clone()"),
                },
                TypeRef::Optional(inner) => match inner.as_ref() {
                    TypeRef::Primitive(p) if needs_i64_cast(p) => {
                        let core_ty = core_prim_str(p);
                        format!("self.{name}.map(|v| v as {core_ty})")
                    }
                    TypeRef::Named(_) => {
                        format!("self.{name}.clone().map(Into::into)")
                    }
                    TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Named(_)) => {
                        format!("self.{name}.clone().map(|v| v.into_iter().map(Into::into).collect())")
                    }
                    TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) => {
                        if let TypeRef::Primitive(p) = vi.as_ref() {
                            let core_ty = core_prim_str(p);
                            format!("self.{name}.clone().map(|v| v.into_iter().map(|x| x as {core_ty}).collect())")
                        } else {
                            format!("self.{name}.clone()")
                        }
                    }
                    _ => format!("self.{name}.clone()"),
                },
                TypeRef::Map(_, _) => format!("self.{name}.clone()"),
                TypeRef::Unit => format!("self.{name}.clone()"),
                // Json maps to String in PHP — can't directly assign to serde_json::Value
                TypeRef::Json => "Default::default()".to_string(),
            };
            writeln!(out, "            {name}: {expr},").ok();
        }
    }
    // Use ..Default::default() to fill cfg-gated fields stripped from the IR
    if typ.has_stripped_cfg_fields {
        out.push_str("            ..Default::default()\n");
    }
    out.push_str("        };\n        ");
    out
}

/// Compute the set of enum-tainted types for which binding→core From CAN be generated.
/// A type is excluded if it references (directly or transitively) an enum with data variants,
/// because data-variant fields may reference types that don't implement Default.
fn gen_convertible_enum_tainted(
    types: &[TypeDef],
    enum_tainted: &AHashSet<String>,
    enum_names: &AHashSet<String>,
    enums: &[EnumDef],
) -> AHashSet<String> {
    // First, find which enum-tainted types directly reference data-variant enums
    let mut unconvertible: AHashSet<String> = AHashSet::new();
    for typ in types {
        if !enum_tainted.contains(&typ.name) {
            continue;
        }
        for field in &typ.fields {
            if let Some(enum_name) = get_direct_enum_named(&field.ty, enum_names) {
                if let Some(enum_def) = enums.iter().find(|e| e.name == enum_name) {
                    if enum_def.variants.iter().any(|v| !v.fields.is_empty()) {
                        unconvertible.insert(typ.name.clone());
                    }
                }
            }
        }
    }
    // Transitively exclude types that reference unconvertible types
    let mut changed = true;
    while changed {
        changed = false;
        for typ in types {
            if !enum_tainted.contains(&typ.name) || unconvertible.contains(&typ.name) {
                continue;
            }
            if typ.fields.iter().any(|f| references_named_type(&f.ty, &unconvertible)) {
                unconvertible.insert(typ.name.clone());
                changed = true;
            }
        }
    }
    // Return the set of enum-tainted types that CAN be converted
    enum_tainted
        .iter()
        .filter(|name| !unconvertible.contains(name.as_str()))
        .cloned()
        .collect()
}

/// Generate `impl From<BindingType> for core::Type` for enum-tainted types.
/// Enum-Named fields use string→enum parsing (match on variant names, first variant as fallback).
/// Fields referencing other enum-tainted struct types use `.into()` (their own From is also generated).
/// Non-enum fields use the normal conversion with i64 casts.
fn gen_enum_tainted_from_binding_to_core(
    typ: &TypeDef,
    core_import: &str,
    enum_names: &AHashSet<String>,
    _enum_tainted: &AHashSet<String>,
    config: &ConversionConfig,
    enums: &[EnumDef],
) -> String {
    let core_path = alef_codegen::conversions::core_type_path(typ, core_import);
    let mut out = String::with_capacity(512);
    writeln!(out, "impl From<{}> for {core_path} {{", typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let name = &field.name;
        if field.sanitized {
            writeln!(out, "            {name}: Default::default(),").ok();
        } else if let Some(enum_name) = get_direct_enum_named(&field.ty, enum_names) {
            // Direct enum-Named field: generate string→enum match
            let conversion =
                gen_string_to_enum_expr(&format!("val.{name}"), &enum_name, field.optional, enums, core_import);
            writeln!(out, "            {name}: {conversion},").ok();
        } else {
            // Non-enum field (may reference other tainted types, which have their own From)
            let conversion =
                alef_codegen::conversions::field_conversion_to_core_cfg(name, &field.ty, field.optional, config);
            writeln!(out, "            {conversion},").ok();
        }
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// If the TypeRef is a Named type referencing an enum, return the enum name.
fn get_direct_enum_named(ty: &TypeRef, enum_names: &AHashSet<String>) -> Option<String> {
    match ty {
        TypeRef::Named(name) if enum_names.contains(name.as_str()) => Some(name.clone()),
        TypeRef::Optional(inner) => get_direct_enum_named(inner, enum_names),
        _ => None,
    }
}

/// Generate an expression that converts a String to a core enum type via matching.
/// Falls back to the first variant if no match found.
/// Data variants (with fields) use `Default::default()` for each field.
fn gen_string_to_enum_expr(
    val_expr: &str,
    enum_name: &str,
    optional: bool,
    enums: &[EnumDef],
    core_import: &str,
) -> String {
    let enum_def = match enums.iter().find(|e| e.name == enum_name) {
        Some(e) => e,
        None => return "Default::default()".to_string(),
    };
    let core_enum_path = alef_codegen::conversions::core_enum_path(enum_def, core_import);

    if enum_def.variants.is_empty() {
        return "Default::default()".to_string();
    }

    /// Build the variant constructor expression, filling data variant fields with defaults.
    fn variant_expr(core_path: &str, variant: &alef_core::ir::EnumVariant) -> String {
        if variant.fields.is_empty() {
            format!("{core_path}::{}", variant.name)
        } else if alef_codegen::conversions::is_tuple_variant(&variant.fields) {
            let defaults: Vec<&str> = variant.fields.iter().map(|_| "Default::default()").collect();
            format!("{core_path}::{}({})", variant.name, defaults.join(", "))
        } else {
            let defaults: Vec<String> = variant
                .fields
                .iter()
                .map(|f| format!("{}: Default::default()", f.name))
                .collect();
            format!("{core_path}::{} {{ {} }}", variant.name, defaults.join(", "))
        }
    }

    let first_expr = variant_expr(&core_enum_path, &enum_def.variants[0]);
    let mut match_arms = String::new();
    for variant in &enum_def.variants {
        let expr = variant_expr(&core_enum_path, variant);
        write!(match_arms, "\"{}\" => {expr}, ", variant.name).ok();
    }
    write!(match_arms, "_ => {first_expr}").ok();

    if optional {
        format!("{val_expr}.as_deref().map(|s| match s {{ {match_arms} }})")
    } else {
        format!("match {val_expr}.as_str() {{ {match_arms} }}")
    }
}

/// Generate a global Tokio runtime for PHP async support.
fn gen_tokio_runtime() -> String {
    "static WORKER_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect(\"Failed to create Tokio runtime\")
});"
    .to_string()
}
