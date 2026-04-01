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
        let core_import = config.core_import();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("rustler::prelude::*");
        builder.add_import("std::collections::HashMap");
        builder.add_import(&core_import);

        let (_module_name, module_prefix) = get_module_info(api, config);

        // Check if we have opaque types and add Arc import if needed
        let has_opaque = api.types.iter().any(|t| t.is_opaque);
        if has_opaque {
            builder.add_import("std::sync::Arc");
        }

        for typ in &api.types {
            if typ.is_opaque {
                builder.add_item(&gen_opaque_resource(typ, &core_import));
            } else {
                builder.add_item(&gen_struct(typ, &mapper, &module_prefix));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            if func.is_async {
                builder.add_item(&gen_nif_async_function(func, &mapper));
            } else {
                builder.add_item(&gen_nif_function(func, &mapper));
            }
        }

        for typ in &api.types {
            for method in &typ.methods {
                if method.is_async {
                    builder.add_item(&gen_nif_async_method(&typ.name, method, &mapper));
                } else {
                    builder.add_item(&gen_nif_method(&typ.name, method, &mapper));
                }
            }
        }

        // From/Into conversions
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ) {
                builder.add_item(&skif_codegen::conversions::gen_from_binding_to_core(typ, &core_import));
                builder.add_item(&skif_codegen::conversions::gen_from_core_to_binding(typ, &core_import));
            }
        }
        for e in &api.enums {
            if skif_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&skif_codegen::conversions::gen_enum_from_binding_to_core(
                    e,
                    &core_import,
                ));
                builder.add_item(&skif_codegen::conversions::gen_enum_from_core_to_binding(
                    e,
                    &core_import,
                ));
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

/// Generate an opaque Rustler resource struct with inner Arc.
fn gen_opaque_resource(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("#[derive(Clone)]\n");
    out.push_str(&format!("pub struct {} {{\n", typ.name));
    out.push_str(&format!("    inner: std::sync::Arc<{}::{}>,\n", core_import, typ.name));
    out.push_str("}\n\n");
    out.push_str(&format!("impl rustler::Resource for {} {{}}", typ.name));
    out
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
         todo!(\"call into core\")\n}}",
        func.name
    )
}

/// Generate a Rustler NIF async free function (sync wrapper scheduled on DirtyCpu).
fn gen_nif_async_function(func: &FunctionDef, mapper: &RustlerMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    // Append "_async" to function name for Rustler
    format!(
        "#[rustler::nif(schedule = \"DirtyCpu\")]\npub fn {}_async({params}) -> {return_annotation} {{\n    \
         let rt = tokio::runtime::Runtime::new()\n        \
         .map_err(|e| e.to_string())?;\n    \
         rt.block_on(async {{\n        \
         todo!(\"call into core\")\n    \
         }}).map_err(|e| e.to_string())\n        \
         .map(ExtractionResult::from)\n\
         }}",
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
         todo!(\"call into core\")\n}}",
        method_fn_name,
        params.join(", "),
        return_annotation
    )
}

/// Generate a Rustler NIF async method for a struct (sync wrapper scheduled on DirtyCpu).
fn gen_nif_async_method(struct_name: &str, method: &MethodDef, mapper: &RustlerMapper) -> String {
    let method_fn_name = format!("{}_{}_async", struct_name.to_lowercase(), method.name);

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
        "#[rustler::nif(schedule = \"DirtyCpu\")]\npub fn {}({}) -> {} {{\n    \
         let rt = tokio::runtime::Runtime::new()\n        \
         .map_err(|e| e.to_string())?;\n    \
         rt.block_on(async {{\n        \
         todo!(\"call into core\")\n    \
         }}).map_err(|e| e.to_string())\n        \
         .map(ExtractionResult::from)\n\
         }}",
        method_fn_name,
        params.join(", "),
        return_annotation
    )
}

/// Generate the rustler::init! macro invocation.
fn gen_nif_init(api: &ApiSurface) -> String {
    let mut exports = vec![];

    for func in &api.functions {
        let func_name = if func.is_async {
            format!("{}_async", func.name)
        } else {
            func.name.clone()
        };
        exports.push(func_name);
    }

    for typ in &api.types {
        for method in &typ.methods {
            let method_name = if method.is_async {
                format!("{}_{}_async", typ.name.to_lowercase(), method.name)
            } else {
                format!("{}_{}", typ.name.to_lowercase(), method.name)
            };
            exports.push(method_name);
        }
    }

    if exports.is_empty() {
        "rustler::init!(\"elixir_module\", []);".to_string()
    } else {
        format!("rustler::init!(\"elixir_module\", [{}]);", exports.join(", "))
    }
}
