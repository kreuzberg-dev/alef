use crate::type_map::MagnusMapper;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder, StructBuilder};
use skif_codegen::shared::{constructor_parts, function_params};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

pub struct MagnusBackend;

impl Backend for MagnusBackend {
    fn name(&self) -> &str {
        "magnus"
    }

    fn language(&self) -> Language {
        Language::Ruby
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: false,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let mapper = MagnusMapper;

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("magnus::{function, method, prelude::*, Error, Ruby}");
        builder.add_import("std::collections::HashMap");

        for typ in &api.types {
            if !typ.is_opaque {
                builder.add_item(&gen_struct(typ, &mapper));
                builder.add_item(&gen_struct_methods(typ, &mapper));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum(enum_def));
        }

        for func in &api.functions {
            builder.add_item(&gen_function(func, &mapper));
        }

        let module_name = get_module_name(&api.crate_name);
        builder.add_item(&gen_module_init(&module_name, api));

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
            path: PathBuf::from(stubs_path),
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

/// Generate a Magnus-wrapped struct definition using the shared TypeMapper.
fn gen_struct(typ: &TypeDef, mapper: &MagnusMapper) -> String {
    let module_name = "Kreuzberg";
    let class_path = format!("{}::{}", module_name, typ.name);

    let mut struct_builder = StructBuilder::new(&typ.name);
    struct_builder.add_attr(&format!(r#"magnus::wrap(class = "{}")"#, class_path));

    if typ.is_clone {
        struct_builder.add_derive("Clone");
    }

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
        if !method.is_static && !method.is_async {
            impl_builder.add_method(&gen_instance_method(method, mapper));
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

    format!(
        "fn {}(&self) -> {} {{\n        self.{}.clone()\n    }}",
        field.name, return_type, field.name
    )
}

/// Generate an instance method binding.
fn gen_instance_method(method: &MethodDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "fn {}(&self, {params}) -> {return_annotation} {{\n        \
         todo!(\"call into core implementation\")\n    }}",
        method.name
    )
}

/// Generate a Magnus enum definition.
fn gen_enum(enum_def: &EnumDef) -> String {
    let mut lines = vec![
        "#[derive(Clone, Copy, PartialEq, Eq)]".to_string(),
        format!("pub enum {} {{", enum_def.name),
    ];

    for variant in &enum_def.variants {
        lines.push(format!("    {},", variant.name));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Generate a free function binding.
fn gen_function(func: &FunctionDef, mapper: &MagnusMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    format!(
        "fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n}}",
        func.name
    )
}

/// Generate the module initialization function.
fn gen_module_init(module_name: &str, api: &ApiSurface) -> String {
    let mut lines = vec![
        "#[magnus::init]".to_string(),
        "fn init(ruby: &Ruby) -> Result<(), Error> {".to_string(),
        format!(r#"    let module = ruby.define_module("{}")?;"#, module_name),
        "".to_string(),
    ];

    for typ in &api.types {
        if !typ.is_opaque {
            lines.push(format!(
                r#"    let class = module.define_class("{}", ruby.class_object())?;"#,
                typ.name
            ));

            if !typ.fields.is_empty() {
                let arg_count = typ.fields.len();
                lines.push(format!(
                    r#"    class.define_singleton_method("new", function!({name}::new, {count}))?;"#,
                    name = typ.name,
                    count = arg_count
                ));
            }

            for field in &typ.fields {
                lines.push(format!(
                    r#"    class.define_method("{name}", method!({typ_name}::{name}, 0))?;"#,
                    name = field.name,
                    typ_name = typ.name
                ));
            }

            for method in &typ.methods {
                if !method.is_static && !method.is_async {
                    let param_count = method.params.len();
                    lines.push(format!(
                        r#"    class.define_method("{name}", method!({typ_name}::{name}, {count}))?;"#,
                        name = method.name,
                        typ_name = typ.name,
                        count = param_count
                    ));
                }
            }

            lines.push("".to_string());
        }
    }

    for func in &api.functions {
        if !func.is_async {
            let param_count = func.params.len();
            lines.push(format!(
                r#"    module.define_module_function("{name}", function!({name}, {count}))?;"#,
                name = func.name,
                count = param_count
            ));
        }
    }

    lines.push("".to_string());
    lines.push("    Ok(())".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}
