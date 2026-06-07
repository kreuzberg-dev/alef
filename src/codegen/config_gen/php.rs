use super::shared::{constructor_fields, default_value_for_field, use_unwrap_or_default};
use crate::core::ir::{TypeDef, TypeRef};

/// Generate a PHP kwargs constructor for a type with `has_default`.
/// All fields become `Option<T>` parameters so PHP users can omit any field.
/// Assignments wrap non-Optional fields in `Some()` and apply defaults.
pub fn gen_php_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let fields: Vec<_> = constructor_fields(typ)
        .map(|field| {
            let mapped = type_mapper(&field.ty);
            let is_optional_field = field.optional || matches!(&field.ty, TypeRef::Optional(_));

            let assignment = if is_optional_field {
                // Struct field is Option<T>, param is Option<T> — pass through directly
                field.name.clone()
            } else if use_unwrap_or_default(field) {
                // Struct field is T, param is Option<T> — unwrap with type's default
                format!("{}.unwrap_or_default()", field.name)
            } else {
                // Struct field is T, param is Option<T> — unwrap with explicit default
                let default_str = default_value_for_field(field, "rust");
                format!("{}.unwrap_or({})", field.name, default_str)
            };

            minijinja::context! {
                name => field.name.clone(),
                ty => mapped,
                assignment => assignment,
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/php_kwargs_constructor.jinja",
        minijinja::context! {
            fields => fields,
        },
    )
}
