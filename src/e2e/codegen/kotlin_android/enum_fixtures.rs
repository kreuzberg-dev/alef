use std::collections::HashSet;

/// Returns true when `ty` is a `Named(T)` reference (or `Optional<Named(T)>`)
/// where `T` is **not** a known struct name. Such fields are enum-typed and
/// must route through `.getValue()` in generated assertions.
pub(super) fn is_enum_typed(ty: &crate::core::ir::TypeRef, struct_names: &HashSet<&str>) -> bool {
    use crate::core::ir::TypeRef;
    match ty {
        TypeRef::Named(name) => !struct_names.contains(name.as_str()),
        TypeRef::Optional(inner) => {
            matches!(inner.as_ref(), TypeRef::Named(name) if !struct_names.contains(name.as_str()))
        }
        _ => false,
    }
}

/// Extract a default value from fixture.input.backend for a stub method.
///
/// Given a method name and fixture, attempts to find the corresponding input value
/// in fixture.input.backend. For numeric defaults that would be 0, emits 1 instead
/// (downstream validation rejects 0 for counts like dimensions).
pub(super) fn extract_kotlin_android_fixture_default(
    method_name: &str,
    fixture: &crate::e2e::fixture::Fixture,
) -> Option<String> {
    use heck::ToLowerCamelCase;

    let backend_input = fixture.input.get("backend").and_then(|v| v.as_object())?;

    // Try snake_case first, then lower_camel_case.
    let val = backend_input
        .get(&method_name.to_lowercase())
        .or_else(|| backend_input.get(&method_name.to_lower_camel_case()))?;

    Some(match val {
        serde_json::Value::Number(n) => {
            // For numeric defaults, emit 1 instead of 0 if it's 0.
            if let Some(i) = n.as_i64() {
                if i == 0 { "1".to_string() } else { i.to_string() }
            } else if let Some(u) = n.as_u64() {
                if u == 0 { "1".to_string() } else { u.to_string() }
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => format!("\"{}\"", s),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => return None, // Complex types not supported
    })
}
