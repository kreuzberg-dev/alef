use crate::type_map::PhpMapper;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder};
use skif_codegen::generators::{self, RustBindingConfig};
use skif_codegen::shared::{constructor_parts, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

pub struct PhpBackend;

impl PhpBackend {
    fn binding_config() -> RustBindingConfig<'static> {
        RustBindingConfig {
            struct_attrs: &["php_class"],
            field_attrs: &[],
            struct_derives: &["Clone"],
            method_block_attr: Some("php_impl"),
            constructor_attr: "",
            static_attr: None,
            function_attr: "#[php_function]",
            enum_attrs: &[],
            enum_derives: &[],
            needs_signature: false,
            signature_prefix: "",
            signature_suffix: "",
        }
    }
}

impl Backend for PhpBackend {
    fn name(&self) -> &str {
        "php"
    }

    fn language(&self) -> Language {
        Language::Php
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
        let mapper = PhpMapper;
        let cfg = Self::binding_config();

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("ext_php_rs::prelude::*");
        builder.add_import("std::collections::HashMap");

        for typ in &api.types {
            if !typ.is_opaque {
                builder.add_item(&generators::gen_struct(typ, &mapper, &cfg));
                builder.add_item(&gen_struct_methods(typ, &mapper));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum_constants(enum_def));
        }

        for func in &api.functions {
            builder.add_item(&gen_function(func, &mapper));
        }

        builder.add_item(&gen_module_init(api, config));

        let content = builder.build();

        let output_dir = resolve_output_dir(
            config.output.php.as_ref(),
            &config.crate_config.name,
            "crates/{name}-php/src/",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_dir).join("lib.rs"),
            content,
            generated_header: false,
        }])
    }
}

/// Generate ext-php-rs methods for a struct.
fn gen_struct_methods(typ: &TypeDef, mapper: &PhpMapper) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    if !typ.fields.is_empty() {
        let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
        let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
        let constructor = format!(
            "pub fn __construct({param_list}) -> Self {{\n    \
             Self {{ {assignments} }}\n\
             }}"
        );
        impl_builder.add_method(&constructor);
    }

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        impl_builder.add_method(&gen_instance_method(method, mapper));
    }
    for method in &statics {
        impl_builder.add_method(&gen_static_method(method, mapper));
    }

    impl_builder.build()
}

/// Generate an instance method binding.
fn gen_instance_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}(&self, {params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n\
         }}",
        method.name
    )
}

/// Generate a static method binding.
fn gen_static_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n\
         }}",
        method.name
    )
}

/// Generate PHP enum constants (enums as string constants).
fn gen_enum_constants(enum_def: &EnumDef) -> String {
    let mut lines = vec![format!("// {} enum values", enum_def.name)];

    for variant in &enum_def.variants {
        let const_name = format!("{}_{}", enum_def.name.to_uppercase(), variant.name.to_uppercase());
        lines.push(format!("pub const {}: &str = \"{}\";", const_name, variant.name));
    }

    lines.join("\n")
}

/// Generate a free function binding.
fn gen_function(func: &FunctionDef, mapper: &PhpMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    format!(
        "#[php_function]\npub fn {}({params}) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n\
         }}",
        func.name
    )
}

/// Generate the module initialization function.
fn gen_module_init(api: &ApiSurface, _config: &SkifConfig) -> String {
    let mut lines = vec![
        "#[php_module]".to_string(),
        "pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {".to_string(),
        "    let module = module".to_string(),
    ];

    for typ in &api.types {
        if !typ.is_opaque {
            lines.push(format!("        .add_class::<{}>()", typ.name));
        }
    }
    for func in &api.functions {
        lines.push(format!("        .add_function({})", func.name));
    }

    lines.push("        .build();".to_string());
    lines.push("    module".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}
