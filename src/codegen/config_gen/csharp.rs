use super::shared::{constructor_fields, default_value_for_field, is_tuple_field};
use crate::core::ir::{TypeDef, TypeRef};
use heck::ToPascalCase;

/// Generate C# record with init properties for a type with `has_default`.
pub fn gen_csharp_record(typ: &TypeDef, namespace: &str, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let fields: Vec<_> = constructor_fields(typ)
        .filter(|field| !is_tuple_field(field))
        .map(|field| {
            minijinja::context! {
                type => type_mapper(&field.ty),
                name_pascal => field.name.to_pascal_case(),
                default => default_value_for_field(field, "csharp"),
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/csharp_record.jinja",
        minijinja::context! {
            namespace => namespace,
            type_name => typ.name.clone(),
            fields => fields,
        },
    )
    .trim_end_matches('\n')
    .to_string()
}
