use super::shared::{constructor_fields, default_value_for_field, is_tuple_field};
use crate::core::ir::{TypeDef, TypeRef};
use heck::ToPascalCase;

/// Generate Go functional options pattern for a type with `has_default`.
/// Returns: type definition + Option type + WithField functions + NewConfig constructor
pub fn gen_go_functional_options(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let fields: Vec<_> = constructor_fields(typ)
        .filter(|field| !is_tuple_field(field))
        .map(|field| {
            minijinja::context! {
                name => field.name.clone(),
                pascal_name => field.name.to_pascal_case(),
                field_name => field.name.to_pascal_case(),
                go_type => type_mapper(&field.ty),
                default => default_value_for_field(field, "go"),
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/go_functional_options.jinja",
        minijinja::context! {
            type_name => typ.name.clone(),
            fields => fields,
        },
    )
    .trim_end_matches('\n')
    .to_string()
}
