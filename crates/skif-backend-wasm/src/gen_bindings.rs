use crate::type_map::WasmMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::naming::to_node_name;
use skif_codegen::shared::constructor_parts;
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

/// Check if a TypeRef is a Copy type that shouldn't be cloned.
fn is_copy_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_) => true, // All primitives are Copy
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
        builder.add_import("std::collections::HashMap");
        builder.add_import(&core_import);

        // Clippy allows for generated code
        builder.add_inner_attribute("allow(clippy::too_many_arguments)");
        builder.add_inner_attribute("allow(clippy::missing_errors_doc)");

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
                builder.add_item(&gen_opaque_struct(typ));
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &opaque_types));
            } else {
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

        let convertible = skif_codegen::conversions::convertible_types(api);
        // From/Into conversions (WASM uses Js prefix, so we need custom generation)
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible)
                && !exclude_types.contains(&typ.name)
            {
                builder.add_item(&gen_from_js_binding_to_core(typ, &core_import));
                builder.add_item(&gen_from_core_to_js_binding(typ, &core_import));
            }
        }
        for e in &api.enums {
            if skif_codegen::conversions::can_generate_enum_conversion(e) && !exclude_types.contains(&e.name) {
                builder.add_item(&gen_enum_from_js_binding_to_core(e, &core_import));
                builder.add_item(&gen_enum_from_core_to_js_binding(e, &core_import));
            }
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Wasm)?;

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
fn gen_opaque_struct(typ: &TypeDef) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut struct_builder = StructBuilder::new(&js_name);
    struct_builder.add_attr("wasm_bindgen");
    struct_builder.add_derive("Clone");

    // We can't use StructBuilder for private fields, so build manually
    let mut out = String::with_capacity(256);
    writeln!(out, "#[derive(Clone)]").ok();
    writeln!(out, "#[wasm_bindgen]").ok();
    writeln!(out, "pub struct {} {{", js_name).ok();
    writeln!(out, "    inner: std::sync::Arc<skif_core::{}>,", typ.name).ok();
    write!(out, "}}").ok();
    out
}

/// Generate wasm-bindgen methods for an opaque struct.
fn gen_opaque_struct_methods(typ: &TypeDef, mapper: &WasmMapper, _opaque_types: &AHashSet<String>) -> String {
    let js_name = format!("Js{}", typ.name);
    let mut impl_builder = ImplBuilder::new(&js_name);
    impl_builder.add_attr("wasm_bindgen");

    for method in &typ.methods {
        impl_builder.add_method(&gen_opaque_method(method, mapper, &typ.name));
    }

    impl_builder.build()
}

/// Generate a method for an opaque wasm-bindgen struct that delegates to self.inner.
fn gen_opaque_method(method: &MethodDef, mapper: &WasmMapper, _type_name: &str) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, mapper.map_type(&p.ty)))
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let call_args: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            if matches!(p.ty, skif_core::ir::TypeRef::Named(_)) {
                format!("{}.into()", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect();
    let args_str = call_args.join(", ");

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    if method.is_async {
        let body = if method.error_type.is_some() {
            format!(
                "let result = self.inner.{}({}).await\n        \
                 .map_err(|e| JsValue::from_str(&e.to_string()))?;\n    \
                 Ok({}::from(result))",
                method.name, args_str, return_type
            )
        } else {
            format!(
                "let result = self.inner.{}({}).await;\n    \
                 Ok({}::from(result))",
                method.name, args_str, return_type
            )
        };
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub async fn {}(&self, {}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else if method.is_static {
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else {
        let body = if method.error_type.is_some() {
            format!(
                "self.inner.{}({})\n        .map(|r| {}::from(r))\n        \
                 .map_err(|e| JsValue::from_str(&e.to_string()))",
                method.name, args_str, return_type
            )
        } else {
            format!("{}::from(self.inner.{}({}))", return_type, method.name, args_str)
        };
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub fn {}(&self, {}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    }
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
            impl_builder.add_method(&gen_method(method, mapper, &typ.name));
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
fn gen_method(method: &MethodDef, mapper: &WasmMapper, type_name: &str) -> String {
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, mapper.map_type(&p.ty)))
        .collect();

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());
    let core_import = "skif_core";

    let js_name = to_node_name(&method.name);
    let js_name_attr = if js_name != method.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    if method.is_async {
        // For WASM, native async fn automatically becomes a Promise
        let call_args = method
            .params
            .iter()
            .map(|p| {
                if matches!(p.ty, skif_core::ir::TypeRef::Named(_)) {
                    format!("{}.into()", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
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
            "#[wasm_bindgen{js_name_attr}]\npub async fn {}(&self, {}) -> {} {{\n    \
             {body}\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else if method.is_static {
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            method.name,
            params.join(", "),
            return_annotation
        )
    } else {
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub fn {}(&self, {}) -> {} {{\n    \
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
    let core_import = "skif_core";

    let js_name = to_node_name(&func.name);
    let js_name_attr = if js_name != func.name {
        format!("(js_name = \"{}\")", js_name)
    } else {
        String::new()
    };

    if func.is_async {
        // For WASM, native async fn automatically becomes a Promise
        let call_args = func
            .params
            .iter()
            .map(|p| {
                if matches!(p.ty, skif_core::ir::TypeRef::Named(_)) {
                    format!("{}.into()", p.name)
                } else {
                    p.name.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
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
            "#[wasm_bindgen{js_name_attr}]\npub async fn {}({}) -> {} {{\n    \
             {body}\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    } else {
        format!(
            "#[wasm_bindgen{js_name_attr}]\npub fn {}({}) -> {} {{\n    \
             todo!(\"call into core implementation\")\n}}",
            func.name,
            params.join(", "),
            return_annotation
        )
    }
}

/// Generate `impl From<JsType> for core::Type` (WASM binding -> core).
fn gen_from_js_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", typ.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, typ.name).unwrap();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in &typ.fields {
        let conversion = skif_codegen::conversions::field_conversion_to_core(&field.name, &field.ty, field.optional);
        writeln!(out, "            {conversion},").unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<core::Type> for JsType` (core -> WASM binding).
fn gen_from_core_to_js_binding(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", typ.name);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", typ.name, js_name).unwrap();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", typ.name).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in &typ.fields {
        let conversion = skif_codegen::conversions::field_conversion_from_core(
            &field.name,
            &field.ty,
            field.optional,
            field.sanitized,
        );
        writeln!(out, "            {conversion},").unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<JsEnum> for core::Enum` (WASM binding -> core).
fn gen_enum_from_js_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", js_name, enum_def.name).unwrap();
    writeln!(out, "    fn from(val: {}) -> Self {{", js_name).unwrap();
    writeln!(out, "        match val {{").unwrap();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {}::{} => Self::{},",
            js_name, variant.name, variant.name
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<core::Enum> for JsEnum` (core -> WASM binding).
fn gen_enum_from_core_to_js_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    let js_name = format!("Js{}", enum_def.name);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", enum_def.name, js_name).unwrap();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", enum_def.name).unwrap();
    writeln!(out, "        match val {{").unwrap();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {core_import}::{}::{} => Self::{},",
            enum_def.name, variant.name, variant.name
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}
