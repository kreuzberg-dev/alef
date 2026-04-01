use crate::type_map::RustlerMapper;
use skif_codegen::builder::{RustFileBuilder, StructBuilder};
use skif_codegen::shared::function_params;
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

pub struct RustlerBackend;

impl Backend for RustlerBackend {
    fn name(&self) -> &str {
        "rustler"
    }

    fn language(&self) -> Language {
        Language::Elixir
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
        let mapper = RustlerMapper;

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("rustler::prelude::*");
        builder.add_import("std::collections::HashMap");

        let (_module_name, module_prefix) = get_module_info(api, config);

        for typ in &api.types {
            if !typ.is_opaque {
                builder.add_item(&gen_struct(typ, &mapper, &module_prefix));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            builder.add_item(&gen_nif_function(func, &mapper));
        }

        for typ in &api.types {
            if !typ.is_opaque {
                for method in &typ.methods {
                    builder.add_item(&gen_nif_method(&typ.name, method, &mapper));
                }
            }
        }

        builder.add_item(&gen_nif_init(api));

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.elixir.as_ref(),
            &config.crate_config.name,
            "packages/elixir/native/{name}_rustler/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }
}

/// Get module name and prefix from config or derive from crate name.
fn get_module_info(_api: &ApiSurface, config: &SkifConfig) -> (String, String) {
    let app_name = config.elixir_app_name();
    let module_prefix = {
        use heck::ToPascalCase;
        app_name.to_pascal_case()
    };
    (app_name, module_prefix)
}

/// Generate a Rustler NIF struct definition using the shared TypeMapper.
fn gen_struct(typ: &TypeDef, mapper: &RustlerMapper, module_prefix: &str) -> String {
    let mut struct_builder = StructBuilder::new(&typ.name);
    struct_builder.add_attr(&format!("rustler::NifStruct(module = \"{}\")", module_prefix));
    struct_builder.add_derive("Clone");
    struct_builder.add_derive("Debug");

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

/// Generate a Rustler NIF enum definition (unit enum).
fn gen_enum(enum_def: &EnumDef) -> String {
    let mut lines = vec![
        "#[derive(NifUnitEnum)]".to_string(),
        "#[derive(Clone, Copy)]".to_string(),
        format!("pub enum {} {{", enum_def.name),
    ];

    for variant in &enum_def.variants {
        lines.push(format!("    {},", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate a Rustler NIF free function using the shared TypeMapper.
fn gen_nif_function(func: &FunctionDef, mapper: &RustlerMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    format!(
        "#[rustler::nif]\npub fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n}}",
        func.name
    )
}

/// Generate a Rustler NIF method for a struct using the shared TypeMapper.
fn gen_nif_method(struct_name: &str, method: &MethodDef, mapper: &RustlerMapper) -> String {
    let method_fn_name = format!("{}_{}", struct_name.to_lowercase(), method.name);

    let mut params = if method.receiver.is_some() {
        vec![format!("resource: ResourceArc<{}>", struct_name)]
    } else {
        vec![]
    };

    for p in &method.params {
        let param_type = mapper.map_type(&p.ty);
        params.push(format!("{}: {}", p.name, param_type));
    }

    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "#[rustler::nif]\npub fn {}({}) -> {} {{\n    \
         todo!(\"call into core implementation\")\n}}",
        method_fn_name,
        params.join(", "),
        return_annotation
    )
}

/// Generate the rustler::init! macro invocation.
fn gen_nif_init(api: &ApiSurface) -> String {
    let mut exports = vec![];

    for func in &api.functions {
        exports.push(func.name.clone());
    }

    for typ in &api.types {
        if !typ.is_opaque {
            for method in &typ.methods {
                exports.push(format!("{}_{}", typ.name.to_lowercase(), method.name));
            }
        }
    }

    if exports.is_empty() {
        "rustler::init!(\"elixir_module\", []);".to_string()
    } else {
        format!("rustler::init!(\"elixir_module\", [{}]);", exports.join(", "))
    }
}
