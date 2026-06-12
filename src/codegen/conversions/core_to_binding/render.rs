use crate::codegen::conversions::ConversionConfig;
use crate::codegen::conversions::helpers::{core_type_path_remapped, field_references_excluded_type, is_newtype};
use crate::core::ir::{CoreWrapper, TypeDef, TypeRef};
use ahash::AHashSet;

use super::fields::field_conversion_from_core_cfg;
use super::wrappers::apply_core_wrapper_from_core;

/// Generate `impl From<core::Type> for BindingType` (core -> binding).
pub fn gen_from_core_to_binding(typ: &TypeDef, core_import: &str, opaque_types: &AHashSet<String>) -> String {
    gen_from_core_to_binding_cfg(typ, core_import, opaque_types, &ConversionConfig::default())
}

/// Generate `impl From<core::Type> for BindingType` with backend-specific config.
pub fn gen_from_core_to_binding_cfg(
    typ: &TypeDef,
    core_import: &str,
    opaque_types: &AHashSet<String>,
    config: &ConversionConfig,
) -> String {
    let core_path = core_type_path_remapped(typ, core_import, config.source_crate_remaps);
    let binding_name = format!("{}{}", config.type_name_prefix, typ.name);

    // Newtype structs: extract inner value with val.0
    if is_newtype(typ) {
        let field = &typ.fields[0];
        let newtype_inner_expr = match &field.ty {
            TypeRef::Named(_) => "val.0.into()".to_string(),
            TypeRef::Path => "val.0.to_string_lossy().to_string()".to_string(),
            TypeRef::Duration => "val.0.as_millis() as u64".to_string(),
            _ => "val.0".to_string(),
        };
        return crate::codegen::template_env::render(
            "conversions/core_to_binding_impl",
            minijinja::context! {
                core_path => core_path,
                binding_name => binding_name,
                has_lifetime_params => typ.has_lifetime_params,
                is_newtype => true,
                newtype_inner_expr => newtype_inner_expr,
                fields => vec![] as Vec<String>,
            },
        );
    }

    let optionalized = config.optionalize_defaults && typ.has_default;

    // Pre-compute all field conversions
    let mut fields = Vec::new();
    for field in &typ.fields {
        if field.binding_excluded {
            continue;
        }
        // Fields referencing excluded types are not present in the binding struct — skip
        if !config.exclude_types.is_empty() && field_references_excluded_type(&field.ty, config.exclude_types) {
            continue;
        }
        // When the binding crate strips cfg-gated fields from the struct
        // (typically because the backend doesn't carry feature gates into the binding
        // crate's Cargo.toml — e.g. extendr), the From impl cannot assign
        // <field>: val.<field> because the binding struct has no slot for it.
        if field.cfg.is_some()
            && !config.never_skip_cfg_field_names.contains(&field.name)
            && config.strip_cfg_fields_from_binding_struct
        {
            continue;
        }
        let base_conversion = field_conversion_from_core_cfg(
            &field.name,
            &field.ty,
            field.optional,
            field.sanitized,
            opaque_types,
            config,
        );
        // Box<T> fields: dereference before conversion.
        let base_conversion = if field.is_boxed && matches!(&field.ty, TypeRef::Named(_)) {
            if field.optional {
                // Optional<Box<T>>: replace .map(Into::into) with .map(|v| (*v).into())
                let src = format!("{}: val.{}.map(Into::into)", field.name, field.name);
                let dst = format!("{}: val.{}.map(|v| (*v).into())", field.name, field.name);
                if base_conversion == src { dst } else { base_conversion }
            } else {
                // Box<T>: replace `val.{name}` with `(*val.{name})`
                base_conversion.replace(&format!("val.{}", field.name), &format!("(*val.{})", field.name))
            }
        } else {
            base_conversion
        };
        // Newtype unwrapping: when the field was resolved from a newtype (e.g. NodeIndex → u32),
        // unwrap the core newtype by accessing `.0`.
        // e.g. `source: val.source` → `source: val.source.0`
        //      `parent: val.parent` → `parent: val.parent.map(|v| v.0)`
        //      `children: val.children` → `children: val.children.iter().map(|v| v.0).collect()`
        let base_conversion = if field.newtype_wrapper.is_some() {
            match &field.ty {
                TypeRef::Optional(_) => {
                    // Replace `val.{name}` with `val.{name}.map(|v| v.0)` in the generated expression
                    base_conversion.replace(
                        &format!("val.{}", field.name),
                        &format!("val.{}.map(|v| v.0)", field.name),
                    )
                }
                TypeRef::Vec(_) => {
                    // Replace `val.{name}` with `val.{name}.iter().map(|v| v.0).collect()` in expression
                    base_conversion.replace(
                        &format!("val.{}", field.name),
                        &format!("val.{}.iter().map(|v| v.0).collect::<Vec<_>>()", field.name),
                    )
                }
                // When `optional=true` and `ty` is a plain Primitive (not TypeRef::Optional), the core
                // field is actually `Option<NewtypeT>`, so we must use `.map(|v| v.0)` not `.0`.
                _ if field.optional => base_conversion.replace(
                    &format!("val.{}", field.name),
                    &format!("val.{}.map(|v| v.0)", field.name),
                ),
                _ => {
                    // Direct field: append `.0` to access the inner primitive
                    base_conversion.replace(&format!("val.{}", field.name), &format!("val.{}.0", field.name))
                }
            }
        } else {
            base_conversion
        };
        // When field.optional=true AND field.ty=Optional(T), the binding struct flattens
        // Option<Option<T>> to Option<T>. Core produces Option<Option<T>>, binding needs
        // Option<T>. Generate the conversion by treating the pre-flattened field as Option<T>:
        // call the standard conversion for the inner type T with optional=true, substituting
        // val.{name}.flatten() for val.{name} so all cast/conversion logic applies to T.
        let is_flattened_optional = field.optional && matches!(field.ty, TypeRef::Optional(_));
        let base_conversion = if is_flattened_optional {
            if let TypeRef::Optional(inner) = &field.ty {
                // Produce the conversion as if the field is Option<inner> with value val.name.flatten()
                let inner_conv = field_conversion_from_core_cfg(
                    &field.name,
                    inner.as_ref(),
                    true,
                    field.sanitized,
                    opaque_types,
                    config,
                );
                // inner_conv references val.{name}; replace with val.{name}.flatten()
                inner_conv.replace(&format!("val.{}", field.name), &format!("val.{}.flatten()", field.name))
            } else {
                base_conversion
            }
        } else {
            base_conversion
        };
        // Optionalized non-optional fields need Some() wrapping in core→binding direction.
        // This covers both NAPI-style full optionalization and PyO3-style Duration optionalization.
        // Flattened-optional fields are already handled above with the correct type.
        let needs_some_wrap = !is_flattened_optional
            && ((optionalized && !field.optional)
                || (config.option_duration_on_defaults
                    && typ.has_default
                    && !field.optional
                    && matches!(field.ty, TypeRef::Duration)));
        let conversion = if needs_some_wrap {
            // Extract the value expression after "name: " and wrap in Some()
            if let Some(expr) = base_conversion.strip_prefix(&format!("{}: ", field.name)) {
                format!("{}: Some({})", field.name, expr)
            } else {
                base_conversion
            }
        } else {
            base_conversion
        };
        // Opaque Named fields without CoreWrapper::Arc (e.g. visitor: Object<'static>) cannot be
        // auto-converted via Arc::new — the binding stores a raw host object that needs a bridge.
        // Emit Default::default() and let the caller (e.g. the convert function) set it separately.
        let is_opaque_no_wrapper_field = field.core_wrapper == CoreWrapper::None
            && matches!(&field.ty, TypeRef::Named(n) if config
                .opaque_types
                .is_some_and(|opaque| opaque.contains(n.as_str())));
        // CoreWrapper: unwrap Arc, convert Cow→String, Bytes→Vec<u8>
        // For sanitized fields, still apply Cow→String conversion: Cow<'_, str> sanitizes to
        // TypeRef::String and the Debug-formatted fallback produces quotes, but Cow implements
        // Display so .to_string() (emitted by apply_core_wrapper_from_core for Cow) is correct.
        // Other sanitized fields (unknown Named types) still fall through to Debug formatting.
        let conversion = if is_opaque_no_wrapper_field {
            // Trait-bridge OptionsField fields wrap the core handle in `Arc<core::T>` on the
            // binding side. Construct the wrapper so the value round-trips through `.into()`
            // (e.g. PHP's `builder().visitor(v).build()` -> `convert(html, opts)`) instead of
            // being silently dropped. Other backends (e.g. NAPI where the binding stores a raw
            // host Object) keep the Default::default() fallback.
            if config.trait_bridge_field_is_arc_wrapper(&field.name) {
                if let TypeRef::Named(name) = &field.ty {
                    let wrapper = format!("{}{}", config.type_name_prefix, name);
                    if field.optional {
                        format!(
                            "{}: val.{}.map(|v| {wrapper} {{ inner: std::sync::Arc::new(v) }})",
                            field.name, field.name
                        )
                    } else {
                        format!(
                            "{}: {wrapper} {{ inner: std::sync::Arc::new(val.{}) }}",
                            field.name, field.name
                        )
                    }
                } else {
                    format!("{}: Default::default()", field.name)
                }
            } else {
                format!("{}: Default::default()", field.name)
            }
        } else if !field.sanitized || field.core_wrapper == crate::core::ir::CoreWrapper::Cow {
            apply_core_wrapper_from_core(
                &conversion,
                &field.name,
                &field.ty,
                &field.core_wrapper,
                &field.vec_inner_core_wrapper,
                field.optional,
            )
        } else {
            conversion
        };
        // In core→binding direction, the binding struct field may be keyword-escaped
        // (e.g. `class_` for `class`). The generated conversion has `field.name: expr`
        // on the left side — rename it to `binding_name: expr` when needed.
        let binding_field = config.binding_field_name_owned(&typ.name, &field.name);
        let conversion = if binding_field != field.name {
            if let Some(expr) = conversion.strip_prefix(&format!("{}: ", field.name)) {
                format!("{binding_field}: {expr}")
            } else {
                conversion
            }
        } else {
            conversion
        };
        fields.push(conversion);
    }

    crate::codegen::template_env::render(
        "conversions/core_to_binding_impl",
        minijinja::context! {
            core_path => core_path,
            binding_name => binding_name,
            has_lifetime_params => typ.has_lifetime_params,
            is_newtype => false,
            newtype_inner_expr => "",
            fields => fields,
        },
    )
}
