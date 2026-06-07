use super::shared::{constructor_fields, default_value_for_field, use_unwrap_or_default};
use crate::core::ir::{TypeDef, TypeRef};

/// Generate a Rustler (Elixir) kwargs constructor for a type with `has_default`.
/// Accepts keyword list or map, applies defaults for missing fields.
/// Fields in `exclude_fields` are skipped (used for bridge fields that cannot implement Encoder/Decoder).
pub fn gen_rustler_kwargs_constructor_with_exclude(
    typ: &TypeDef,
    _type_mapper: &dyn Fn(&TypeRef) -> String,
    exclude_fields: &std::collections::HashSet<String>,
) -> String {
    // Pre-compute field assignments (same logic as gen_rustler_kwargs_constructor but with exclusion)
    let fields: Vec<_> = constructor_fields(typ)
        .filter(|f| !exclude_fields.contains(&f.name))
        .map(|field| {
            let assignment = if field.optional {
                format!("opts.get(\"{}\").and_then(|t| t.decode().ok()),", field.name)
            } else if use_unwrap_or_default(field) {
                format!(
                    "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or_default(),",
                    field.name
                )
            } else {
                let default_str = default_value_for_field(field, "rust");
                let is_enum_variant_default = default_str.contains("::") || default_str.starts_with("\"");

                if (is_enum_variant_default && matches!(&field.ty, TypeRef::String | TypeRef::Char))
                    || matches!(&field.ty, TypeRef::Named(_))
                {
                    format!(
                        "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or_default(),",
                        field.name
                    )
                } else {
                    format!(
                        "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or({}),",
                        field.name, default_str
                    )
                }
            };

            minijinja::context! {
                name => field.name.clone(),
                assignment => assignment,
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/rustler_kwargs_constructor.jinja",
        minijinja::context! {
            fields => fields,
        },
    )
}

/// Generate a Rustler (Elixir) kwargs constructor for a type with `has_default`.
/// Accepts keyword list or map, applies defaults for missing fields.
pub fn gen_rustler_kwargs_constructor(typ: &TypeDef, _type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    // Pre-compute field assignments
    let fields: Vec<_> = constructor_fields(typ)
        .map(|field| {
            let assignment = if field.optional {
                format!("opts.get(\"{}\").and_then(|t| t.decode().ok()),", field.name)
            } else if use_unwrap_or_default(field) {
                format!(
                    "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or_default(),",
                    field.name
                )
            } else {
                let default_str = default_value_for_field(field, "rust");
                let is_enum_variant_default = default_str.contains("::") || default_str.starts_with("\"");

                let unwrap_default = (is_enum_variant_default && matches!(&field.ty, TypeRef::String | TypeRef::Char))
                    || matches!(&field.ty, TypeRef::Named(_));
                if unwrap_default {
                    format!(
                        "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or_default(),",
                        field.name
                    )
                } else {
                    format!(
                        "opts.get(\"{}\").and_then(|t| t.decode().ok()).unwrap_or({}),",
                        field.name, default_str
                    )
                }
            };

            minijinja::context! {
                name => field.name.clone(),
                assignment => assignment,
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/rustler_kwargs_constructor.jinja",
        minijinja::context! {
            fields => fields,
        },
    )
}
