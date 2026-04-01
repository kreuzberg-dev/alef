use crate::type_map::WasmMapper;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::shared::constructor_parts;
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

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

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("wasm_bindgen::prelude::*");
        builder.add_import("std::collections::HashMap");

        for typ in &api.types {
            if !typ.is_opaque && !exclude_types.contains(&typ.name) {
                builder.add_item(&gen_struct(typ, &mapper));
                builder.add_item(&gen_struct_methods(typ, &mapper, &exclude_types));
            }
        }

        for enum_def in &api.enums {
            if !exclude_types.contains(&enum_def.name) {
                builder.add_item(&gen_enum(enum_def));
            }
        }

        for func in &api.functions {
            if !exclude_functions.contains(&func.name) {
                builder.add_item(&gen_function(func, &mapper));
            }
        }

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

/// Generate a wasm-bindgen struct definition using the shared TypeMapper.
fn gen_struct(typ: &TypeDef, mapper: &WasmMapper) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut struct_builder = StructBuilder::new(&js_name);
    struct_builder.add_attr("wasm_bindgen");
    struct_builder.add_derive("Clone");

    for field in &typ.fields {
        let field_type = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        struct_builder.add_field(&field.name, &field_type, vec![]);
    }

    struct_builder.build()
}

/// Generate wasm-bindgen methods for a struct.
fn gen_struct_methods(typ: &TypeDef, mapper: &WasmMapper, exclude_types: &[String]) -> String {
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
            impl_builder.add_method(&gen_method(method, mapper));
        }
    }

    impl_builder.build()
}

/// Generate a constructor method.
fn gen_new_method(typ: &TypeDef, mapper: &WasmMapper) -> String {
    let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
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

    format!(
        "#[wasm_bindgen(getter)]\npub fn {}(&self) -> {} {{\n    self.{}.clone()\n}}",
        field.name, field_type, field.name
    )
}

/// Generate a setter method for a field.
fn gen_setter(field: &FieldDef, mapper: &WasmMapper) -> String {
    let field_type = if field.optional {
        mapper.optional(&mapper.map_type(&field.ty))
    } else {
        mapper.map_type(&field.ty)
    };

    format!(
        "#[wasm_bindgen(setter)]\npub fn set_{}(&mut self, value: {}) {{\n    self.{} = value;\n}}",
        field.name, field_type, field.name
    )
}

/// Generate a method binding for a struct method.
fn gen_method(method: &MethodDef, mapper: &WasmMapper) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, mapper.map_type(&p.ty)))
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    if method.is_async {
        format!(
            "pub async fn {}(&self, {}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else if method.is_static {
        format!(
            "#[wasm_bindgen(static)]\npub fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else {
        format!(
            "pub fn {}(&self, {}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
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
fn gen_function(func: &FunctionDef, mapper: &WasmMapper) -> String {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, mapper.map_type(&p.ty)))
        .collect();

    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    if func.is_async {
        format!(
            "#[wasm_bindgen]\npub async fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    } else {
        format!(
            "#[wasm_bindgen]\npub fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    }
}
