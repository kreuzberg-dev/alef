use crate::type_map::NapiMapper;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::generators::RustBindingConfig;
use skif_codegen::shared::{constructor_parts, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

pub struct NapiBackend;

impl NapiBackend {
    fn binding_config() -> RustBindingConfig<'static> {
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
        let cfg = Self::binding_config();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("napi::*");
        builder.add_import("napi_derive::napi");
        builder.add_import("std::collections::HashMap");
        builder.add_import("serde_json");

        // NAPI has some unique patterns: Js-prefixed names, Option-wrapped fields,
        // and custom constructor. Use shared generators for enums and functions,
        // but keep struct/method generation custom.
        for typ in &api.types {
            if !typ.is_opaque {
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

/// Generate a NAPI struct with Js-prefixed name and all fields wrapped in Option.
fn gen_struct(typ: &TypeDef, mapper: &NapiMapper) -> String {
    let mut struct_builder = StructBuilder::new(&format!("Js{}", typ.name));
    struct_builder.add_attr("napi");
    struct_builder.add_derive("Clone");

    for field in &typ.fields {
        let field_type = format!("Option<{}>", mapper.map_type(&field.ty));
        struct_builder.add_field(&field.name, &field_type, vec![]);
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

/// Generate a constructor with all params wrapped in Option.
fn gen_constructor(typ: &TypeDef, mapper: &NapiMapper) -> String {
    let params: Vec<String> = typ
        .fields
        .iter()
        .map(|f| format!("{}: Option<{}>", f.name, mapper.map_type(&f.ty)))
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

    let async_kw = if method.is_async { "async " } else { "" };
    format!(
        "#[napi]\npub {async_kw}fn {}(&self, {params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n}}",
        method.name
    )
}

/// Generate a static method binding.
fn gen_static_method(method: &MethodDef, mapper: &NapiMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let async_kw = if method.is_async { "async " } else { "" };
    format!(
        "#[napi]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n}}",
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

    let async_kw = if func.is_async { "async " } else { "" };
    format!(
        "#[napi]\npub {async_kw}fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n}}",
        func.name
    )
}
