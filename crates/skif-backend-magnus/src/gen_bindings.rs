use crate::type_map::MagnusMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::shared::{constructor_parts, function_params};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef, TypeRef};
use std::fmt::Write;
use std::path::PathBuf;

pub struct MagnusBackend;

/// Names that conflict with magnus imports or generated code.
/// `Error` conflicts with `magnus::Error`, `init` conflicts with `#[magnus::init]`.
const MAGNUS_RESERVED_ENUM_NAMES: &[&str] = &["Error"];
const MAGNUS_RESERVED_FN_NAMES: &[&str] = &["init"];

fn is_reserved_enum(name: &str) -> bool {
    MAGNUS_RESERVED_ENUM_NAMES.contains(&name)
}

fn is_reserved_fn(name: &str) -> bool {
    MAGNUS_RESERVED_FN_NAMES.contains(&name)
}

impl Backend for MagnusBackend {
    fn name(&self) -> &str {
        "magnus"
    }

    fn language(&self) -> Language {
        Language::Ruby
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
        let mapper = MagnusMapper;
        let core_import = config.core_import();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import(
            "magnus::{function, method, prelude::*, Error, Ruby, IntoValueFromNative, try_convert::TryConvertOwned}",
        );

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
        let custom_mods = config.custom_modules.for_language(Language::Ruby);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
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

        for typ in &api.types {
            if typ.is_opaque {
                builder.add_item(&gen_opaque_struct(typ, &core_import));
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &opaque_types));
            } else {
                builder.add_item(&gen_struct(typ, &mapper));
                builder.add_item(&gen_struct_methods(typ, &mapper));
            }
        }

        for enum_def in &api.enums {
            if !is_reserved_enum(&enum_def.name) {
                builder.add_item(&gen_enum(enum_def));
            }
        }

        for func in &api.functions {
            if !is_reserved_fn(&func.name) {
                builder.add_item(&gen_function(func, &mapper));
                if func.is_async {
                    builder.add_item(&gen_async_function(func, &mapper));
                }
            }
        }

        // Magnus backend: skip From/Into conversions entirely.
        // In Magnus, binding types ARE the core types (no Js/Py prefix wrapper),
        // so generating From<T> for T is nonsensical and violates orphan rules.

        // Build adapter body map (consumed by generators via body substitution)
        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Ruby)?;

        let module_name = get_module_name(&api.crate_name);
        builder.add_item(&gen_module_init(&module_name, api, config));

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.ruby.as_ref(),
            &config.crate_config.name,
            "packages/ruby/ext/{name}_rb/native/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }

    fn generate_type_stubs(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let stubs_config = match config.ruby.as_ref().and_then(|c| c.stubs.as_ref()) {
            Some(s) => s,
            None => return Ok(vec![]),
        };

        let content = crate::gen_stubs::gen_stubs(api);

        let stubs_path = resolve_output_dir(
            Some(&stubs_config.output),
            &config.crate_config.name,
            stubs_config.output.to_string_lossy().as_ref(),
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&stubs_path).join("types.rbs"),
            content,
            generated_header: true,
        }])
    }
}

/// Convert crate name to PascalCase module name.
fn get_module_name(crate_name: &str) -> String {
    crate_name
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Generate an opaque Magnus-wrapped struct with inner Arc.
fn gen_opaque_struct(typ: &TypeDef, core_import: &str) -> String {
    let module_name = "Kreuzberg";
    let class_path = format!("{}::{}", module_name, typ.name);

    let mut out = String::with_capacity(256);
    writeln!(out, "#[derive(Clone)]").ok();
    writeln!(out, r#"#[magnus::wrap(class = "{}")]"#, class_path).ok();
    writeln!(out, "pub struct {} {{", typ.name).ok();
    writeln!(out, "    inner: Arc<{}::{}>,", core_import, typ.name).ok();
    writeln!(out, "}}").ok();
    let name = &typ.name;
    writeln!(out).ok();
    // SAFETY: #[magnus::wrap] already provides IntoValue. This marker trait
    // enables use in Vec<T> returns from Magnus function!/method! macros.
    writeln!(out, "unsafe impl IntoValueFromNative for {name} {{}}").ok();
    // Magnus only provides TryConvert for &T (references) on TypedData types.
    // We need TryConvert for owned T so wrapped types can be used as function parameters.
    writeln!(out, "\nimpl magnus::TryConvert for {name} {{").ok();
    writeln!(
        out,
        "    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {{"
    )
    .ok();
    writeln!(out, "        let r: &{name} = magnus::TryConvert::try_convert(val)?;").ok();
    writeln!(out, "        Ok(r.clone())").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();
    // SAFETY: TryConvert produces an owned value via Clone, satisfying owned conversion.
    write!(out, "unsafe impl TryConvertOwned for {name} {{}}").ok();
    out
}

/// Generate Magnus methods for an opaque struct (delegates to self.inner).
fn gen_opaque_struct_methods(typ: &TypeDef, mapper: &MagnusMapper, _opaque_types: &AHashSet<String>) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);

    for method in &typ.methods {
        if !method.is_static {
            if method.is_async {
                impl_builder.add_method(&gen_opaque_async_instance_method(method, mapper));
            } else {
                impl_builder.add_method(&gen_opaque_instance_method(method, mapper));
            }
        }
    }

    impl_builder.build()
}

/// Generate an opaque sync instance method for Magnus (delegates to self.inner).
fn gen_opaque_instance_method(method: &MethodDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_magnus_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some());
    format!(
        "fn {}(&self, {params}) -> {return_annotation} {{\n        \
         {body}\n    }}",
        method.name
    )
}

/// Generate an opaque async instance method for Magnus (block on runtime, delegates to self.inner).
fn gen_opaque_async_instance_method(method: &MethodDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_magnus_unimplemented_body(
        &method.return_type,
        &format!("{}_async", method.name),
        method.error_type.is_some(),
    );
    format!(
        "fn {}_async(&self, {params}) -> {return_annotation} {{\n        \
         {body}\n    \
         }}",
        method.name
    )
}

/// Generate a Magnus-wrapped struct definition using the shared TypeMapper.
fn gen_struct(typ: &TypeDef, mapper: &MagnusMapper) -> String {
    let module_name = "Kreuzberg";
    let class_path = format!("{}::{}", module_name, typ.name);

    let mut struct_builder = StructBuilder::new(&typ.name);
    struct_builder.add_attr(&format!(r#"magnus::wrap(class = "{}")"#, class_path));

    // Magnus requires Clone for TryConvert on owned types
    struct_builder.add_derive("Clone");

    for field in &typ.fields {
        let field_type = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        struct_builder.add_field(&field.name, &field_type, vec![]);
    }

    let mut out = struct_builder.build();
    let name = &typ.name;
    // SAFETY: #[magnus::wrap] already provides IntoValue. This marker trait
    // enables use in Vec<T> returns from Magnus function!/method! macros.
    writeln!(out, "\n\nunsafe impl IntoValueFromNative for {name} {{}}").ok();
    // Magnus only provides TryConvert for &T (references) on TypedData types.
    // We need TryConvert for owned T so wrapped types can be used as function parameters.
    writeln!(out, "\nimpl magnus::TryConvert for {name} {{").ok();
    writeln!(
        out,
        "    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {{"
    )
    .ok();
    writeln!(out, "        let r: &{name} = magnus::TryConvert::try_convert(val)?;").ok();
    writeln!(out, "        Ok(r.clone())").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();
    // SAFETY: TryConvert produces an owned value via Clone, satisfying owned conversion.
    write!(out, "unsafe impl TryConvertOwned for {name} {{}}").ok();
    out
}

/// Generate Magnus methods for a struct.
fn gen_struct_methods(typ: &TypeDef, mapper: &MagnusMapper) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);

    if !typ.fields.is_empty() {
        let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
        let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
        let new_method = format!("fn new({param_list}) -> Self {{\n        Self {{ {assignments} }}\n    }}");
        impl_builder.add_method(&new_method);
    }

    for field in &typ.fields {
        impl_builder.add_method(&gen_field_accessor(field, mapper));
    }

    for method in &typ.methods {
        if !method.is_static {
            if method.is_async {
                impl_builder.add_method(&gen_async_instance_method(method, mapper));
            } else {
                impl_builder.add_method(&gen_instance_method(method, mapper));
            }
        }
    }

    impl_builder.build()
}

/// Generate a field accessor method.
fn gen_field_accessor(field: &FieldDef, mapper: &MagnusMapper) -> String {
    let return_type = if field.optional {
        mapper.optional(&mapper.map_type(&field.ty))
    } else {
        mapper.map_type(&field.ty)
    };

    let body = if is_primitive_copy(&field.ty) {
        format!("self.{}", field.name)
    } else {
        format!("self.{}.clone()", field.name)
    };

    format!(
        "fn {}(&self) -> {} {{\n        {}\n    }}",
        field.name, return_type, body
    )
}

/// Check if a type is a Copy type (primitives and unit).
fn is_primitive_copy(ty: &skif_core::ir::TypeRef) -> bool {
    matches!(ty, skif_core::ir::TypeRef::Primitive(_) | skif_core::ir::TypeRef::Unit)
}

/// Generate an instance method binding.
fn gen_instance_method(method: &MethodDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_magnus_unimplemented_body(&method.return_type, &method.name, method.error_type.is_some());
    format!(
        "fn {}(&self, {params}) -> {return_annotation} {{\n        \
         {body}\n    }}",
        method.name
    )
}

/// Generate an async instance method binding for Magnus (block on runtime).
fn gen_async_instance_method(method: &MethodDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    let body = gen_magnus_unimplemented_body(
        &method.return_type,
        &format!("{}_async", method.name),
        method.error_type.is_some(),
    );
    // Append "_async" to the method name for Ruby
    format!(
        "fn {}_async(&self, {params}) -> {return_annotation} {{\n        \
         {body}\n    \
         }}",
        method.name
    )
}

/// Convert a PascalCase name to snake_case for Ruby symbol mapping.
fn pascal_to_snake(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

/// Generate a Magnus enum definition with IntoValue and TryConvert impls.
/// Unit-variant enums are represented as Ruby Symbols for ergonomic Ruby usage.
fn gen_enum(enum_def: &EnumDef) -> String {
    let name = &enum_def.name;
    let mut out = String::with_capacity(512);

    // Enum definition
    writeln!(out, "#[derive(Clone, Copy, PartialEq, Eq, Debug)]").ok();
    writeln!(out, "pub enum {name} {{").ok();
    for variant in &enum_def.variants {
        writeln!(out, "    {},", variant.name).ok();
    }
    writeln!(out, "}}").ok();
    writeln!(out).ok();

    // IntoValue: convert enum variant to Ruby Symbol
    writeln!(out, "impl magnus::IntoValue for {name} {{").ok();
    writeln!(out, "    fn into_value_with(self, handle: &Ruby) -> magnus::Value {{").ok();
    writeln!(out, "        let sym = match self {{").ok();
    for variant in &enum_def.variants {
        let snake = pascal_to_snake(&variant.name);
        writeln!(out, "            {name}::{} => \"{snake}\",", variant.name).ok();
    }
    writeln!(out, "        }};").ok();
    writeln!(out, "        handle.to_symbol(sym).into_value_with(handle)").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();

    // TryConvert: convert Ruby Symbol/String to enum variant
    writeln!(out, "impl magnus::TryConvert for {name} {{").ok();
    writeln!(
        out,
        "    fn try_convert(val: magnus::Value) -> Result<Self, magnus::Error> {{"
    )
    .ok();
    writeln!(out, "        let s: String = magnus::TryConvert::try_convert(val)?;").ok();
    writeln!(out, "        match s.as_str() {{").ok();
    for variant in &enum_def.variants {
        let snake = pascal_to_snake(&variant.name);
        writeln!(out, "            \"{snake}\" => Ok({name}::{}),", variant.name).ok();
    }
    writeln!(out, "            other => Err(magnus::Error::new(").ok();
    writeln!(
        out,
        "                unsafe {{ Ruby::get_unchecked() }}.exception_arg_error(),"
    )
    .ok();
    writeln!(out, "                format!(\"invalid {name} value: {{other}}\"),").ok();
    writeln!(out, "            )),").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "}}").ok();
    writeln!(out).ok();
    // SAFETY: IntoValue is implemented above. This marker trait enables use
    // in Vec<T> returns from Magnus function!/method! macros.
    writeln!(out, "unsafe impl IntoValueFromNative for {name} {{}}").ok();
    // SAFETY: TryConvert produces an owned value, satisfying owned conversion.
    write!(out, "unsafe impl TryConvertOwned for {name} {{}}").ok();

    out
}

/// Generate a free function binding.
fn gen_function(func: &FunctionDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let body = gen_magnus_unimplemented_body(&func.return_type, &func.name, func.error_type.is_some());
    format!(
        "fn {}({params}) -> {return_annotation} {{\n    \
         {body}\n}}",
        func.name
    )
}

/// Generate an async free function binding for Magnus (block on runtime).
fn gen_async_function(func: &FunctionDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    let body = gen_magnus_unimplemented_body(
        &func.return_type,
        &format!("{}_async", func.name),
        func.error_type.is_some(),
    );
    // Append "_async" to the function name for Ruby
    format!(
        "fn {}_async({params}) -> {return_annotation} {{\n    \
         {body}\n\
         }}",
        func.name
    )
}

/// Generate a type-appropriate unimplemented body for Magnus (no todo!()).
fn gen_magnus_unimplemented_body(return_type: &skif_core::ir::TypeRef, fn_name: &str, has_error: bool) -> String {
    use skif_core::ir::TypeRef;
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error {
        format!("Err(magnus::Error::new(magnus::exception::runtime_error(), \"{err_msg}\"))")
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

/// Generate the module initialization function.
fn gen_module_init(module_name: &str, api: &ApiSurface, config: &SkifConfig) -> String {
    let mut lines = vec![
        "#[magnus::init]".to_string(),
        "fn init(ruby: &Ruby) -> Result<(), Error> {".to_string(),
        format!(r#"    let module = ruby.define_module("{}")?;"#, module_name),
        "".to_string(),
    ];

    // Custom registrations (before generated ones)
    if let Some(reg) = config.custom_registrations.for_language(Language::Ruby) {
        for class in &reg.classes {
            lines.push(format!(
                r#"    let class = module.define_class("{class}", ruby.class_object())?;"#
            ));
        }
        for func in &reg.functions {
            lines.push(format!(
                r#"    module.define_module_function("{func}", function!({func}, 0))?;"#
            ));
        }
        lines.push("".to_string());
    }

    for typ in &api.types {
        lines.push(format!(
            r#"    let class = module.define_class("{}", ruby.class_object())?;"#,
            typ.name
        ));

        if !typ.is_opaque && !typ.fields.is_empty() {
            let arg_count = typ.fields.len();
            lines.push(format!(
                r#"    class.define_singleton_method("new", function!({name}::new, {count}))?;"#,
                name = typ.name,
                count = arg_count
            ));
        }

        if !typ.is_opaque {
            for field in &typ.fields {
                lines.push(format!(
                    r#"    class.define_method("{name}", method!({typ_name}::{name}, 0))?;"#,
                    name = field.name,
                    typ_name = typ.name
                ));
            }
        }

        for method in &typ.methods {
            if !method.is_static {
                let method_name = if method.is_async {
                    format!("{}_async", method.name)
                } else {
                    method.name.clone()
                };
                let param_count = method.params.len();
                lines.push(format!(
                    r#"    class.define_method("{name}", method!({typ_name}::{fn_name}, {count}))?;"#,
                    name = method_name,
                    typ_name = typ.name,
                    fn_name = method_name,
                    count = param_count
                ));
            }
        }

        lines.push("".to_string());
    }

    for func in &api.functions {
        if is_reserved_fn(&func.name) {
            continue;
        }
        let func_name = if func.is_async {
            format!("{}_async", func.name)
        } else {
            func.name.clone()
        };
        let param_count = func.params.len();
        lines.push(format!(
            r#"    module.define_module_function("{name}", function!({fn_name}, {count}))?;"#,
            name = func_name,
            fn_name = func_name,
            count = param_count
        ));
    }

    lines.push("".to_string());
    lines.push("    Ok(())".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}
