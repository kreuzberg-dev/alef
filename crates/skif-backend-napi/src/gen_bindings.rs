use crate::type_map::NapiMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::generators::{AsyncPattern, RustBindingConfig};
use skif_codegen::naming::to_node_name;
use skif_codegen::shared::{constructor_parts, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef};
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
        builder.add_import("std::collections::HashMap");
        builder.add_import("serde_json");
        builder.add_import(&core_import);

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
                builder.add_item(&gen_struct(typ, &mapper));
                builder.add_item(&gen_struct_methods(typ, &mapper, &cfg));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            builder.add_item(&gen_function(func, &mapper));
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

        // Generate adapter functions
        let adapter_blocks = skif_adapters::generate_adapters(config, Language::Node)?;
        for block in &adapter_blocks {
            builder.add_item(block);
        }

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
    struct_builder.add_attr("napi");
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

/// Generate NAPI methods for a struct.
fn gen_struct_methods(typ: &TypeDef, mapper: &NapiMapper, _cfg: &RustBindingConfig) -> String {
    let mut impl_builder = ImplBuilder::new(&format!("Js{}", typ.name));
    impl_builder.add_attr("napi");

    let constructor = gen_constructor(typ, mapper);
    impl_builder.add_method(&constructor);

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        impl_builder.add_method(&gen_instance_method(method, mapper));
    }
    for method in &statics {
        impl_builder.add_method(&gen_static_method(method, mapper));
    }

    impl_builder.build()
}

/// Generate NAPI methods for an opaque struct (delegates to self.inner).
fn gen_opaque_struct_methods(
    typ: &TypeDef,
    mapper: &NapiMapper,
    _cfg: &RustBindingConfig,
    _opaque_types: &AHashSet<String>,
) -> String {
    let mut impl_builder = ImplBuilder::new(&format!("Js{}", typ.name));
    impl_builder.add_attr("napi");

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        impl_builder.add_method(&gen_opaque_instance_method(method, mapper));
    }
    for method in &statics {
        impl_builder.add_method(&gen_static_method(method, mapper));
    }

    impl_builder.build()
}

/// Generate an opaque instance method that delegates to self.inner.
fn gen_opaque_instance_method(method: &MethodDef, mapper: &NapiMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let call_args: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            if matches!(p.ty, skif_core::ir::TypeRef::Named(_)) {
                if p.optional {
                    format!("{}.map(Into::into)", p.name)
                } else {
                    format!("{}.into()", p.name)
                }
            } else {
                p.name.clone()
            }
        })
        .collect();
    let args_str = call_args.join(", ");

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    let async_kw = if method.is_async { "async " } else { "" };

    let body = if method.is_async {
        let err_map = if method.error_type.is_some() {
            "\n        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?"
        } else {
            ""
        };
        format!(
            "let result = self.inner.{}({}).await{};",
            method.name, args_str, err_map
        )
    } else {
        let err_map = if method.error_type.is_some() {
            ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?"
        } else {
            ""
        };
        format!("let result = self.inner.{}({}){};", method.name, args_str, err_map)
    };

    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}(&self, {params}) -> {return_annotation} {{\n    \
         {body}\n    Ok({return_type}::from(result))\n}}",
        method.name
    )
}

/// Generate a constructor with params wrapped in Option only if optional.
fn gen_constructor(typ: &TypeDef, mapper: &NapiMapper) -> String {
    let params: Vec<String> = typ
        .fields
        .iter()
        .map(|f| {
            let mapped_type = mapper.map_type(&f.ty);
            let param_type = if f.optional {
                format!("Option<{}>", mapped_type)
            } else {
                mapped_type
            };
            format!("{}: {}", f.name, param_type)
        })
        .collect();

    let (_, _, assignments) = constructor_parts(&typ.fields, &|ty| mapper.map_type(ty));

    format!(
        "#[napi(constructor)]\npub fn new({}) -> Self {{\n    Self {{ {} }}\n}}",
        params.join(", "),
        assignments
    )
}

/// Generate an instance method binding.
fn gen_instance_method(method: &MethodDef, mapper: &NapiMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    let async_kw = if method.is_async { "async " } else { "" };
    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}(&self, {params}) -> {return_annotation} {{\n    \
         todo!(\"call into core\")\n}}",
        method.name
    )
}

/// Generate a static method binding.
fn gen_static_method(method: &MethodDef, mapper: &NapiMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    let async_kw = if method.is_async { "async " } else { "" };
    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core\")\n}}",
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
fn gen_function(func: &FunctionDef, mapper: &NapiMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let js_name = to_node_name(&func.name);
    let js_name_attr = if js_name != func.name {
        format!(", js_name = \"{}\"", js_name)
    } else {
        String::new()
    };

    let async_kw = if func.is_async { "async " } else { "" };
    format!(
        "#[napi{js_name_attr}]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core\")\n}}",
        func.name
    )
}

/// Generate `impl From<JsType> for core::Type` (NAPI binding -> core).
fn gen_from_js_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", typ.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = skif_codegen::conversions::field_conversion_to_core(&field.name, &field.ty, field.optional);
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
        let conversion = skif_codegen::conversions::field_conversion_from_core(&field.name, &field.ty, field.optional);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<JsEnum> for core::Enum` (NAPI binding -> core).
fn gen_enum_from_js_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, enum_def.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {}::{} => Self::{},",
            js_name, variant.name, variant.name
        )
        .ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Enum> for JsEnum` (core -> NAPI binding).
fn gen_enum_from_core_to_js_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", enum_def.name, js_name).ok();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", enum_def.name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {core_import}::{}::{} => Self::{},",
            enum_def.name, variant.name, variant.name
        )
        .ok();
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
