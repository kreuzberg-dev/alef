use crate::type_map::RustlerMapper;
use ahash::AHashSet;
use skif_codegen::builder::RustFileBuilder;
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef, TypeRef};
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
        builder.add_import("rustler::{Env, Term, NifResult, ResourceArc}");

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
        let custom_mods = config.custom_modules.for_language(Language::Elixir);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

        let (_module_name, module_prefix) = get_module_info(api, config);

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
                builder.add_item(&gen_opaque_resource(typ, &core_import, &opaque_types));
            } else {
                builder.add_item(&gen_struct(typ, &mapper, &module_prefix));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            if func.is_async {
                builder.add_item(&gen_nif_async_function(func, &mapper, &opaque_types));
            } else {
                builder.add_item(&gen_nif_function(func, &mapper, &opaque_types));
            }
        }

        for typ in &api.types {
            for method in &typ.methods {
                if method.is_async {
                    builder.add_item(&gen_nif_async_method(
                        &typ.name,
                        method,
                        &mapper,
                        typ.is_opaque,
                        &opaque_types,
                    ));
                } else {
                    builder.add_item(&gen_nif_method(
                        &typ.name,
                        method,
                        &mapper,
                        typ.is_opaque,
                        &opaque_types,
                    ));
                }
            }
        }

        let convertible = skif_codegen::conversions::convertible_types(api);
        // From/Into conversions
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible) {
                builder.add_item(&skif_codegen::conversions::gen_from_binding_to_core(typ, &core_import));
                builder.add_item(&skif_codegen::conversions::gen_from_core_to_binding(
                    typ,
                    &core_import,
                    &opaque_types,
                ));
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

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Elixir)?;

        builder.add_item(&gen_nif_init(api, config));

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
fn gen_opaque_resource(typ: &TypeDef, core_import: &str, _opaque_types: &AHashSet<String>) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("#[derive(Clone)]\n");
    out.push_str(&format!("pub struct {} {{\n", typ.name));
    out.push_str(&format!("    inner: Arc<{}::{}>,\n", core_import, typ.name));
    out.push_str("}\n\n");
    out.push_str(&format!("impl rustler::Resource for {} {{}}", typ.name));
    out
}

/// Generate a Rustler NIF struct definition using the shared TypeMapper.
/// Rustler 0.37: NifStruct is a derive macro with #[module = "..."] attribute.
fn gen_struct(typ: &TypeDef, mapper: &RustlerMapper, module_prefix: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);
    writeln!(out, "#[derive(Debug, Clone, rustler::NifStruct)]").unwrap();
    writeln!(out, "#[module = \"{}.{}\"]", module_prefix, typ.name).unwrap();
    writeln!(out, "pub struct {} {{", typ.name).unwrap();

    for field in &typ.fields {
        let field_type = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        writeln!(out, "    pub {}: {},", field.name, field_type).unwrap();
    }

    write!(out, "}}").unwrap();
    out
}

/// Generate a Rustler NIF enum definition (unit enum).
fn gen_enum(enum_def: &EnumDef) -> String {
    let mut lines = vec![
        "#[derive(Debug, Clone, Copy, rustler::NifUnitEnum)]".to_string(),
        format!("pub enum {} {{", enum_def.name),
    ];

    for variant in &enum_def.variants {
        lines.push(format!("    {},", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate a Rustler NIF free function using the shared TypeMapper.
fn gen_nif_function(func: &FunctionDef, mapper: &RustlerMapper, opaque_types: &AHashSet<String>) -> String {
    use skif_core::ir::TypeRef;

    // If any param is an opaque Named type, use ResourceArc wrapping
    let params_str = func
        .params
        .iter()
        .map(|p| {
            if let TypeRef::Named(n) = &p.ty {
                if opaque_types.contains(n) {
                    return format!("{}: ResourceArc<{}>", p.name, n);
                }
            }
            format!("{}: {}", p.name, mapper.map_type(&p.ty))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let return_type = map_return_type(&func.return_type, mapper, opaque_types);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let body = gen_rustler_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some());
    format!(
        "#[rustler::nif]\npub fn {}({params_str}) -> {return_annotation} {{\n    \
         {body}\n}}",
        func.name
    )
}

/// Generate a Rustler NIF async free function (sync wrapper scheduled on DirtyCpu).
fn gen_nif_async_function(func: &FunctionDef, mapper: &RustlerMapper, opaque_types: &AHashSet<String>) -> String {
    use skif_core::ir::TypeRef;

    let params_str = func
        .params
        .iter()
        .map(|p| {
            if let TypeRef::Named(n) = &p.ty {
                if opaque_types.contains(n) {
                    return format!("{}: ResourceArc<{}>", p.name, n);
                }
            }
            format!("{}: {}", p.name, mapper.map_type(&p.ty))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let return_type = map_return_type(&func.return_type, mapper, opaque_types);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let body = gen_rustler_unimplemented_body(
        &func.return_type,
        &format!("{}_async", func.name),
        func.error_type.is_some(),
    );
    // Append "_async" to function name for Rustler
    format!(
        "#[rustler::nif(schedule = \"DirtyCpu\")]\npub fn {}_async({params_str}) -> {return_annotation} {{\n    \
         {body}\n\
         }}",
        func.name
    )
}

/// Generate a Rustler NIF method for a struct using the shared TypeMapper.
fn gen_nif_method(
    struct_name: &str,
    method: &MethodDef,
    mapper: &RustlerMapper,
    is_opaque: bool,
    opaque_types: &AHashSet<String>,
) -> String {
    use skif_core::ir::TypeRef;

    let method_fn_name = format!("{}_{}", struct_name.to_lowercase(), method.name);

    let mut params = if method.receiver.is_some() {
        if is_opaque {
            vec![format!("resource: ResourceArc<{}>", struct_name)]
        } else {
            vec![format!("obj: {}", struct_name)]
        }
    } else {
        vec![]
    };

    for p in &method.params {
        if let TypeRef::Named(n) = &p.ty {
            if opaque_types.contains(n) {
                params.push(format!("{}: ResourceArc<{}>", p.name, n));
                continue;
            }
        }
        let param_type = mapper.map_type(&p.ty);
        params.push(format!("{}: {}", p.name, param_type));
    }

    let return_type = map_return_type(&method.return_type, mapper, opaque_types);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_rustler_unimplemented_body(&method.return_type, &method_fn_name, method.error_type.is_some());
    format!(
        "#[rustler::nif]\npub fn {}({}) -> {} {{\n    \
         {body}\n}}",
        method_fn_name,
        params.join(", "),
        return_annotation
    )
}

/// Generate a Rustler NIF async method for a struct (sync wrapper scheduled on DirtyCpu).
fn gen_nif_async_method(
    struct_name: &str,
    method: &MethodDef,
    mapper: &RustlerMapper,
    is_opaque: bool,
    opaque_types: &AHashSet<String>,
) -> String {
    use skif_core::ir::TypeRef;

    let method_fn_name = format!("{}_{}_async", struct_name.to_lowercase(), method.name);

    let mut params = if method.receiver.is_some() {
        if is_opaque {
            vec![format!("resource: ResourceArc<{}>", struct_name)]
        } else {
            vec![format!("obj: {}", struct_name)]
        }
    } else {
        vec![]
    };

    for p in &method.params {
        if let TypeRef::Named(n) = &p.ty {
            if opaque_types.contains(n) {
                params.push(format!("{}: ResourceArc<{}>", p.name, n));
                continue;
            }
        }
        let param_type = mapper.map_type(&p.ty);
        params.push(format!("{}: {}", p.name, param_type));
    }

    let return_type = map_return_type(&method.return_type, mapper, opaque_types);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_rustler_unimplemented_body(&method.return_type, &method_fn_name, method.error_type.is_some());
    format!(
        "#[rustler::nif(schedule = \"DirtyCpu\")]\npub fn {}({}) -> {} {{\n    \
         {body}\n\
         }}",
        method_fn_name,
        params.join(", "),
        return_annotation
    )
}

/// Generate a type-appropriate unimplemented body for Rustler (no todo!()).
fn gen_rustler_unimplemented_body(return_type: &skif_core::ir::TypeRef, fn_name: &str, has_error: bool) -> String {
    use skif_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(String::from(\"{err_msg}\"))")
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
            TypeRef::Named(_) | TypeRef::Json => {
                format!("todo!(\"Not auto-delegatable: {fn_name} -- return type requires custom implementation\")")
            }
        }
    }
}

/// Map a return type, wrapping opaque Named types in ResourceArc.
fn map_return_type(ty: &skif_core::ir::TypeRef, mapper: &RustlerMapper, opaque_types: &AHashSet<String>) -> String {
    use skif_core::ir::TypeRef;
    match ty {
        TypeRef::Named(n) if opaque_types.contains(n) => format!("ResourceArc<{n}>"),
        _ => mapper.map_type(ty),
    }
}

/// Generate the rustler::init! macro invocation.
fn gen_nif_init(api: &ApiSurface, config: &SkifConfig) -> String {
    let mut exports = vec![];

    // Custom NIF function registrations (before generated ones)
    if let Some(reg) = config.custom_registrations.for_language(Language::Elixir) {
        for func in &reg.functions {
            exports.push(func.clone());
        }
    }

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
