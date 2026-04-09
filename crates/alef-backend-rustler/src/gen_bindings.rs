use crate::type_map::RustlerMapper;
use ahash::AHashSet;
use alef_codegen::builder::RustFileBuilder;
use alef_codegen::generators;
use alef_codegen::shared;
use alef_codegen::type_mapper::TypeMapper;
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, ParamDef, TypeDef, TypeRef};
use heck::{ToPascalCase, ToSnakeCase};
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

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let mapper = RustlerMapper;
        let core_import = config.core_import();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("rustler::{Env, Term, NifResult, ResourceArc}");

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
                if typ.has_default {
                    builder.add_item(&alef_codegen::generators::gen_struct_default_impl(typ, ""));
                }
                // Generate config constructor if type has Default
                if typ.has_default && !typ.fields.is_empty() {
                    let config_impl = gen_rustler_config_impl(typ, &mapper);
                    builder.add_item(&config_impl);
                }
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            if func.is_async {
                builder.add_item(&gen_nif_async_function(func, &mapper, &opaque_types, &core_import));
            } else {
                builder.add_item(&gen_nif_function(func, &mapper, &opaque_types, &core_import));
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
                        &core_import,
                    ));
                } else {
                    builder.add_item(&gen_nif_method(
                        &typ.name,
                        method,
                        &mapper,
                        typ.is_opaque,
                        &opaque_types,
                        &core_import,
                    ));
                }
            }
        }

        let binding_to_core = alef_codegen::conversions::convertible_types(api);
        let core_to_binding = alef_codegen::conversions::core_to_binding_convertible_types(api);
        // From/Into conversions
        for typ in &api.types {
            if alef_codegen::conversions::can_generate_conversion(typ, &binding_to_core) {
                builder.add_item(&alef_codegen::conversions::gen_from_binding_to_core(typ, &core_import));
            }
            if alef_codegen::conversions::can_generate_conversion(typ, &core_to_binding) {
                builder.add_item(&alef_codegen::conversions::gen_from_core_to_binding(
                    typ,
                    &core_import,
                    &opaque_types,
                ));
            }
        }
        for e in &api.enums {
            if alef_codegen::conversions::can_generate_enum_conversion(e) {
                builder.add_item(&alef_codegen::conversions::gen_enum_from_binding_to_core(
                    e,
                    &core_import,
                ));
            }
            if alef_codegen::conversions::can_generate_enum_conversion_from_core(e) {
                builder.add_item(&alef_codegen::conversions::gen_enum_from_core_to_binding(
                    e,
                    &core_import,
                ));
            }
        }

        // Error converter functions
        for error in &api.errors {
            builder.add_item(&alef_codegen::error_gen::gen_rustler_error_converter(
                error,
                &core_import,
            ));
        }

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Elixir)?;

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

    fn generate_public_api(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let app_name = config.elixir_app_name();

        // Generate the main Elixir wrapper module file
        let mut content = String::from("# This file is auto-generated by alef. DO NOT EDIT.\n");
        content.push_str(&format!("defmodule {} do\n", app_name.to_pascal_case()));
        content.push_str(&format!("  @moduledoc \"High-level API for {}.\"\n\n", app_name));

        // Generate wrapper functions for all API functions
        for func in &api.functions {
            let doc_line = func.doc.lines().next().unwrap_or("Function");
            content.push_str(&format!("  @doc \"{}\"\n", doc_line));

            // Generate @spec if not async
            if !func.is_async {
                content.push_str(&format!("  @spec {}(", func.name.to_snake_case()));
                let param_types: Vec<String> = func.params.iter().map(|_| "term()".to_string()).collect();
                content.push_str(&param_types.join(", "));
                content.push_str(") :: {:ok, term()} | {:error, term()}\n");
            }

            // Generate function signature
            let native_mod = format!("{}.Native", app_name.to_pascal_case());
            content.push_str(&format!("  def {}(", func.name.to_snake_case()));
            let params: Vec<String> = func.params.iter().map(|p| p.name.to_snake_case()).collect();
            content.push_str(&params.join(", "));
            content.push_str(") do\n");
            content.push_str(&format!(
                "    {}.{}({})\n",
                native_mod,
                func.name.to_snake_case(),
                params.join(", ")
            ));
            content.push_str("  end\n\n");
        }

        content.push_str("end\n");

        let output_dir = resolve_output_dir(
            config.output.elixir.as_ref(),
            &config.crate_config.name,
            "packages/elixir/lib/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join(format!("{}.ex", app_name.to_snake_case())),
            content,
            generated_header: false,
        }])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "mix",
            crate_suffix: "-rustler",
            depends_on_ffi: false,
            post_build: vec![],
        })
    }
}

/// Get module name and prefix from config or derive from crate name.
fn get_module_info(_api: &ApiSurface, config: &AlefConfig) -> (String, String) {
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
    let core_path = alef_codegen::conversions::core_type_path(typ, core_import);
    out.push_str(&format!("    inner: Arc<{}>,\n", core_path));
    out.push_str("}\n\n");
    out.push_str(&format!("impl rustler::Resource for {} {{}}", typ.name));
    out
}

/// Generate a Rustler NIF struct definition using the shared TypeMapper.
/// Rustler 0.37: NifStruct is a derive macro with #[module = "..."] attribute.
fn gen_struct(typ: &TypeDef, mapper: &RustlerMapper, module_prefix: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);
    writeln!(out, "#[derive(Debug, Clone, rustler::NifStruct)]").ok();
    writeln!(out, "#[module = \"{}.{}\"]", module_prefix, typ.name).ok();
    writeln!(out, "pub struct {} {{", typ.name).ok();

    for field in &typ.fields {
        let field_type = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        writeln!(out, "    pub {}: {},", field.name, field_type).ok();
    }

    write!(out, "}}").ok();
    out
}

/// Generate a Rustler config constructor impl for a type with `has_default`.
fn gen_rustler_config_impl(typ: &TypeDef, mapper: &RustlerMapper) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(512);

    writeln!(out, "impl {} {{", typ.name).ok();

    // Generate kwargs constructor using config_gen helper
    let map_fn = |ty: &TypeRef| mapper.map_type(ty);
    let config_method = alef_codegen::config_gen::gen_rustler_kwargs_constructor(typ, &map_fn);
    write!(out, "    {}", config_method).ok();

    writeln!(out, "}}").ok();
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

    // Default impl for config constructor unwrap_or_default()
    if let Some(first) = enum_def.variants.first() {
        lines.push(String::new());
        lines.push(format!("impl Default for {} {{", enum_def.name));
        lines.push(format!("    fn default() -> Self {{ Self::{} }}", first.name));
        lines.push("}".to_string());
    }

    lines.join("\n")
}

/// Build call argument expressions for Rustler (opaque Named types access .inner via ResourceArc).
fn gen_rustler_call_args(params: &[ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                format!("&{}.inner", p.name)
            }
            TypeRef::Named(_) => {
                if p.optional {
                    format!("{}.map(Into::into)", p.name)
                } else {
                    format!("{}.into()", p.name)
                }
            }
            TypeRef::String | TypeRef::Char => format!("&{}", p.name),
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            TypeRef::Duration => format!("std::time::Duration::from_secs({})", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Wrap a return expression for Rustler (opaque types get ResourceArc wrapping).
fn gen_rustler_wrap_return(
    expr: &str,
    return_type: &TypeRef,
    _type_name: &str,
    opaque_types: &AHashSet<String>,
    returns_ref: bool,
) -> String {
    match return_type {
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if returns_ref {
                format!("ResourceArc::new({n} {{ inner: Arc::new({expr}.clone()) }})")
            } else {
                format!("ResourceArc::new({n} {{ inner: Arc::new({expr}) }})")
            }
        }
        TypeRef::Named(_) => {
            if returns_ref {
                format!("{expr}.clone().into()")
            } else {
                format!("{expr}.into()")
            }
        }
        TypeRef::String | TypeRef::Char | TypeRef::Bytes => format!("{expr}.into()"),
        TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
        TypeRef::Duration => format!("{expr}.as_secs()"),
        TypeRef::Json => format!("{expr}.to_string()"),
        _ => expr.to_string(),
    }
}

/// Build call argument expressions for Rustler opaque method (receiver is `resource`).
fn gen_rustler_method_call_args(params: &[ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                format!("&{}.inner", p.name)
            }
            TypeRef::Named(_) => {
                if p.optional {
                    format!("{}.map(Into::into)", p.name)
                } else {
                    format!("{}.into()", p.name)
                }
            }
            TypeRef::String | TypeRef::Char => format!("&{}", p.name),
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            TypeRef::Duration => format!("std::time::Duration::from_secs({})", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate a Rustler NIF free function using the shared TypeMapper.
fn gen_nif_function(
    func: &FunctionDef,
    mapper: &RustlerMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
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

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let call_args = gen_rustler_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        if func.error_type.is_some() {
            let wrap = gen_rustler_wrap_return("result", &func.return_type, "", opaque_types, func.returns_ref);
            format!("let result = {core_call}.map_err(|e| e.to_string())?;\n    Ok({wrap})")
        } else {
            gen_rustler_wrap_return(&core_call, &func.return_type, "", opaque_types, func.returns_ref)
        }
    } else {
        gen_rustler_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some())
    };
    format!(
        "#[rustler::nif]\npub fn {}({params_str}) -> {return_annotation} {{\n    \
         {body}\n}}",
        func.name
    )
}

/// Generate a Rustler NIF async free function (sync wrapper scheduled on DirtyCpu).
fn gen_nif_async_function(
    func: &FunctionDef,
    mapper: &RustlerMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
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

    let can_delegate = shared::can_auto_delegate_function(func, opaque_types);

    let body = if can_delegate {
        let call_args = gen_rustler_call_args(&func.params, opaque_types);
        let core_call = format!("{core_import}::{}({call_args})", func.name);
        let result_wrap = gen_rustler_wrap_return("result", &func.return_type, "", opaque_types, func.returns_ref);
        if func.error_type.is_some() {
            format!(
                "let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;\n    \
                 let result = rt.block_on(async {{ {core_call}.await }}).map_err(|e| e.to_string())?;\n    \
                 Ok({result_wrap})"
            )
        } else {
            format!(
                "let rt = tokio::runtime::Runtime::new().unwrap();\n    \
                 let result = rt.block_on(async {{ {core_call}.await }});\n    \
                 {result_wrap}"
            )
        }
    } else {
        gen_rustler_unimplemented_body(
            &func.return_type,
            &format!("{}_async", func.name),
            func.error_type.is_some(),
        )
    };
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
    core_import: &str,
) -> String {
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

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate {
        let call_args = gen_rustler_method_call_args(&method.params, opaque_types);
        let core_call = if is_opaque {
            format!("resource.inner.{}({})", method.name, call_args)
        } else {
            // Non-opaque: convert binding struct to core type, then call
            format!(
                "{core_import}::{}::from(obj).{}({})",
                struct_name, method.name, call_args
            )
        };
        if method.error_type.is_some() {
            let wrap = gen_rustler_wrap_return(
                "result",
                &method.return_type,
                struct_name,
                opaque_types,
                method.returns_ref,
            );
            format!("let result = {core_call}.map_err(|e| e.to_string())?;\n    Ok({wrap})")
        } else {
            gen_rustler_wrap_return(
                &core_call,
                &method.return_type,
                struct_name,
                opaque_types,
                method.returns_ref,
            )
        }
    } else {
        gen_rustler_unimplemented_body(&method.return_type, &method_fn_name, method.error_type.is_some())
    };
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
    core_import: &str,
) -> String {
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

    let can_delegate = shared::can_auto_delegate(method, opaque_types);

    let body = if can_delegate {
        let call_args = gen_rustler_method_call_args(&method.params, opaque_types);
        let core_call = if is_opaque {
            format!("resource.inner.{}({})", method.name, call_args)
        } else {
            format!(
                "{core_import}::{}::from(obj).{}({})",
                struct_name, method.name, call_args
            )
        };
        let result_wrap = gen_rustler_wrap_return(
            "result",
            &method.return_type,
            struct_name,
            opaque_types,
            method.returns_ref,
        );
        if method.error_type.is_some() {
            format!(
                "let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;\n    \
                 let result = rt.block_on(async {{ {core_call}.await }}).map_err(|e| e.to_string())?;\n    \
                 Ok({result_wrap})"
            )
        } else {
            format!(
                "let rt = tokio::runtime::Runtime::new().unwrap();\n    \
                 let result = rt.block_on(async {{ {core_call}.await }});\n    \
                 {result_wrap}"
            )
        }
    } else {
        gen_rustler_unimplemented_body(&method.return_type, &method_fn_name, method.error_type.is_some())
    };
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
fn gen_rustler_unimplemented_body(return_type: &alef_core::ir::TypeRef, fn_name: &str, has_error: bool) -> String {
    use alef_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(String::from(\"{err_msg}\"))")
    } else {
        match return_type {
            TypeRef::Unit => "()".to_string(),
            TypeRef::String | TypeRef::Char | TypeRef::Path => format!("String::from(\"[unimplemented: {fn_name}]\")"),
            TypeRef::Bytes => "Vec::new()".to_string(),
            TypeRef::Primitive(p) => match p {
                alef_core::ir::PrimitiveType::Bool => "false".to_string(),
                _ => "0".to_string(),
            },
            TypeRef::Optional(_) => "None".to_string(),
            TypeRef::Vec(_) => "Vec::new()".to_string(),
            TypeRef::Map(_, _) => "Default::default()".to_string(),
            TypeRef::Duration => "0u64".to_string(),
            TypeRef::Named(_) | TypeRef::Json => format!("panic!(\"alef: {fn_name} not auto-delegatable\")"),
        }
    }
}

/// Map a return type, wrapping opaque Named types in ResourceArc.
fn map_return_type(ty: &alef_core::ir::TypeRef, mapper: &RustlerMapper, opaque_types: &AHashSet<String>) -> String {
    use alef_core::ir::TypeRef;
    match ty {
        TypeRef::Named(n) if opaque_types.contains(n) => format!("ResourceArc<{n}>"),
        _ => mapper.map_type(ty),
    }
}

/// Generate the rustler::init! macro invocation.
fn gen_nif_init(api: &ApiSurface, config: &AlefConfig) -> String {
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

    // Rustler auto-detects #[rustler::nif] functions; explicit list is deprecated
    let _ = exports; // computed for potential future use
    let module = config
        .elixir
        .as_ref()
        .map(|e| {
            use heck::ToUpperCamelCase;
            format!(
                "Elixir.{}",
                e.app_name.as_deref().unwrap_or("NativeModule").to_upper_camel_case()
            )
        })
        .unwrap_or_else(|| "Elixir.NativeModule".to_string());
    format!("rustler::init!(\"{module}\");")
}
