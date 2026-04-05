use crate::type_map::WasmMapper;
use ahash::AHashSet;
use eisberg_codegen::builder::{ImplBuilder, RustFileBuilder};
use eisberg_codegen::generators::{self};
use eisberg_codegen::naming::to_node_name;
use eisberg_codegen::shared::{self, constructor_parts};
use eisberg_codegen::type_mapper::TypeMapper;
use eisberg_core::backend::{Backend, Capabilities, GeneratedFile};
use eisberg_core::config::{Language, SkifConfig, resolve_output_dir};
use eisberg_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

/// Check if a TypeRef is a Copy type that shouldn't be cloned.
fn is_copy_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_) => true, // All primitives are Copy
        TypeRef::Duration => true,     // Duration maps to u64 (secs), which is Copy
        TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Json => false,
        TypeRef::Optional(_) | TypeRef::Vec(_) | TypeRef::Map(_, _) => false,
        TypeRef::Named(_) => false, // Custom types are not Copy
        TypeRef::Unit => true,
    }
}

pub struct WasmBackend;

impl Backend for WasmBackend {
    fn name(&self) -> &str {
        "wasm"
    }

    fn language(&self) -> Language {
        Language::Wasm
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
        let wasm_config = config.wasm.as_ref();
        let exclude_functions = wasm_config.map(|c| c.exclude_functions.clone()).unwrap_or_default();
        let exclude_types = wasm_config.map(|c| c.exclude_types.clone()).unwrap_or_default();
        let type_overrides = wasm_config.map(|c| c.type_overrides.clone()).unwrap_or_default();

        let mapper = WasmMapper::new(type_overrides);
        let core_import = config.core_import();

        // Note: custom modules and registrations handled below after builder creation

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("wasm_bindgen::prelude::*");

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
        let custom_mods = config.custom_modules.for_language(Language::Wasm);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

        // Check if we have opaque types and add Arc import if needed
        let opaque_types: AHashSet<String> = api
            .types
            .iter()
            .filter(|t| t.is_opaque && !exclude_types.contains(&t.name))
            .map(|t| t.name.clone())
            .collect();
        if !opaque_types.is_empty() {
            builder.add_import("std::sync::Arc");
        }

        for typ in &api.types {
            if exclude_types.contains(&typ.name) {
                continue;
            }
            if typ.is_opaque {
                builder.add_item(&gen_opaque_struct(typ, &core_import));
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &opaque_types, &core_import));
            } else {
                builder.add_item(&gen_struct(typ, &mapper));
                builder.add_item(&gen_struct_methods(
                    typ,
                    &mapper,
                    &exclude_types,
                    &core_import,
                    &opaque_types,
                ));
            }
        }

        for enum_def in &api.enums {
            if !exclude_types.contains(&enum_def.name) {
                builder.add_item(&gen_enum(enum_def));
            }
        }

        for func in &api.functions {
            if !exclude_functions.contains(&func.name) {
                builder.add_item(&gen_function(func, &mapper, &core_import, &opaque_types));
            }
        }

        let wasm_conv_config = eisberg_codegen::conversions::ConversionConfig {
            type_name_prefix: "Js",
            map_uses_jsvalue: true,
            ..Default::default()
        };
        let convertible = eisberg_codegen::conversions::convertible_types(api);
        let core_to_binding_convertible = eisberg_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions using shared parameterized generators
        for typ in &api.types {
            if exclude_types.contains(&typ.name) {
                continue;
            }
            let is_strict = eisberg_codegen::conversions::can_generate_conversion(typ, &convertible);
            let is_relaxed = eisberg_codegen::conversions::can_generate_conversion(typ, &core_to_binding_convertible);
            if is_strict {
                // Both directions
                builder.add_item(&eisberg_codegen::conversions::gen_from_binding_to_core_cfg(
                    typ,
                    &core_import,
                    &wasm_conv_config,
                ));
                builder.add_item(&eisberg_codegen::conversions::gen_from_core_to_binding_cfg(
                    typ,
                    &core_import,
                    &opaque_types,
                    &wasm_conv_config,
                ));
            } else if is_relaxed {
                // Only core→binding (sanitized fields prevent binding→core)
                builder.add_item(&eisberg_codegen::conversions::gen_from_core_to_binding_cfg(
                    typ,
                    &core_import,
                    &opaque_types,
                    &wasm_conv_config,
                ));
            }
        }
        for e in &api.enums {
            if !exclude_types.contains(&e.name) {
                if eisberg_codegen::conversions::can_generate_enum_conversion(e) {
                    builder.add_item(&eisberg_codegen::conversions::gen_enum_from_binding_to_core_cfg(
                        e,
                        &core_import,
                        &wasm_conv_config,
                    ));
                }
                if eisberg_codegen::conversions::can_generate_enum_conversion_from_core(e) {
                    builder.add_item(&eisberg_codegen::conversions::gen_enum_from_core_to_binding_cfg(
                        e,
                        &core_import,
                        &wasm_conv_config,
                    ));
                }
            }
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = eisberg_adapters::build_adapter_bodies(config, Language::Wasm)?;

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.wasm.as_ref(),
            &config.crate_config.name,
            "crates/{name}-wasm/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }
}

/// Generate an opaque wasm-bindgen struct with inner Arc.
fn gen_opaque_struct(typ: &TypeDef, core_import: &str) -> String {
    let js_name = format!("Js{}", typ.name);

    // We can't use StructBuilder for private fields, so build manually
    let mut out = String::with_capacity(256);
    writeln!(out, "#[derive(Clone)]").ok();
    writeln!(out, "#[wasm_bindgen]").ok();
    writeln!(out, "pub struct {} {{", js_name).ok();
    let core_path = eisberg_codegen::conversions::core_type_path(typ, core_import);
    writeln!(out, "    inner: Arc<{}>,", core_path).ok();
    write!(out, "}}").ok();
    out
}

/// Generate wasm-bindgen methods for an opaque struct.
fn gen_opaque_struct_methods(
    typ: &TypeDef,
    mapper: &WasmMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut impl_builder = ImplBuilder::new(&js_name);
    impl_builder.add_attr("wasm_bindgen");

    for method in &typ.methods {
        if method.is_static {
            impl_builder.add_method(&gen_opaque_static_method(
                method,
                mapper,
                &typ.name,
                opaque_types,
                core_import,
            ));
        } else {
            impl_builder.add_method(&gen_opaque_method(method, mapper, &typ.name, opaque_types));
        }
    }

    impl_builder.build()
}

/// Generate a method for an opaque wasm-bindgen struct that delegates to self.inner.
fn gen_opaque_method(
    method: &MethodDef,
    mapper: &WasmMapper,
    type_name: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let ty = mapper.map_type(&p.ty);
            if p.optional {
                format!("{}: Option<{}>", p.name, ty)
            } else {
                format!("{}: {}", p.name, ty)
            }
        })
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let async_kw = if method.is_async { "async " } else { "" };

    // Check if the core method takes ownership (Owned receiver).
    // If so, we must clone out of Arc since wasm_bindgen methods take &self.
    let needs_clone = matches!(method.receiver, Some(eisberg_core::ir::ReceiverKind::Owned));

    let body = if can_delegate {
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let core_call = if needs_clone {
            format!("(*self.inner).clone().{}({})", method.name, call_args)
        } else {
            format!("self.inner.{}({})", method.name, call_args)
        };
        if method.is_async {
            // WASM async: native async fn becomes a Promise automatically
            let result_wrap = wasm_wrap_return(
                "result",
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            );
            if method.error_type.is_some() {
                format!(
                    "let result = {core_call}.await\n        \
                     .map_err(|e| JsValue::from_str(&e.to_string()))?;\n    \
                     Ok({result_wrap})"
                )
            } else {
                format!("let result = {core_call}.await;\n    Ok({result_wrap})")
            }
        } else if method.error_type.is_some() {
            if matches!(method.return_type, TypeRef::Unit) {
                format!("{core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok(())")
            } else {
                let wrap = wasm_wrap_return(
                    "result",
                    &method.return_type,
                    type_name,
                    opaque_types,
                    true,
                    method.returns_ref,
                );
                format!("let result = {core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok({wrap})")
            }
        } else {
            wasm_wrap_return(
                &core_call,
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            )
        }
    } else {
        gen_wasm_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let mut attrs = String::new();
    // Per-item clippy suppression: too_many_arguments when >7 params (including &self)
    if method.params.len() + 1 > 7 {
        attrs.push_str("#[allow(clippy::too_many_arguments)]\n");
    }
    // Per-item clippy suppression: missing_errors_doc for Result-returning methods
    if method.error_type.is_some() {
        attrs.push_str("#[allow(clippy::missing_errors_doc)]\n");
    }
    // Per-item clippy suppression: should_implement_trait for trait-conflicting names
    if generators::is_trait_method_name(&method.name) {
        attrs.push_str("#[allow(clippy::should_implement_trait)]\n");
    }
    format!(
        "{attrs}#[wasm_bindgen{js_name_attr}]\npub {async_kw}fn {}(&self, {}) -> {} {{\n    \
         {body}\n}}",
        method.name,
        params.join(", "),
        return_annotation
    )
}

/// Generate a static method for an opaque wasm-bindgen struct.
/// Static methods call CoreType::method() instead of self.inner.method().
fn gen_opaque_static_method(
    method: &MethodDef,
    mapper: &WasmMapper,
    type_name: &str,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let ty = mapper.map_type(&p.ty);
            if p.optional {
                format!("{}: Option<{}>", p.name, ty)
            } else {
                format!("{}: {}", p.name, ty)
            }
        })
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate {
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let core_call = format!("{core_import}::{type_name}::{}({call_args})", method.name);
        if method.error_type.is_some() {
            let wrap = wasm_wrap_return(
                "result",
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            );
            format!("let result = {core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok({wrap})")
        } else {
            wasm_wrap_return(
                &core_call,
                &method.return_type,
                type_name,
                opaque_types,
                true,
                method.returns_ref,
            )
        }
    } else {
        gen_wasm_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
    };

    let mut attrs = String::new();
    // Per-item clippy suppression: too_many_arguments when >7 params
    if method.params.len() > 7 {
        attrs.push_str("#[allow(clippy::too_many_arguments)]\n");
    }
    // Per-item clippy suppression: missing_errors_doc for Result-returning methods
    if method.error_type.is_some() {
        attrs.push_str("#[allow(clippy::missing_errors_doc)]\n");
    }
    // Per-item clippy suppression: should_implement_trait for trait-conflicting names
    if generators::is_trait_method_name(&method.name) {
        attrs.push_str("#[allow(clippy::should_implement_trait)]\n");
    }
    format!(
        "{attrs}#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
         {body}\n}}",
        method.name,
        params.join(", "),
        return_annotation
    )
}

/// Generate a wasm-bindgen struct definition with private fields.
fn gen_struct(typ: &TypeDef, mapper: &WasmMapper) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut out = String::with_capacity(512);
    writeln!(out, "#[derive(Clone)]").ok();
    writeln!(out, "#[wasm_bindgen]").ok();
    writeln!(out, "pub struct {} {{", js_name).ok();

    for field in &typ.fields {
        let field_type = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        // Fields are private (no pub)
        writeln!(out, "    {}: {},", field.name, field_type).ok();
    }

    writeln!(out, "}}").ok();
    out
}

/// Generate wasm-bindgen methods for a struct.
fn gen_struct_methods(
    typ: &TypeDef,
    mapper: &WasmMapper,
    exclude_types: &[String],
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut impl_builder = ImplBuilder::new(&js_name);
    impl_builder.add_attr("wasm_bindgen");

    if !typ.fields.is_empty() {
        impl_builder.add_method(&gen_new_method(typ, mapper));
    }

    for field in &typ.fields {
        impl_builder.add_method(&gen_getter(field, mapper));
        impl_builder.add_method(&gen_setter(field, mapper));
    }

    if !exclude_types.contains(&typ.name) {
        for method in &typ.methods {
            impl_builder.add_method(&gen_method(method, mapper, &typ.name, core_import, opaque_types));
        }
    }

    impl_builder.build()
}

/// Generate a constructor method.
fn gen_new_method(typ: &TypeDef, mapper: &WasmMapper) -> String {
    let map_fn = |ty: &eisberg_core::ir::TypeRef| mapper.map_type(ty);
    let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);

    format!(
        "#[wasm_bindgen(constructor)]\npub fn new({param_list}) -> Js{} {{\n    Js{} {{ {assignments} }}\n}}",
        typ.name, typ.name
    )
}

/// Generate a getter method for a field.
fn gen_getter(field: &FieldDef, mapper: &WasmMapper) -> String {
    let field_type = if field.optional {
        mapper.optional(&mapper.map_type(&field.ty))
    } else {
        mapper.map_type(&field.ty)
    };

    let js_name = to_node_name(&field.name);
    let js_name_attr = if js_name != field.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    // Only clone non-Copy types; Copy types are returned directly
    let return_expr = if is_copy_type(&field.ty) {
        format!("self.{}", field.name)
    } else {
        format!("self.{}.clone()", field.name)
    };

    format!(
        "#[wasm_bindgen(getter{js_name_attr})]\npub fn {}(&self) -> {} {{\n    {}\n}}",
        field.name, field_type, return_expr
    )
}

/// Generate a setter method for a field.
fn gen_setter(field: &FieldDef, mapper: &WasmMapper) -> String {
    let field_type = if field.optional {
        mapper.optional(&mapper.map_type(&field.ty))
    } else {
        mapper.map_type(&field.ty)
    };

    let js_name = to_node_name(&field.name);
    let js_name_attr = if js_name != field.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    format!(
        "#[wasm_bindgen(setter{js_name_attr})]\npub fn set_{}(&mut self, value: {}) {{\n    self.{} = value;\n}}",
        field.name, field_type, field.name
    )
}

/// Generate a method binding for a struct method.
fn gen_method(
    method: &MethodDef,
    mapper: &WasmMapper,
    type_name: &str,
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let ty = mapper.map_type(&p.ty);
            if p.optional {
                format!("{}: Option<{}>", p.name, ty)
            } else {
                format!("{}: {}", p.name, ty)
            }
        })
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let mut attrs = String::new();
    // Per-item clippy suppression: too_many_arguments when >7 params (including &self for instance methods)
    let effective_param_count = if method.is_static {
        method.params.len()
    } else {
        method.params.len() + 1
    };
    if effective_param_count > 7 {
        attrs.push_str("#[allow(clippy::too_many_arguments)]\n");
    }
    // Per-item clippy suppression: missing_errors_doc for Result-returning methods
    if method.error_type.is_some() {
        attrs.push_str("#[allow(clippy::missing_errors_doc)]\n");
    }
    // Per-item clippy suppression: should_implement_trait for trait-conflicting names
    if generators::is_trait_method_name(&method.name) {
        attrs.push_str("#[allow(clippy::should_implement_trait)]\n");
    }

    if method.is_async {
        let call_args = generators::gen_call_args(&method.params, opaque_types);
        let core_call = format!(
            "{core_import}::{type_name}::from(self.clone()).{method_name}({call_args})",
            method_name = method.name
        );
        let body = if method.error_type.is_some() {
            format!(
                "let result = {core_call}.await\n        \
                 .map_err(|e| JsValue::from_str(&e.to_string()))?;\n    \
                 Ok({}::from(result))",
                return_type
            )
        } else {
            format!(
                "let result = {core_call}.await;\n    \
                 Ok({}::from(result))",
                return_type
            )
        };
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub async fn {}(&self, {}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else if method.is_static {
        let body = if can_delegate {
            let call_args = generators::gen_call_args(&method.params, opaque_types);
            let core_call = format!("{core_import}::{type_name}::{}({call_args})", method.name);
            if method.error_type.is_some() {
                let wrap = wasm_wrap_return(
                    "result",
                    &method.return_type,
                    type_name,
                    opaque_types,
                    false,
                    method.returns_ref,
                );
                format!("let result = {core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok({wrap})")
            } else {
                wasm_wrap_return(
                    &core_call,
                    &method.return_type,
                    type_name,
                    opaque_types,
                    false,
                    method.returns_ref,
                )
            }
        } else {
            gen_wasm_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
        };
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else {
        let body = if can_delegate {
            let call_args = generators::gen_call_args(&method.params, opaque_types);
            let core_call = format!(
                "{core_import}::{type_name}::from(self.clone()).{method_name}({call_args})",
                method_name = method.name
            );
            if method.error_type.is_some() {
                let wrap = wasm_wrap_return(
                    "result",
                    &method.return_type,
                    type_name,
                    opaque_types,
                    false,
                    method.returns_ref,
                );
                format!("let result = {core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok({wrap})")
            } else {
                wasm_wrap_return(
                    &core_call,
                    &method.return_type,
                    type_name,
                    opaque_types,
                    false,
                    method.returns_ref,
                )
            }
        } else {
            gen_wasm_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some())
        };
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub fn {}(&self, {}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    }
}

/// Generate a wasm-bindgen enum definition.
fn gen_enum(enum_def: &EnumDef) -> String {
    let js_name = format!("Js{}", enum_def.name);
    let mut lines = vec![
        "#[wasm_bindgen]".to_string(),
        "#[derive(Clone, Copy, PartialEq, Eq)]".to_string(),
        format!("pub enum {} {{", js_name),
    ];

    for (idx, variant) in enum_def.variants.iter().enumerate() {
        lines.push(format!("    {} = {},", variant.name, idx));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate a free function binding.
fn gen_function(func: &FunctionDef, mapper: &WasmMapper, core_import: &str, opaque_types: &AHashSet<String>) -> String {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let ty = mapper.map_type(&p.ty);
            if p.optional {
                format!("{}: Option<{}>", p.name, ty)
            } else {
                format!("{}: {}", p.name, ty)
            }
        })
        .collect();

    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let js_name = to_node_name(&func.name);
    let js_name_attr = if js_name != func.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let mut attrs = String::new();
    // Per-item clippy suppression: too_many_arguments when >7 params
    if func.params.len() > 7 {
        attrs.push_str("#[allow(clippy::too_many_arguments)]\n");
    }
    // Per-item clippy suppression: missing_errors_doc for Result-returning functions
    if func.error_type.is_some() {
        attrs.push_str("#[allow(clippy::missing_errors_doc)]\n");
    }

    if func.is_async {
        let call_args = generators::gen_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let body = if func.error_type.is_some() {
            format!(
                "let result = {core_call}.await\n        \
                 .map_err(|e| JsValue::from_str(&e.to_string()))?;\n    \
                 Ok({}::from(result))",
                return_type
            )
        } else {
            format!(
                "let result = {core_call}.await;\n    \
                 Ok({}::from(result))",
                return_type
            )
        };
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub async fn {}({}) -> {} {{\n    \
             {body}\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    } else if can_delegate {
        let call_args = generators::gen_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let body = if func.error_type.is_some() {
            let wrap = wasm_wrap_return_fn("result", &func.return_type, opaque_types, func.returns_ref);
            format!("let result = {core_call}.map_err(|e| JsValue::from_str(&e.to_string()))?;\n    Ok({wrap})")
        } else {
            wasm_wrap_return_fn(&core_call, &func.return_type, opaque_types, func.returns_ref)
        };
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             {body}\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    } else {
        let body = gen_wasm_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some());
        format!(
            "{attrs}#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             {body}\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    }
}

/// Generate a type-appropriate unimplemented body for WASM (no todo!()).
fn gen_wasm_unimplemented_body(return_type: &TypeRef, fn_name: &str, has_error: bool) -> String {
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(JsValue::from_str(\"{err_msg}\"))")
    } else {
        match return_type {
            TypeRef::Unit => "()".to_string(),
            TypeRef::String | TypeRef::Path => format!("String::from(\"[unimplemented: {fn_name}]\")"),
            TypeRef::Bytes => "Vec::new()".to_string(),
            TypeRef::Primitive(p) => match p {
                eisberg_core::ir::PrimitiveType::Bool => "false".to_string(),
                _ => "0".to_string(),
            },
            TypeRef::Optional(_) => "None".to_string(),
            TypeRef::Vec(_) => "Vec::new()".to_string(),
            TypeRef::Map(_, _) => "Default::default()".to_string(),
            TypeRef::Duration => "0u64".to_string(),
            TypeRef::Named(_) | TypeRef::Json => format!("panic!(\"eisberg: {fn_name} not auto-delegatable\")"),
        }
    }
}

/// WASM-specific return wrapping for opaque methods (adds Js prefix for opaque Named returns).
fn wasm_wrap_return(
    expr: &str,
    return_type: &TypeRef,
    type_name: &str,
    opaque_types: &AHashSet<String>,
    self_is_opaque: bool,
    returns_ref: bool,
) -> String {
    match return_type {
        // Self-returning opaque method
        TypeRef::Named(n) if n == type_name && self_is_opaque => {
            if returns_ref {
                format!("Self {{ inner: Arc::new({expr}.clone()) }}")
            } else {
                format!("Self {{ inner: Arc::new({expr}) }}")
            }
        }
        // Other opaque Named return: needs Js prefix
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if returns_ref {
                format!("Js{n} {{ inner: Arc::new({expr}.clone()) }}")
            } else {
                format!("Js{n} {{ inner: Arc::new({expr}) }}")
            }
        }
        // Optional<opaque>: wrap with Js prefix
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!("{expr}.map(|v| Js{name} {{ inner: Arc::new(v.clone()) }})")
                } else {
                    format!("{expr}.map(|v| Js{name} {{ inner: Arc::new(v) }})")
                }
            }
            _ => generators::wrap_return(expr, return_type, type_name, opaque_types, self_is_opaque, returns_ref),
        },
        // Vec<opaque>: wrap with Js prefix
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!("{expr}.into_iter().map(|v| Js{name} {{ inner: Arc::new(v.clone()) }}).collect()")
                } else {
                    format!("{expr}.into_iter().map(|v| Js{name} {{ inner: Arc::new(v) }}).collect()")
                }
            }
            _ => generators::wrap_return(expr, return_type, type_name, opaque_types, self_is_opaque, returns_ref),
        },
        _ => generators::wrap_return(expr, return_type, type_name, opaque_types, self_is_opaque, returns_ref),
    }
}

/// WASM-specific return wrapping for free functions (no type_name context, adds Js prefix).
fn wasm_wrap_return_fn(
    expr: &str,
    return_type: &TypeRef,
    opaque_types: &AHashSet<String>,
    returns_ref: bool,
) -> String {
    match return_type {
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if returns_ref {
                format!("Js{n} {{ inner: Arc::new({expr}.clone()) }}")
            } else {
                format!("Js{n} {{ inner: Arc::new({expr}) }}")
            }
        }
        TypeRef::Named(_) => {
            if returns_ref {
                format!("{expr}.clone().into()")
            } else {
                format!("{expr}.into()")
            }
        }
        TypeRef::String | TypeRef::Bytes => {
            if returns_ref {
                format!("{expr}.into()")
            } else {
                expr.to_string()
            }
        }
        TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
        TypeRef::Json => format!("{expr}.to_string()"),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!("{expr}.map(|v| Js{name} {{ inner: Arc::new(v.clone()) }})")
                } else {
                    format!("{expr}.map(|v| Js{name} {{ inner: Arc::new(v) }})")
                }
            }
            TypeRef::Named(_) => {
                if returns_ref {
                    format!("{expr}.map(|v| v.clone().into())")
                } else {
                    format!("{expr}.map(Into::into)")
                }
            }
            TypeRef::Path => {
                format!("{expr}.map(Into::into)")
            }
            TypeRef::String | TypeRef::Bytes => {
                if returns_ref {
                    format!("{expr}.map(Into::into)")
                } else {
                    expr.to_string()
                }
            }
            _ => expr.to_string(),
        },
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!("{expr}.into_iter().map(|v| Js{name} {{ inner: Arc::new(v.clone()) }}).collect()")
                } else {
                    format!("{expr}.into_iter().map(|v| Js{name} {{ inner: Arc::new(v) }}).collect()")
                }
            }
            TypeRef::Named(_) => {
                if returns_ref {
                    format!("{expr}.into_iter().map(|v| v.clone().into()).collect()")
                } else {
                    format!("{expr}.into_iter().map(Into::into).collect()")
                }
            }
            TypeRef::Path => {
                format!("{expr}.into_iter().map(Into::into).collect()")
            }
            TypeRef::String | TypeRef::Bytes => {
                if returns_ref {
                    format!("{expr}.into_iter().map(Into::into).collect()")
                } else {
                    expr.to_string()
                }
            }
            _ => expr.to_string(),
        },
        _ => expr.to_string(),
    }
}
