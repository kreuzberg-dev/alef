use skif_codegen::builder::RustFileBuilder;
use skif_codegen::generators::{self, AsyncPattern, RustBindingConfig};
use skif_codegen::type_mapper::TypeMapper;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::ApiSurface;
use std::borrow::Cow;
use std::path::PathBuf;

pub struct ExtendrBackend;

impl ExtendrBackend {
    fn binding_config(core_import: &str) -> RustBindingConfig<'_> {
        RustBindingConfig {
            struct_attrs: &[],
            field_attrs: &[],
            struct_derives: &["Clone"],
            method_block_attr: None,
            constructor_attr: "",
            static_attr: None,
            function_attr: "#[extendr]",
            enum_attrs: &[],
            enum_derives: &["Clone", "PartialEq"],
            needs_signature: false,
            signature_prefix: "",
            signature_suffix: "",
            core_import,
            async_pattern: AsyncPattern::None,
        }
    }
}

impl TypeMapper for ExtendrBackend {
    fn primitive(&self, prim: &skif_core::ir::PrimitiveType) -> Cow<'static, str> {
        use skif_core::ir::PrimitiveType;
        match prim {
            PrimitiveType::Bool => Cow::Borrowed("bool"),
            PrimitiveType::U8
            | PrimitiveType::U16
            | PrimitiveType::U32
            | PrimitiveType::I8
            | PrimitiveType::I16
            | PrimitiveType::I32 => Cow::Borrowed("i32"),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => {
                Cow::Borrowed("f64")
            }
            PrimitiveType::F32 | PrimitiveType::F64 => Cow::Borrowed("f64"),
        }
    }

    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}

impl Backend for ExtendrBackend {
    fn name(&self) -> &str {
        "extendr"
    }

    fn language(&self) -> Language {
        Language::R
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
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        let mut builder = RustFileBuilder::new().with_generated_header();
        builder.add_import("extendr_api::prelude::*");
        builder.add_import(&core_import);

        // Generate type bindings
        for typ in &api.types {
            builder.add_item(&generators::gen_struct(typ, self, &cfg));
            let impl_block = generators::gen_impl_block(typ, self, &cfg);
            if !impl_block.is_empty() {
                builder.add_item(&impl_block);
            }
        }

        // Generate enum bindings
        for e in &api.enums {
            builder.add_item(&generators::gen_enum(e, &cfg));
        }

        // Generate function bindings
        for func in &api.functions {
            builder.add_item(&generators::gen_function(func, self, &cfg));
        }

        // Generate adapter functions
        let adapter_blocks = skif_adapters::generate_adapters(config, Language::R)?;
        for block in &adapter_blocks {
            builder.add_item(block);
        }

        // Module registration
        let module_name = config.r_package_name().replace('-', "_");
        let module_items = format!(
            "extendr_module! {{\n    mod {module};\n{types}{funcs}}}\n",
            module = module_name,
            types = api
                .types
                .iter()
                .map(|t| format!("    impl {};\n", t.name))
                .collect::<String>(),
            funcs = api
                .functions
                .iter()
                .map(|f| format!("    fn {};\n", f.name))
                .collect::<String>(),
        );
        builder.add_item(&module_items);

        let output_path = resolve_output_dir(
            config.output.r.as_ref(),
            &config.crate_config.name,
            "packages/r/src/rust/src",
        );

        Ok(vec![GeneratedFile {
            path: PathBuf::from(&output_path).join("lib.rs"),
            content: builder.build(),
            generated_header: false,
        }])
    }
}
