use crate::core::ir::{TypeDef, TypeRef};

/// Generate an extendr (R) kwargs constructor for a type with `has_default`.
///
/// Rust does not support function-parameter defaults, and extendr 0.9 only allows
/// defaults via the per-parameter `#[extendr(default = "...")]` attribute (not via
/// `param: T = expr` syntax).  Rather than encode every default in attribute form,
/// we accept each field as `Option<T>` and unwrap it via `T::default()` (or via the
/// type's own `Default::default()` for the whole struct as the base) inside the body.
/// The R-side wrapper generated in `generate_public_api` already supplies named
/// arguments with `NULL` defaults, so callers see ergonomic kwargs at the R level.
///
/// `enum_names` is the set of type names that are enums in this API surface.  For
/// fields whose type resolves to a Named enum, the parameter is widened to
/// `Option<String>` (extendr has no `TryFrom<&Robj>` for binding enums) and the body
/// deserialises the string back to the enum via `serde_json::from_str`.
pub fn gen_extendr_kwargs_constructor(
    typ: &TypeDef,
    type_mapper: &dyn Fn(&TypeRef) -> String,
    enum_names: &ahash::AHashSet<String>,
) -> String {
    // Helper predicates to classify field types
    let is_named_enum = |ty: &TypeRef| -> bool { matches!(ty, TypeRef::Named(n) if enum_names.contains(n.as_str())) };
    let is_named_struct =
        |ty: &TypeRef| -> bool { matches!(ty, TypeRef::Named(n) if !enum_names.contains(n.as_str())) };
    let is_optional_named_struct = |ty: &TypeRef| -> bool {
        if let TypeRef::Optional(inner) = ty {
            is_named_struct(inner)
        } else {
            false
        }
    };
    let ty_is_optional = |ty: &TypeRef| -> bool { matches!(ty, TypeRef::Optional(_)) };

    // Pre-collect emittable fields (skip struct-typed fields that extendr cannot convert)
    let emittable_fields: Vec<_> = typ
        .fields
        .iter()
        .filter(|f| {
            !f.binding_excluded && f.cfg.is_none() && !is_named_struct(&f.ty) && !is_optional_named_struct(&f.ty)
        })
        .map(|field| {
            let param_type = if is_named_enum(&field.ty) {
                "Option<String>".to_string()
            } else if ty_is_optional(&field.ty) {
                type_mapper(&field.ty)
            } else {
                format!("Option<{}>", type_mapper(&field.ty))
            };

            minijinja::context! {
                name => field.name.clone(),
                type => param_type,
            }
        })
        .collect();

    // Pre-compute body assignments for all fields
    let body_assignments: Vec<_> = typ
        .fields
        .iter()
        .filter(|f| !f.binding_excluded && f.cfg.is_none() && !is_named_struct(&f.ty) && !is_optional_named_struct(&f.ty))
        .map(|field| {
            let code = if is_named_enum(&field.ty) {
                if field.optional {
                    format!(
                        "if let Some(v) = {} {{ __out.{} = serde_json::from_str(&format!(\"\\\"{{v}}\\\"\")).ok(); }}",
                        field.name, field.name
                    )
                } else {
                    format!(
                        "if let Some(v) = {} {{ if let Ok(parsed) = serde_json::from_str(&format!(\"\\\"{{v}}\\\"\")) {{ __out.{} = parsed; }} }}",
                        field.name, field.name
                    )
                }
            } else if ty_is_optional(&field.ty) || field.optional {
                format!(
                    "if let Some(v) = {} {{ __out.{} = Some(v); }}",
                    field.name, field.name
                )
            } else {
                format!(
                    "if let Some(v) = {} {{ __out.{} = v; }}",
                    field.name, field.name
                )
            };

            minijinja::context! {
                code => code,
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/extendr_kwargs_constructor.jinja",
        minijinja::context! {
            type_name => typ.name.clone(),
            type_name_lower => typ.name.to_lowercase(),
            params => emittable_fields,
            body_assignments => body_assignments,
        },
    )
}
