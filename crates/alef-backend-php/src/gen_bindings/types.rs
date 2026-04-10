use crate::type_map::PhpMapper;
use ahash::AHashSet;
use alef_codegen::builder::ImplBuilder;
use alef_codegen::generators::{self, RustBindingConfig};
use alef_codegen::shared::{constructor_parts, partition_methods};
use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::{EnumDef, TypeDef};

use super::functions::{
    gen_async_instance_method, gen_async_static_method, gen_instance_method, gen_instance_method_non_opaque,
    gen_static_method,
};

/// Generate ext-php-rs methods for an opaque struct (delegates to self.inner).
pub(crate) fn gen_opaque_struct_methods(
    typ: &TypeDef,
    mapper: &PhpMapper,
    opaque_types: &AHashSet<String>,
    core_import: &str,
) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(
                method,
                mapper,
                true,
                &typ.name,
                opaque_types,
            ));
        } else {
            impl_builder.add_method(&gen_instance_method(method, mapper, true, &typ.name, opaque_types));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper, opaque_types));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper, opaque_types, typ, core_import));
        }
    }

    impl_builder.build()
}

/// Generate a PHP struct, adding `serde::Serialize` and `serde::Deserialize` when serde is available.
/// All structs need Deserialize (not just those with Named params) because
/// structs with from_json may reference other structs that also need Deserialize.
/// Serialize is needed for the serde bridge `From<BindingType> for CoreType` used
/// by enum-tainted types (types with enum-Named fields that PHP maps to String).
pub(crate) fn gen_php_struct(typ: &TypeDef, mapper: &PhpMapper, cfg: &RustBindingConfig<'_>) -> String {
    if cfg.has_serde {
        // Build a modified config that also derives Serialize + Deserialize.
        let mut extra_derives: Vec<&str> = cfg.struct_derives.to_vec();
        extra_derives.push("serde::Serialize");
        extra_derives.push("serde::Deserialize");
        let modified_cfg = RustBindingConfig {
            struct_attrs: cfg.struct_attrs,
            field_attrs: cfg.field_attrs,
            struct_derives: &extra_derives,
            method_block_attr: cfg.method_block_attr,
            constructor_attr: cfg.constructor_attr,
            static_attr: cfg.static_attr,
            function_attr: cfg.function_attr,
            enum_attrs: cfg.enum_attrs,
            enum_derives: cfg.enum_derives,
            needs_signature: cfg.needs_signature,
            signature_prefix: cfg.signature_prefix,
            signature_suffix: cfg.signature_suffix,
            core_import: cfg.core_import,
            async_pattern: cfg.async_pattern,
            has_serde: cfg.has_serde,
            type_name_prefix: cfg.type_name_prefix,
        };
        generators::gen_struct(typ, mapper, &modified_cfg)
    } else {
        generators::gen_struct(typ, mapper, cfg)
    }
}

/// Return true if a TypeRef contains a Named type (another struct/class that
/// ext-php-rs cannot deserialize from a PHP value as an owned parameter).
pub(crate) fn type_ref_has_named(ty: &alef_core::ir::TypeRef) -> bool {
    use alef_core::ir::TypeRef;
    match ty {
        TypeRef::Named(_) => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => type_ref_has_named(inner),
        TypeRef::Map(k, v) => type_ref_has_named(k) || type_ref_has_named(v),
        _ => false,
    }
}

/// Generate ext-php-rs methods for a struct.
pub(crate) fn gen_struct_methods(
    typ: &TypeDef,
    mapper: &PhpMapper,
    has_serde: bool,
    core_import: &str,
    opaque_types: &AHashSet<String>,
) -> String {
    let mut impl_builder = ImplBuilder::new(&typ.name);
    impl_builder.add_attr("php_impl");

    if !typ.fields.is_empty() {
        let has_named_params = typ.fields.iter().any(|f| type_ref_has_named(&f.ty));
        if has_named_params {
            if has_serde {
                let constructor = "pub fn from_json(json: String) -> PhpResult<Self> {\n    \
                     serde_json::from_str(&json)\n        \
                     .map_err(|e| PhpException::default(e.to_string()))\n\
                     }"
                .to_string();
                impl_builder.add_method(&constructor);
            } else {
                let constructor = format!(
                    "pub fn __construct() -> PhpResult<Self> {{\n    \
                     Err(PhpException::default(\"Not implemented: constructor for {} requires complex params\".to_string()))\n\
                     }}",
                    typ.name
                );
                impl_builder.add_method(&constructor);
            }
        } else {
            let map_fn = |ty: &alef_core::ir::TypeRef| mapper.map_type(ty);
            if typ.has_default {
                // kwargs-style constructor: all fields optional with defaults
                let config_method = alef_codegen::config_gen::gen_php_kwargs_constructor(typ, &map_fn);
                impl_builder.add_method(&config_method);
            } else {
                // Normal positional constructor
                let (param_list, _, assignments) = constructor_parts(&typ.fields, &map_fn);
                let constructor = format!(
                    "pub fn __construct({param_list}) -> Self {{\n    \
                     Self {{ {assignments} }}\n\
                     }}"
                );
                impl_builder.add_method(&constructor);
            }
        }
    }

    let (instance, statics) = partition_methods(&typ.methods);

    for method in &instance {
        if method.is_async {
            impl_builder.add_method(&gen_async_instance_method(
                method,
                mapper,
                false,
                &typ.name,
                opaque_types,
            ));
        } else {
            impl_builder.add_method(&gen_instance_method_non_opaque(
                method,
                mapper,
                typ,
                core_import,
                opaque_types,
            ));
        }
    }
    for method in &statics {
        if method.is_async {
            impl_builder.add_method(&gen_async_static_method(method, mapper, opaque_types));
        } else {
            impl_builder.add_method(&gen_static_method(method, mapper, opaque_types, typ, core_import));
        }
    }

    impl_builder.build()
}

/// Generate PHP enum constants (enums as string constants).
pub(crate) fn gen_enum_constants(enum_def: &EnumDef) -> String {
    let mut lines = vec![format!("// {} enum values", enum_def.name)];

    for variant in &enum_def.variants {
        let const_name = format!("{}_{}", enum_def.name.to_uppercase(), variant.name.to_uppercase());
        lines.push(format!("pub const {}: &str = \"{}\";", const_name, variant.name));
    }

    lines.join("\n")
}
