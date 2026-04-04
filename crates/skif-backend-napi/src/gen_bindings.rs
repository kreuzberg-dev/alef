use crate::type_map::NapiMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_codegen::naming::to_node_name;
use skif_codegen::shared::{can_auto_delegate, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

pub struct NapiBackend;

impl NapiBackend {
    fn binding_config(core_import: &str) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &["napi"],
            field_attrs: &[],
            struct_derives: &["Clone"],
            method_block_attr: Some("napi"),
            constructor_attr: "#[napi(constructor)]",
            static_attr: None,
            function_attr: "#[napi]",
            enum_attrs: &["napi(string_enum)"],
            enum_derives: &["Clone"],
            needs_signature: false,
            signature_prefix: "",
            signature_suffix: "",
            core_import,
            async_pattern: AsyncPattern::NapiNativeAsync,
            // NAPI napi(object) structs don't derive Serialize — disable serde bridge
            has_serde: false,
            type_name_prefix: "Js",
        }
    }
}

impl Backend for NapiBackend {
    fn name(&self) -> &str {
        "napi"
    }

    fn language(&self) -> Language {
        Language::Node
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
        let mapper = NapiMapper;
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("napi::*");
        builder.add_import("napi_derive::napi");
        builder.add_import("serde_json");

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

        // Custom module declarations (NAPI auto-exports, no explicit registration needed)
        let custom_mods = config.custom_modules.for_language(Language::Node);
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

        // NAPI has some unique patterns: Js-prefixed names, Option-wrapped fields,
        // and custom constructor. Use shared generators for enums and functions,
        // but keep struct/method generation custom.
        for typ in &api.types {
            if typ.is_opaque {
                builder.add_item(&skif_codegen::generators::gen_opaque_struct_prefixed(typ, &cfg, "Js"));
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &cfg, &opaque_types));
            } else {
                // Non-opaque structs use #[napi(object)] — plain JS objects without methods.
                // napi(object) structs cannot have #[napi] impl blocks.
                builder.add_item(&gen_struct(typ, &mapper));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            // Skip functions with opaque type params — NAPI opaque structs don't implement FromNapiValue.
            // These functions are todo!() stubs and need manual wiring via class methods instead.
            let has_opaque_param = func.params.iter().any(|p| {
                if let skif_core::ir::TypeRef::Named(n) = &p.ty {
                    opaque_types.contains(n)
                } else {
                    false
                }
            });
            if !has_opaque_param {
                builder.add_item(&gen_function(func, &mapper, &cfg, &opaque_types));
            }
        }

        let convertible = skif_codegen::conversions::convertible_types(api);
        // From/Into conversions (NAPI uses Js prefix, so we need custom generation)
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible) {
                builder.add_item(&gen_from_js_binding_to_core(typ, &core_import));
                builder.add_item(&gen_from_core_to_js_binding(typ, &core_import));
            }
        }
        for e in &api.enums {
            if skif_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&gen_enum_from_js_binding_to_core(e, &core_import));
                builder.add_item(&gen_enum_from_core_to_js_binding(e, &core_import));
            }
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Node)?;

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.node.as_ref(),
            &config.crate_config.name,
            "crates/{name}-node/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }
}

/// Generate a NAPI struct with Js-prefixed name and fields wrapped in Option only if optional.
fn gen_struct(typ: &TypeDef, mapper: &NapiMapper) -> String {
    let mut struct_builder = StructBuilder::new(&format!("Js{}", typ.name));
    // Use napi(object) so the struct can be used as function/method parameters (FromNapiValue)
    struct_builder.add_attr("napi(object)");
    struct_builder.add_derive("Clone");

    for field in &typ.fields {
        let mapped_type = mapper.map_type(&field.ty);
        let field_type = if field.optional {
            format!("Option<{}>", mapped_type)
        } else {
            mapped_type
        };
        let js_name = to_node_name(&field.name);
        let attrs = if js_name != field.name {
            vec![format!("napi(js_name = \"{}\")", js_name)]
        } else {
            vec![]
        };
        struct_builder.add_field(&field.name, &field_type, attrs);
    }

    struct_builder.build()
}

/// Generate NAPI methods for an opaque struct (delegates to self.inner).
fn gen_opaque_struct_methods(
    typ: &TypeDef,
    mapper: &NapiMapper,
    cfg: &RustBindingConfig,
    opaque_types: &AHashSet<String>,
) -> String {
    let mut impl_builder = ImplBuilder::new(&format!("Js{}", typ.name));
    impl_builder.add_attr("napi");

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        impl_builder.add_method(&gen_opaque_instance_method(method, mapper, typ, cfg, opaque_types));
    }
    for method in &statics {
        impl_builder.add_method(&gen_static_method(method, mapper, typ, cfg, opaque_types));
    }

    impl_builder.build()
}

/// Generate an opaque instance method that delegates to self.inner.
fn gen_opaque_instance_method(
    method: &MethodDef,
    mapper: &NapiMapper,
    typ: &TypeDef,
    cfg: &RustBindingConfig,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let async_kw = if method.is_async { "async " } else { "" };

    let type_name = &typ.name;
    let is_owned_receiver = matches!(method.receiver.as_ref(), Some(skif_core::ir::ReceiverKind::Owned));
    let call_args = generators::gen_call_args(&method.params, opaque_types);

    // Use the shared can_auto_delegate check for opaque instance methods.
    let opaque_can_delegate = !method.sanitized
        && (!is_owned_receiver || typ.is_clone)
        && method
            .params
            .iter()
            .all(|p| !p.sanitized && skif_codegen::shared::is_delegatable_param(&p.ty, opaque_types))
        && skif_codegen::shared::is_opaque_delegatable_type(&method.return_type);

    let make_core_call = |method_name: &str| -> String {
        if is_owned_receiver {
            format!("(*self.inner).clone().{method_name}({call_args})")
        } else {
            format!("self.inner.{method_name}({call_args})")
        }
    };

    let make_async_core_call = |method_name: &str| -> String { format!("inner.{method_name}({call_args})") };

    let async_result_wrap = napi_wrap_return("result", &method.return_type, type_name, opaque_types, true);

    let body = if !opaque_can_delegate {
        // Try serde-based param conversion for methods with non-opaque Named params
        if cfg.has_serde
            && !method.sanitized
            && generators::has_named_params(&method.params, opaque_types)
            && method.error_type.is_some()
            && skif_codegen::shared::is_opaque_delegatable_type(&method.return_type)
        {
            let err_conv = ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))";
            let serde_bindings =
                generators::gen_serde_let_bindings(&method.params, opaque_types, cfg.core_import, err_conv, "        ");
            let serde_call_args = generators::gen_call_args_with_let_bindings(&method.params, opaque_types);
            let core_call = format!("self.inner.{}({serde_call_args})", method.name);
            if matches!(method.return_type, TypeRef::Unit) {
                format!("{serde_bindings}{core_call}{err_conv}?;\n    Ok(())")
            } else {
                let wrap = napi_wrap_return("result", &method.return_type, type_name, opaque_types, true);
                format!("{serde_bindings}let result = {core_call}{err_conv}?;\n    Ok({wrap})")
            }
        } else {
            generators::gen_unimplemented_body(
                &method.return_type,
                &format!("{type_name}.{}", method.name),
                method.error_type.is_some(),
                cfg,
            )
        }
    } else if method.is_async {
        let inner_clone_line = "let inner = self.inner.clone();\n    ";
        let core_call_str = make_async_core_call(&method.name);
        generators::gen_async_body(
            &core_call_str,
            cfg,
            method.error_type.is_some(),
            &async_result_wrap,
            true,
            inner_clone_line,
        )
    } else {
        let core_call = make_core_call(&method.name);
        if method.error_type.is_some() {
            let err_conv = ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))";
            if matches!(method.return_type, TypeRef::Unit) {
                format!("{core_call}{err_conv}?;\n    Ok(())")
            } else {
                let wrap = napi_wrap_return("result", &method.return_type, type_name, opaque_types, true);
                format!("let result = {core_call}{err_conv}?;\n    Ok({wrap})")
            }
        } else {
            napi_wrap_return(&core_call, &method.return_type, type_name, opaque_types, true)
        }
    };

    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}(&self, {params}) -> {return_annotation} {{\n    \
         {body}\n}}",
        method.name
    )
}

/// Generate a static method binding.
fn gen_static_method(
    method: &MethodDef,
    mapper: &NapiMapper,
    typ: &TypeDef,
    cfg: &RustBindingConfig,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let type_name = &typ.name;
    let core_type_path = typ.rust_path.replace('-', "_");
    let call_args = generators::gen_call_args(&method.params, opaque_types);
    let can_delegate_static = can_auto_delegate(method, opaque_types);

    let async_kw = if method.is_async { "async " } else { "" };

    let body = if !can_delegate_static {
        generators::gen_unimplemented_body(
            &method.return_type,
            &format!("{type_name}::{}", method.name),
            method.error_type.is_some(),
            cfg,
        )
    } else if method.is_async {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        let return_wrap = napi_wrap_return("result", &method.return_type, type_name, opaque_types, typ.is_opaque);
        generators::gen_async_body(&core_call, cfg, method.error_type.is_some(), &return_wrap, false, "")
    } else {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        if method.error_type.is_some() {
            let err_conv = ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))";
            let wrapped = napi_wrap_return("val", &method.return_type, type_name, opaque_types, typ.is_opaque);
            if wrapped == "val" {
                format!("{core_call}{err_conv}")
            } else {
                format!("{core_call}.map(|val| {wrapped}){err_conv}")
            }
        } else {
            napi_wrap_return(&core_call, &method.return_type, type_name, opaque_types, typ.is_opaque)
        }
    };

    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         {body}\n}}",
        method.name
    )
}

/// Generate a NAPI enum definition using string_enum with Js prefix.
fn gen_enum(enum_def: &EnumDef) -> String {
    let mut lines = vec![
        "#[napi(string_enum)]".to_string(),
        "#[derive(Clone)]".to_string(),
        format!("pub enum Js{} {{", enum_def.name),
    ];

    for variant in &enum_def.variants {
        lines.push(format!("    {},", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate a free function binding.
fn gen_function(
    func: &FunctionDef,
    mapper: &NapiMapper,
    cfg: &RustBindingConfig,
    opaque_types: &AHashSet<String>,
) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let js_name = to_node_name(&func.name);
    let js_name_attr = if js_name != func.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let core_import = cfg.core_import;
    let core_fn_path = {
        let path = func.rust_path.replace('-', "_");
        if path.starts_with(core_import) {
            path
        } else {
            format!("{core_import}::{}", func.name)
        }
    };

    // Use let-binding pattern for non-opaque Named params
    let use_let_bindings = generators::has_named_params(&func.params, opaque_types);
    let call_args = if use_let_bindings {
        generators::gen_call_args_with_let_bindings(&func.params, opaque_types)
    } else {
        generators::gen_call_args(&func.params, opaque_types)
    };

    let can_delegate_fn = skif_codegen::shared::can_auto_delegate_function(func, opaque_types);

    let err_conv = ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))";

    let async_kw = if func.is_async { "async " } else { "" };

    let body = if !can_delegate_fn {
        // Try serde-based conversion for non-delegatable functions with Named params
        if cfg.has_serde && use_let_bindings && func.error_type.is_some() {
            let serde_bindings =
                generators::gen_serde_let_bindings(&func.params, opaque_types, core_import, err_conv, "    ");
            let core_call = format!("{core_fn_path}({call_args})");

            if matches!(func.return_type, TypeRef::Unit) {
                format!("{serde_bindings}{core_call}{err_conv}?;\n    Ok(())")
            } else {
                let wrapped = napi_wrap_return_fn("val", &func.return_type, opaque_types);
                if wrapped == "val" {
                    format!("{serde_bindings}{core_call}{err_conv}")
                } else {
                    format!("{serde_bindings}{core_call}.map(|val| {wrapped}){err_conv}")
                }
            }
        } else {
            generators::gen_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some(), cfg)
        }
    } else if func.is_async {
        let core_call = format!("{core_fn_path}({call_args})");
        let return_wrap = napi_wrap_return_fn("result", &func.return_type, opaque_types);
        generators::gen_async_body(&core_call, cfg, func.error_type.is_some(), &return_wrap, false, "")
    } else {
        let core_call = format!("{core_fn_path}({call_args})");

        // When can_delegate_fn is true, params are simple enough that gen_call_args
        // handles conversion directly (no extra let bindings needed).
        if func.error_type.is_some() {
            let wrapped = napi_wrap_return_fn("val", &func.return_type, opaque_types);
            if wrapped == "val" {
                format!("{core_call}{err_conv}")
            } else {
                format!("{core_call}.map(|val| {wrapped}){err_conv}")
            }
        } else {
            napi_wrap_return_fn(&core_call, &func.return_type, opaque_types)
        }
    };

    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         {body}\n}}",
        func.name
    )
}

/// NAPI-specific return wrapping for opaque instance methods.
/// Extends the shared `wrap_return` with i64 casts for u64/usize/isize primitives.
fn napi_wrap_return(
    expr: &str,
    return_type: &TypeRef,
    type_name: &str,
    opaque_types: &AHashSet<String>,
    self_is_opaque: bool,
) -> String {
    match return_type {
        TypeRef::Primitive(p) if needs_napi_cast(p) => {
            format!("{expr} as i64")
        }
        TypeRef::Duration => format!("{expr}.as_secs() as i64"),
        // Opaque Named returns need Js prefix
        TypeRef::Named(n) if n == type_name && self_is_opaque => {
            format!("Self {{ inner: Arc::new({expr}) }}")
        }
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            format!("Js{n} {{ inner: Arc::new({expr}) }}")
        }
        TypeRef::Named(_) => format!("{expr}.into()"),
        _ => generators::wrap_return(expr, return_type, type_name, opaque_types, self_is_opaque),
    }
}

/// NAPI-specific return wrapping for free functions (no type_name context).
fn napi_wrap_return_fn(expr: &str, return_type: &TypeRef, opaque_types: &AHashSet<String>) -> String {
    match return_type {
        TypeRef::Primitive(p) if needs_napi_cast(p) => {
            format!("{expr} as i64")
        }
        TypeRef::Duration => format!("{expr}.as_secs() as i64"),
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            format!("Js{n} {{ inner: Arc::new({expr}) }}")
        }
        TypeRef::Named(_) => format!("{expr}.into()"),
        TypeRef::String | TypeRef::Bytes => format!("{expr}.into()"),
        TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                format!("{expr}.map(|v| Js{name} {{ inner: Arc::new(v) }})")
            }
            TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => {
                format!("{expr}.map(Into::into)")
            }
            _ => expr.to_string(),
        },
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                format!("{expr}.into_iter().map(|v| Js{name} {{ inner: Arc::new(v) }}).collect()")
            }
            TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => {
                format!("{expr}.into_iter().map(Into::into).collect()")
            }
            _ => expr.to_string(),
        },
        _ => expr.to_string(),
    }
}

/// Generate `impl From<JsType> for core::Type` (NAPI binding -> core).
fn gen_from_js_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", typ.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = napi_field_conversion(&field.name, &field.ty, field.optional, "val", true);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Type> for JsType` (core -> NAPI binding).
fn gen_from_core_to_js_binding(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", typ.name);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", typ.name, js_name).ok();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = napi_field_conversion(&field.name, &field.ty, field.optional, "val", false);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// NAPI-specific field conversion that handles U64/Usize→i64 type casts.
/// `to_core=true`: NAPI binding → core (i64 → u64/usize via `as`)
/// `to_core=false`: core → NAPI binding (u64/usize → i64 via `as`)
fn napi_field_conversion(name: &str, ty: &skif_core::ir::TypeRef, optional: bool, val: &str, to_core: bool) -> String {
    use skif_core::ir::TypeRef;
    match ty {
        TypeRef::Primitive(p) if needs_napi_cast(p) => {
            let cast_to = if to_core { core_prim_str(p) } else { "i64" };
            format!("{name}: {val}.{name} as {cast_to}")
        }
        // Duration: NAPI uses i64 (secs), core uses std::time::Duration
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
        // Path: binding uses String, core uses PathBuf
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
            TypeRef::Primitive(p) if needs_napi_cast(p) => {
                let cast_to = if to_core { core_prim_str(p) } else { "i64" };
                format!("{name}: {val}.{name}.map(|v| v as {cast_to})")
            }
            TypeRef::Named(_) => {
                format!("{name}: {val}.{name}.map(Into::into)")
            }
            TypeRef::Path => {
                if to_core {
                    format!("{name}: {val}.{name}.map(Into::into)")
                } else {
                    format!("{name}: {val}.{name}.map(|p| p.to_string_lossy().to_string())")
                }
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
                let k_expr = if has_named_key { "k.into()" } else { "k" };
                let v_expr = if has_named_val { "v.into()" } else { "v" };
                format!("{name}: {val}.{name}.into_iter().map(|(k, v)| ({k_expr}, {v_expr})).collect()")
            } else {
                format!("{name}: {val}.{name}")
            }
        }
        _ => format!("{name}: {val}.{name}"),
    }
}

fn needs_napi_cast(p: &skif_core::ir::PrimitiveType) -> bool {
    matches!(
        p,
        skif_core::ir::PrimitiveType::U64 | skif_core::ir::PrimitiveType::Usize | skif_core::ir::PrimitiveType::Isize
    )
}

fn core_prim_str(p: &skif_core::ir::PrimitiveType) -> &'static str {
    match p {
        skif_core::ir::PrimitiveType::U64 => "u64",
        skif_core::ir::PrimitiveType::Usize => "usize",
        skif_core::ir::PrimitiveType::Isize => "isize",
        _ => unreachable!(),
    }
}

/// Generate `impl From<JsEnum> for core::Enum` (NAPI binding -> core).
/// Binding enums are always unit-variant-only. Core enums may have data variants,
/// in which case Default::default() is used for fields.
fn gen_enum_from_js_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, enum_def.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        let arm = skif_codegen::conversions::binding_to_core_match_arm(&js_name, &variant.name, &variant.fields);
        writeln!(out, "            {arm}").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Enum> for JsEnum` (core -> NAPI binding).
/// Core enums may have data variants; binding enums are always unit-variant-only,
/// so data fields are discarded.
fn gen_enum_from_core_to_js_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    let core_prefix = format!("{core_import}::{}", enum_def.name);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", enum_def.name, js_name).ok();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", enum_def.name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        let arm = skif_codegen::conversions::core_to_binding_match_arm(&core_prefix, &variant.name, &variant.fields);
        writeln!(out, "            {arm}").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate a global Tokio runtime for NAPI async support.
fn gen_tokio_runtime() -> String {
    "static WORKER_POOL: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect(\"Failed to create Tokio runtime\")
});"
    .to_string()
}
