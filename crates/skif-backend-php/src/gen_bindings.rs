use crate::type_map::PhpMapper;
use ahash::AHashSet;
use skif_codegen::builder::{ImplBuilder, RustFileBuilder};
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_codegen::shared::{constructor_parts, function_params, partition_methods};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FunctionDef, MethodDef, TypeDef};
use std::path::PathBuf;

pub struct PhpBackend;

impl PhpBackend {
    fn binding_config(core_import: &str) -> RustBindingConfig<'_> {
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
            core_import,
            async_pattern: AsyncPattern::TokioBlockOn,
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
            supports_async: true,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let mapper = PhpMapper;
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("ext_php_rs::prelude::*");
        builder.add_import("std::collections::HashMap");
        builder.add_import(&core_import);

        // Check if any function or method is async
        let has_async =
            api.functions.iter().any(|f| f.is_async) || api.types.iter().any(|t| t.methods.iter().any(|m| m.is_async));

        if has_async {
            builder.add_item(&gen_tokio_runtime());
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
                builder.add_item(&generators::gen_opaque_struct(typ, &cfg));
                builder.add_item(&gen_opaque_struct_methods(typ, &mapper, &opaque_types));
            } else {
                builder.add_item(&generators::gen_struct(typ, &mapper, &cfg));
                builder.add_item(&gen_struct_methods(typ, &mapper));
            }
        }

        for enum_def in &api.enums {
            builder.add_item(&gen_enum_constants(enum_def));
        }

        for func in &api.functions {
            if func.is_async {
                builder.add_item(&gen_async_function(func, &mapper));
            } else {
                builder.add_item(&gen_function(func, &mapper));
            }
        }

        let convertible = skif_codegen::conversions::convertible_types(api);
        // From/Into conversions
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible) {
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

/// Generate ext-php-rs methods for an opaque struct (delegates to self.inner).
fn gen_opaque_struct_methods(typ: &TypeDef, mapper: &PhpMapper, _opaque_types: &AHashSet<String>) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(method, mapper));
        } else {
            impl_builder.add_method(&gen_instance_method(method, mapper));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper));
        }
    }

    impl_builder.build()
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
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(method, mapper));
        } else {
            impl_builder.add_method(&gen_instance_method(method, mapper));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper));
        }
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
         todo!(\"call into core\")\n\
         }}",
        func.name
    )
}

/// Generate an async free function binding for PHP (block on runtime).
fn gen_async_function(func: &FunctionDef, mapper: &PhpMapper) -> String {
    let params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    // Append "_async" to the function name for PHP (since it's not truly async)
    format!(
        "#[php_function]\npub fn {}_async({params}) -> {return_annotation} {{\n    \
         WORKER_RUNTIME.block_on(async {{\n        \
         todo!(\"call into core\")\n    \
         }}).map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))\n\
         }}",
        func.name
    )
}

/// Generate an async instance method binding for PHP (block on runtime).
fn gen_async_instance_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    // Append "_async" to the method name for PHP
    format!(
        "pub fn {}_async(&self, {params}) -> {return_annotation} {{\n    \
         WORKER_RUNTIME.block_on(async {{\n        \
         todo!(\"call into core\")\n    \
         }}).map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))\n\
         }}",
        method.name
    )
}

/// Generate an async static method binding for PHP (block on runtime).
fn gen_async_static_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    // Append "_async" to the method name for PHP
    format!(
        "pub fn {}_async({params}) -> {return_annotation} {{\n    \
         WORKER_RUNTIME.block_on(async {{\n        \
         todo!(\"call into core\")\n    \
         }}).map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()))\n\
         }}",
        method.name
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
        lines.push(format!("        .add_class::<{}>()", typ.name));
    }
    for func in &api.functions {
        let func_name = if func.is_async {
            format!("{}_async", func.name)
        } else {
            func.name.clone()
        };
        lines.push(format!("        .add_function({})", func_name));
    }

    lines.push("        .build();".to_string());
    lines.push("    module".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

/// Generate a global Tokio runtime for PHP async support.
fn gen_tokio_runtime() -> String {
    "static WORKER_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect(\"Failed to create Tokio runtime\")
});"
    .to_string()
}
