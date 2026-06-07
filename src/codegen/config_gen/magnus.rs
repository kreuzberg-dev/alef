use super::shared::{constructor_fields, default_value_for_field, use_unwrap_or_default};
use crate::core::ir::{DefaultValue, FieldDef, TypeDef, TypeRef};

const MAGNUS_MAX_ARITY: usize = 15;

/// Generate a Magnus (Ruby) kwargs constructor for a type with `has_default`.
///
/// For types with <=15 fields, generates a positional `Option<T>` parameter constructor.
/// For types with >15 fields (exceeding Magnus arity limit), generates a hash-based constructor
/// using `RHash` that extracts fields by name, applying defaults for missing keys.
pub fn gen_magnus_kwargs_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    // Always use the hash-based constructor so Ruby callers can pass keyword args
    // (`Type.new(field1: ..., field2: ...)`) regardless of field count. Magnus
    // function! macro caps arity at 15, but the hash form uses variadic arity (-1)
    // and works for any number of fields.
    let _ = MAGNUS_MAX_ARITY;
    gen_magnus_hash_constructor(typ, type_mapper)
}

/// Wrap a type string for use as a type-path prefix in Rust.
///
/// Types containing `<` (generics like `Vec<String>`, `Option<T>`) cannot be used as
/// `Vec<String>::try_convert(v)` — that's a parse error. They must use the UFCS form
/// `<Vec<String>>::try_convert(v)` instead. Simple names like `String`, `bool` can use
/// `String::try_convert(v)` directly.
fn as_type_path_prefix(type_str: &str) -> String {
    if type_str.contains('<') {
        format!("<{type_str}>")
    } else {
        type_str.to_string()
    }
}

/// Generate a hash-based Magnus constructor for types with many fields.
/// Accepts `(kwargs: RHash)` and extracts each field by symbol name, applying defaults.
fn gen_magnus_hash_constructor(typ: &TypeDef, type_mapper: &dyn Fn(&TypeRef) -> String) -> String {
    let fields: Vec<_> = constructor_fields(typ)
        .map(|field| {
            let is_optional = field_is_optional_in_rust(field);
            // Use inner type for try_convert, since the hash value is T, not Option<T>.
            // When field.ty is already Optional(T) and field.optional is true, strip one layer so we
            // call <T>::try_convert, not <Option<T>>::try_convert (which would yield Option<Option<T>>).
            let effective_inner_ty = match &field.ty {
                TypeRef::Optional(inner) if is_optional => inner.as_ref(),
                ty => ty,
            };
            let inner_type = type_mapper(effective_inner_ty);
            let type_prefix = as_type_path_prefix(&inner_type);

            let assignment = if is_optional {
                // Field is Option<T>: extract from hash, wrap in Some, default to None
                format!(
                    "kwargs.get(ruby.to_symbol(\"{}\")).and_then(|v| {}::try_convert(v).ok()),",
                    field.name, type_prefix
                )
            } else if use_unwrap_or_default(field) {
                format!(
                    "kwargs.get(ruby.to_symbol(\"{}\")).and_then(|v| {}::try_convert(v).ok()).unwrap_or_default(),",
                    field.name, type_prefix
                )
            } else if matches!(effective_inner_ty, TypeRef::Named(_))
                && !matches!(&field.typed_default, Some(DefaultValue::EnumVariant(_)))
            {
                // Named types in the Magnus binding default to required because
                // Magnus-wrapped structs (`#[magnus::wrap]`) never implement
                // `Default`. The exception is Named *enum* fields whose typed
                // default we resolved to a specific variant (e.g.
                // `output_format: OutputFormat::Plain` from the parent's
                // `impl Default`): those have a concrete fallback expression
                // and fall through to the explicit-default branch below so
                // missing kwargs still construct successfully.
                format!(
                    "kwargs.get(ruby.to_symbol(\"{}\")).and_then(|v| {}::try_convert(v).ok()).ok_or_else(|| magnus::Error::new(unsafe {{ magnus::Ruby::get_unchecked() }}.exception_arg_error(), \"missing required field: {}\"))?,",
                    field.name, type_prefix, field.name
                )
            } else {
                // When the binding maps the field type to String (e.g. an excluded enum), but the
                // original default is an EnumVariant, `default_value_for_field` would emit
                // `TypeName::Variant` which is invalid for a `String` field. Fall back to the
                // string-literal form in that case.
                let default_str = if inner_type == "String" {
                    if let Some(DefaultValue::EnumVariant(variant)) = &field.typed_default {
                        use heck::ToSnakeCase;
                        format!("\"{}\".to_string()", variant.to_snake_case())
                    } else {
                        default_value_for_field(field, "rust")
                    }
                } else {
                    default_value_for_field(field, "rust")
                };
                format!(
                    "kwargs.get(ruby.to_symbol(\"{}\")).and_then(|v| {}::try_convert(v).ok()).unwrap_or({}),",
                    field.name, type_prefix, default_str
                )
            };

            minijinja::context! {
                name => field.name.clone(),
                assignment => assignment,
            }
        })
        .collect();

    crate::codegen::template_env::render(
        "config_gen/magnus_hash_constructor.jinja",
        minijinja::context! {
            fields => fields,
        },
    )
}

/// Returns true if the generated Rust field type is already `Option<T>`.
/// This covers both:
/// - Fields with `optional: true` (the Rust field type becomes `Option<inner_type>`)
/// - Fields whose `TypeRef` is explicitly `Optional(_)` (rare, for nested Option types)
fn field_is_optional_in_rust(field: &FieldDef) -> bool {
    field.optional || matches!(&field.ty, TypeRef::Optional(_))
}
