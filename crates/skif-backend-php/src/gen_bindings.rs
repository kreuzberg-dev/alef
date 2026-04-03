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
            has_serde: true,
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
        let enum_names = api.enums.iter().map(|e| e.name.clone()).collect();
        let mapper = PhpMapper { enum_names };
        let core_import = config.core_import();
        let cfg = Self::binding_config(&core_import);

        // Build the inner module content (types, methods, conversions)
        let mut builder = RustFileBuilder::new();
        builder.add_import("ext_php_rs::prelude::*");
        builder.add_import("std::collections::HashMap");
        builder.add_import(&core_import);

        // Custom module declarations
        let custom_mods = config.custom_modules.for_language(Language::Php);
        for module in custom_mods {
            builder.add_item(&format!("pub mod {module};"));
        }

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
        // From/Into conversions with PHP-specific i64 casts.
        // Skip types with enum Named fields: PhpMapper maps enum Named→String, so the
        // binding stores a String where the core has an enum. Auto-generated .into()
        // calls would require From<String> for the core enum which doesn't exist.
        let enum_names_ref = &mapper.enum_names;
        for typ in &api.types {
            if skif_codegen::conversions::can_generate_conversion(typ, &convertible)
                && !has_enum_named_field(typ, enum_names_ref)
            {
                builder.add_item(&gen_php_from_binding_to_core(typ, &core_import));
                builder.add_item(&gen_php_from_core_to_binding(typ, &core_import));
            }
        }

        let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Php)?;

        // Add feature gate as inner attribute — entire crate is gated
        let php_config = config.php.as_ref();
        if let Some(feature_name) = php_config.and_then(|c| c.feature_gate.as_deref()) {
            builder.add_inner_attribute(&format!("cfg(feature = \"{feature_name}\")"));
            builder.add_inner_attribute(&format!(
                "cfg_attr(all(windows, target_env = \"msvc\", feature = \"{feature_name}\"), feature(abi_vectorcall))"
            ));
        }

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

/// Return true if a TypeRef contains a Named type (another struct/class that
/// ext-php-rs cannot deserialize from a PHP value as an owned parameter).
fn type_ref_has_named(ty: &skif_core::ir::TypeRef) -> bool {
    use skif_core::ir::TypeRef;
    match ty {
        TypeRef::Named(_) => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_named(inner),
        TypeRef::Map(k, v) => type_ref_has_named(k) || type_ref_has_named(v),
        _ => false,
    }
}

/// Generate ext-php-rs methods for a struct.
fn gen_struct_methods(typ: &TypeDef, mapper: &PhpMapper) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    if !typ.fields.is_empty() {
        let has_named_params = typ.fields.iter().any(|f| type_ref_has_named(&f.ty));
        if has_named_params {
            // ext-php-rs cannot convert PHP values into #[php_class] structs as owned
            // constructor parameters (FromZvalMut not satisfied). Generate a todo!()
            // constructor to keep the class visible in PHP but defer implementation.
            let constructor = format!(
                "pub fn __construct() -> Self {{\n    \
                 todo!(\"constructor for {} requires complex params — implement manually\")\n\
                 }}",
                typ.name
            );
            impl_builder.add_method(&constructor);
        } else {
            let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
            let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
            let constructor = format!(
                "pub fn __construct({param_list}) -> Self {{\n    \
                 Self {{ {assignments} }}\n\
                 }}"
            );
            impl_builder.add_method(&constructor);
        }
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
    let _params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}(&self) -> {return_annotation} {{\n    \
         todo!(\"call into core implementation\")\n\
         }}",
        method.name
    )
}

/// Generate a static method binding.
fn gen_static_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let _params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}() -> {return_annotation} {{\n    \
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
    let _params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    format!(
        "#[php_function]\npub fn {}() -> {return_annotation} {{\n    \
         todo!(\"call into core\")\n\
         }}",
        func.name
    )
}

/// Generate an async free function binding for PHP (block on runtime).
fn gen_async_function(func: &FunctionDef, mapper: &PhpMapper) -> String {
    let _params = function_params(&func.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&func.return_type);
    let return_annotation = mapper.wrap_return(&return_type, func.error_type.is_some());

    format!(
        "#[php_function]\npub fn {}_async() -> {return_annotation} {{\n    \
         todo!(\"wire up {}_async\")\n\
         }}",
        func.name, func.name
    )
}

/// Generate an async instance method binding for PHP (block on runtime).
fn gen_async_instance_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let _params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}_async(&self) -> {return_annotation} {{\n    \
         todo!(\"wire up {}_async\")\n\
         }}",
        method.name, method.name
    )
}

/// Generate an async static method binding for PHP (block on runtime).
fn gen_async_static_method(method: &MethodDef, mapper: &PhpMapper) -> String {
    let _params = function_params(&method.params, &|ty| mapper.map_type(ty));
    let return_type = mapper.map_type(&method.return_type);
    let return_annotation = mapper.wrap_return(&return_type, method.error_type.is_some());

    format!(
        "pub fn {}_async() -> {return_annotation} {{\n    \
         todo!(\"wire up {}_async\")\n\
         }}",
        method.name, method.name
    )
}

/// Generate `impl From<Type> for core::Type` with PHP-specific i64 casts.
fn gen_php_from_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", typ.name, typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = php_field_conversion(&field.name, &field.ty, field.optional, "val", true);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Type> for Type` with PHP-specific i64 casts.
fn gen_php_from_core_to_binding(typ: &TypeDef, core_import: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", typ.name, typ.name).ok();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = php_field_conversion(&field.name, &field.ty, field.optional, "val", false);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// PHP-specific field conversion that handles U64/Usize→i64 type casts.
fn php_field_conversion(name: &str, ty: &skif_core::ir::TypeRef, optional: bool, val: &str, to_core: bool) -> String {
    use skif_core::ir::{PrimitiveType, TypeRef};
    match ty {
        TypeRef::Primitive(p) if matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize) => {
            let cast_to = if to_core {
                match p {
                    PrimitiveType::U64 => "u64",
                    PrimitiveType::Usize => "usize",
                    PrimitiveType::Isize => "isize",
                    _ => unreachable!(),
                }
            } else {
                "i64"
            };
            format!("{name}: {val}.{name} as {cast_to}")
        }
        TypeRef::Named(_) => {
            if optional {
                format!("{name}: {val}.{name}.map(Into::into)")
            } else {
                format!("{name}: {val}.{name}.into()")
            }
        }
        // Path: binding uses String (i64 for PHP), core uses PathBuf
        TypeRef::Path => {
            if to_core {
                if optional {
                    format!("{name}: {val}.{name}.map(Into::into)")
                } else {
                    format!("{name}: {val}.{name}.into()")
                }
            } else if optional {
                format!("{name}: {val}.{name}.map(|p| p.to_string_lossy().to_string())")
            } else {
                format!("{name}: {val}.{name}.to_string_lossy().to_string()")
            }
        }
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Primitive(p) if matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize) => {
                let cast_to = if to_core {
                    match p {
                        PrimitiveType::U64 => "u64",
                        PrimitiveType::Usize => "usize",
                        PrimitiveType::Isize => "isize",
                        _ => unreachable!(),
                    }
                } else {
                    "i64"
                };
                format!("{name}: {val}.{name}.map(|v| v as {cast_to})")
            }
            TypeRef::Named(_) | TypeRef::Path => {
                if to_core {
                    format!("{name}: {val}.{name}.map(Into::into)")
                } else {
                    format!("{name}: {val}.{name}.map(|p| p.to_string_lossy().to_string())")
                }
            }
            _ => format!("{name}: {val}.{name}"),
        },
        _ => format!("{name}: {val}.{name}"),
    }
}

/// Return true if any field of the type (recursively through Optional/Vec) is a Named type
/// that is an enum. PHP maps enum Named types to String, so From/Into impls would need
/// From<String> for the core enum which doesn't exist — skip generation for such types.
fn has_enum_named_field(typ: &skif_core::ir::TypeDef, enum_names: &AHashSet<String>) -> bool {
    fn type_ref_has_enum_named(ty: &skif_core::ir::TypeRef, enum_names: &AHashSet<String>) -> bool {
        use skif_core::ir::TypeRef;
        match ty {
            TypeRef::Named(name) => enum_names.contains(name.as_str()),
            TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_enum_named(inner, enum_names),
            TypeRef::Map(k, v) => type_ref_has_enum_named(k, enum_names) || type_ref_has_enum_named(v, enum_names),
            _ => false,
        }
    }
    typ.fields.iter().any(|f| type_ref_has_enum_named(&f.ty, enum_names))
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
