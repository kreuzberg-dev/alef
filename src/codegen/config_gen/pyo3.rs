use super::shared::{constructor_fields, default_value_for_field};
use crate::core::ir::{TypeDef, TypeRef};

/// Generate a PyO3 `#[new]` constructor with kwargs for a type with `has_default`.
/// All fields become keyword args with their defaults in `#[pyo3(signature = (...))]`.
pub fn gen_pyo3_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let signature_defaults = constructor_fields(typ)
        .map(|field| format!("{}={}", field.name, default_value_for_field(field, "python")))
        .collect::<Vec<_>>()
        .join(", ");
    let fields: Vec<_> = constructor_fields(typ)
        .map(|field| {
            minijinja::context! {
                name => field.name.clone(),
                type => type_mapper(&field.ty),
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/pyo3_kwargs_constructor.jinja",
        minijinja::context! {
            signature_defaults => signature_defaults,
            fields => fields,
        },
    )
    .trim_end_matches('\n')
    .to_string()
}
