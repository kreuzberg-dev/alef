use crate::type_map::PhpMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder};
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_codegen::shared::{self, constructor_parts, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, detect_serde_available, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef, TypeRef};
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

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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

        // Clippy allows for generated code
        builder.add_inner_attribute("allow(unused_imports)");
        builder.add_inner_attribute("allow(clippy::too_many_arguments)");
        builder.add_inner_attribute("allow(clippy::missing_errors_doc)");
        builder.add_inner_attribute("allow(unused_variables)");
        builder.add_inner_attribute("allow(dead_code)");
        builder.add_inner_attribute("allow(clippy::should_implement_trait)");

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

        let convertible = skif_codegen::conversions::convertible_types(api);
        let core_to_binding = skif_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions with PHP-specific i64 casts.
        // Types with enum Named fields (or that reference such types transitively) can't
        // have binding→core From impls because PHP maps enums to String and there's no
        // From<String> for the core enum type. Core→binding is always safe.
        let enum_names_ref = &mapper.enum_names;
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
        for typ in &api.types {
            // binding→core: only when not enum-tainted
            if !enum_tainted.contains(&typ.name)
                && skif_codegen::conversions::can_generate_conversion(typ, &convertible)
            {
                builder.add_item(&gen_php_from_binding_to_core(typ, &core_import));
            } else if enum_tainted.contains(&typ.name) && has_serde {
                // Enum-tainted types can't use field-by-field From (no From<String> for core enum),
                // but when serde is available we bridge via JSON serialization round-trip.
                builder.add_item(&gen_serde_bridge_from(typ, &core_import));
            }
            // core→binding: always (enum→String via format, sanitized fields via format)
            if skif_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&gen_php_from_core_to_binding(typ, &core_import, enum_names_ref));
            }
        }

        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Php)?;

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
fn type_ref_has_named(ty: &skif_core::ir::TypeRef) -> bool {
    use skif_core::ir::TypeRef;
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
            let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
            let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
            let constructor = format!(
                "pub fn __construct({param_list}) -> Self {{\n    \
                 Self {{ {assignments} }}\n\
                 }}"
            );
            impl_builder.add_method(&constructor);
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
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let params_str = if params.is_empty() { String::new() } else { params };

    let body = if can_delegate && is_opaque {
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let core_call = format!("self.inner.{}({})", method.name, call_args);
        if method.error_type.is_some() {
            if matches!(method.return_type, TypeRef::Unit) {
                format!(
                    "{core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(())"
                )
            } else {
                let wrap = generators::wrap_return("result", &method.return_type, type_name, opaque_types, true);
                format!(
                    "let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
                )
            }
        } else {
            generators::wrap_return(&core_call, &method.return_type, type_name, opaque_types, true)
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    if params_str.is_empty() {
        format!(
            "pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
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
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
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
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let field_conversions = generators::gen_lossy_binding_to_core_fields(typ, core_import);
        let core_call = format!("core_self.{}({})", method.name, call_args);
        let result_wrap = match &method.return_type {
            TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => ".into()".to_string(),
            _ => String::new(),
        };
        if method.error_type.is_some() {
            format!(
                "{field_conversions}let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok(result{result_wrap})"
            )
        } else {
            format!("{field_conversions}{core_call}{result_wrap}")
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    if params_str.is_empty() {
        format!(
            "pub fn {}(&self) -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}(&self, {params_str}) -> {return_annotation} {{\n    \
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
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);
    let core_type_path = typ.rust_path.replace('-', "_");
    let call_args = generators::gen_call_args(&method.params, opaque_types);

    let body = if can_delegate {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        if method.error_type.is_some() {
            let wrap = generators::wrap_return("val", &method.return_type, &typ.name, opaque_types, typ.is_opaque);
            if wrap == "val" {
                format!("{core_call}.map_err(|e| PhpException::default(e.to_string()))")
            } else {
                format!("{core_call}.map(|val| {wrap}).map_err(|e| PhpException::default(e.to_string()))")
            }
        } else {
            generators::wrap_return(&core_call, &method.return_type, &typ.name, opaque_types, typ.is_opaque)
        }
    } else {
        gen_php_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    if params.is_empty() {
        format!(
            "pub fn {}() -> {return_annotation} {{\n    \
             {body}\n\
             }}",
            method.name
        )
    } else {
        format!(
            "pub fn {}({params}) -> {return_annotation} {{\n    \
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
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let call_args = generators::gen_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        if func.error_type.is_some() {
            let wrap = generators::wrap_return("result", &func.return_type, "", opaque_types, false);
            format!(
                "let result = {core_call}.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n    Ok({wrap})"
            )
        } else {
            generators::wrap_return(&core_call, &func.return_type, "", opaque_types, false)
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
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let call_args = generators::gen_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let result_wrap = generators::wrap_return("result", &func.return_type, "", opaque_types, false);
        if func.error_type.is_some() {
            format!(
                "WORKER_RUNTIME.block_on(async {{\n        \
                 let result = {core_call}.await.map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))?;\n        \
                 Ok({result_wrap})\n    }})"
            )
        } else {
            format!("let result = WORKER_RUNTIME.block_on(async {{ {core_call}.await }});\n    {result_wrap}")
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
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate && is_opaque {
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let inner_clone = "let inner = self.inner.clone();\n    ";
        let core_call = format!("inner.{}({})", method.name, call_args);
        let result_wrap = generators::wrap_return("result", &method.return_type, type_name, opaque_types, true);
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
fn gen_async_static_method(method: &MethodDef, mapper: &PhpMapper, _opaque_types: &AHashSet<String>) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
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
fn gen_php_unimplemented_body(return_type: &skif_core::ir::TypeRef, fn_name: &str, has_error: bool) -> String {
    use skif_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(ext_php_rs::exception::PhpException::default(\"{err_msg}\".to_string()).into())")
    } else {
        match return_type {
            TypeRef::Unit => "()".to_string(),
            TypeRef::String | TypeRef::Path => format!("String::from(\"[unimplemented: {fn_name}]\")"),
            TypeRef::Bytes => "Vec::new()".to_string(),
            TypeRef::Primitive(p) => match p {
                skif_core::ir::PrimitiveType::Bool => "false".to_string(),
                _ => "0".to_string(),
            },
            TypeRef::Optional(_) => "None".to_string(),
            TypeRef::Vec(_) => "Vec::new()".to_string(),
            TypeRef::Map(_, _) => "Default::default()".to_string(),
            TypeRef::Named(_) | TypeRef::Json => format!("panic!(\"skif: {fn_name} not auto-delegatable\")"),
            TypeRef::Duration => "std::time::Duration::default()".to_string(),
        }
    }
}

/// Generate `impl From<Type> for core::Type` with PHP-specific i64 casts.
fn gen_php_from_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", typ.name, typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = php_field_conversion(&field.name, &field.ty, field.optional, "val", true);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate a serde JSON bridge `impl From<BindingType> for core::Type`.
/// Used for enum-tainted types where field-by-field From can't work (no From<String> for core enums),
/// but serde can round-trip through JSON since the binding type derives Serialize and the core type
/// derives Deserialize.
fn gen_serde_bridge_from(typ: &TypeDef, core_import: &str) -> String {
    let core_path = skif_codegen::conversions::core_type_path(typ, core_import);
    format!(
        "impl From<{}> for {} {{\n    \
         fn from(val: {}) -> Self {{\n        \
         let json = serde_json::to_string(&val).expect(\"skif: serialize binding type\");\n        \
         serde_json::from_str(&json).expect(\"skif: deserialize to core type\")\n    \
         }}\n\
         }}",
        typ.name, core_path, typ.name
    )
}

/// Generate `impl From<core::Type> for Type` with PHP-specific i64 casts.
fn gen_php_from_core_to_binding(typ: &TypeDef, core_import: &str, enum_names: &AHashSet<String>) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", typ.name, typ.name).ok();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion =
            php_field_conversion_from_core(&field.name, &field.ty, field.optional, field.sanitized, enum_names);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// PHP-specific core→binding field conversion.
/// Enum Named fields → format!("{:?}") since PHP maps enums to String.
/// Sanitized fields → format!("{:?}").
/// Everything else delegates to the shared conversion.
fn php_field_conversion_from_core(
    name: &str,
    ty: &skif_core::ir::TypeRef,
    optional: bool,
    sanitized: bool,
    enum_names: &AHashSet<String>,
) -> String {
    use skif_core::ir::{PrimitiveType, TypeRef};
    // Sanitized fields: use format!("{:?}") to convert to String
    if sanitized {
        if optional {
            return format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))");
        }
        return format!("{name}: format!(\"{{:?}}\", val.{name})");
    }
    // Duration: PHP uses i64 (secs), core uses std::time::Duration
    if matches!(ty, TypeRef::Duration) {
        if optional {
            return format!("{name}: val.{name}.map(|d| d.as_secs() as i64)");
        }
        return format!("{name}: val.{name}.as_secs() as i64");
    }
    // PHP maps U64/Usize to i64 — need `as i64` cast for core→binding
    match ty {
        TypeRef::Primitive(PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize) => {
            if optional {
                return format!("{name}: val.{name}.map(|v| v as i64)");
            }
            return format!("{name}: val.{name} as i64");
        }
        TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Primitive(p) if matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)) =>
        {
            return format!("{name}: val.{name}.map(|v| v as i64)");
        }
        _ => {}
    }
    // Enum Named fields: PHP maps enums to String, use format!("{:?}")
    match ty {
        TypeRef::Named(n) if enum_names.contains(n.as_str()) => {
            if optional {
                format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))")
            } else {
                format!("{name}: format!(\"{{:?}}\", val.{name})")
            }
        }
        TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Named(n) if enum_names.contains(n.as_str())) => {
            if optional {
                format!("{name}: val.{name}.as_ref().map(|v| v.iter().map(|i| format!(\"{{:?}}\", i)).collect())")
            } else {
                format!("{name}: val.{name}.iter().map(|v| format!(\"{{:?}}\", v)).collect()")
            }
        }
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(n) if enum_names.contains(n.as_str()) => {
                format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))")
            }
            _ => {
                let opaque_types = AHashSet::new();
                skif_codegen::conversions::field_conversion_from_core(name, ty, optional, false, &opaque_types)
            }
        },
        _ => {
            let opaque_types = AHashSet::new();
            skif_codegen::conversions::field_conversion_from_core(name, ty, optional, false, &opaque_types)
        }
    }
}

/// PHP-specific field conversion that handles U64/Usize→i64 type casts.
fn php_field_conversion(name: &str, ty: &skif_core::ir::TypeRef, optional: bool, val: &str, to_core: bool) -> String {
    use skif_core::ir::{PrimitiveType, TypeRef};
    match ty {
        TypeRef::Primitive(p @ (PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)) => {
            let cast_to = if to_core {
                match p {
                    PrimitiveType::U64 => "u64",
                    PrimitiveType::Usize => "usize",
                    PrimitiveType::Isize => "isize",
                    _ => unreachable!(),
                }
            } else {
                "i64"
            };
            format!("{name}: {val}.{name} as {cast_to}")
        }
        // Duration: PHP uses i64 (secs), core uses std::time::Duration
        TypeRef::Duration => {
            if to_core {
                if optional {
                    format!("{name}: {val}.{name}.map(|v| std::time::Duration::from_secs(v as u64))")
                } else {
                    format!("{name}: std::time::Duration::from_secs({val}.{name} as u64)")
                }
            } else if optional {
                format!("{name}: {val}.{name}.map(|d| d.as_secs() as i64)")
            } else {
                format!("{name}: {val}.{name}.as_secs() as i64")
            }
        }
        TypeRef::Named(_) => {
            if optional {
                format!("{name}: {val}.{name}.map(Into::into)")
            } else {
                format!("{name}: {val}.{name}.into()")
            }
        }
        // Path: binding uses String (i64 for PHP), core uses PathBuf
        TypeRef::Path => {
            if to_core {
                if optional {
                    format!("{name}: {val}.{name}.map(Into::into)")
                } else {
                    format!("{name}: {val}.{name}.into()")
                }
            } else if optional {
                format!("{name}: {val}.{name}.map(|p| p.to_string_lossy().to_string())")
            } else {
                format!("{name}: {val}.{name}.to_string_lossy().to_string()")
            }
        }
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Primitive(p @ (PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)) => {
                let cast_to = if to_core {
                    match p {
                        PrimitiveType::U64 => "u64",
                        PrimitiveType::Usize => "usize",
                        PrimitiveType::Isize => "isize",
                        _ => unreachable!(),
                    }
                } else {
                    "i64"
                };
                format!("{name}: {val}.{name}.map(|v| v as {cast_to})")
            }
            TypeRef::Named(_) | TypeRef::Path => {
                format!("{name}: {val}.{name}.map(Into::into)")
            }
            TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Named(_)) => {
                format!("{name}: {val}.{name}.map(|v| v.into_iter().map(Into::into).collect())")
            }
            _ => format!("{name}: {val}.{name}"),
        },
        // Vec of named types — map each element with Into
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(_) => {
                if optional {
                    format!("{name}: {val}.{name}.map(|v| v.into_iter().map(Into::into).collect())")
                } else {
                    format!("{name}: {val}.{name}.into_iter().map(Into::into).collect()")
                }
            }
            _ => format!("{name}: {val}.{name}"),
        },
        // Map — convert Named keys/values via Into
        TypeRef::Map(k, v) => {
            let has_named_key = matches!(k.as_ref(), TypeRef::Named(_));
            let has_named_val = matches!(v.as_ref(), TypeRef::Named(_));
            if has_named_key || has_named_val {
                let key_conv = if has_named_key { "k.into()" } else { "k" };
                let val_conv = if has_named_val { "v.into()" } else { "v" };
                format!("{name}: {val}.{name}.into_iter().map(|(k, v)| ({key_conv}, {val_conv})).collect()")
            } else {
                format!("{name}: {val}.{name}")
            }
        }
        _ => format!("{name}: {val}.{name}"),
    }
}

/// Return true if any field of the type (recursively through Optional/Vec) is a Named type
/// that is an enum. PHP maps enum Named types to String, so From/Into impls would need
/// From<String> for the core enum which doesn't exist — skip generation for such types.
/// Check if a TypeRef references any type in the given set (transitively through containers).
fn references_named_type(ty: &skif_core::ir::TypeRef, names: &AHashSet<String>) -> bool {
    use skif_core::ir::TypeRef;
    match ty {
        TypeRef::Named(name) => names.contains(name.as_str()),
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => references_named_type(inner, names),
        TypeRef::Map(k, v) => references_named_type(k, names) || references_named_type(v, names),
        _ => false,
    }
}

fn has_enum_named_field(typ: &skif_core::ir::TypeDef, enum_names: &AHashSet<String>) -> bool {
    fn type_ref_has_enum_named(ty: &skif_core::ir::TypeRef, enum_names: &AHashSet<String>) -> bool {
        use skif_core::ir::TypeRef;
        match ty {
            TypeRef::Named(name) => enum_names.contains(name.as_str()),
            TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_enum_named(inner, enum_names),
            TypeRef::Map(k, v) => type_ref_has_enum_named(k, enum_names) || type_ref_has_enum_named(v, enum_names),
            _ => false,
        }
    }
    typ.fields.iter().any(|f| type_ref_has_enum_named(&f.ty, enum_names))
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
