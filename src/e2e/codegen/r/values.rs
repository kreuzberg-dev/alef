//! R literal rendering helpers.

use crate::e2e::escape::escape_r;

/// Convert a `serde_json::Value` to an R literal string.
///
/// # Arguments
///
/// * `value` - The JSON value to convert
///
/// Convert a PascalCase string to snake_case.
/// e.g. "DoubleEqual" → "double_equal", "Backticks" → "backticks"
fn pascal_to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        for lc in ch.to_lowercase() {
            result.push(lc);
        }
    }
    result
}

/// Convert a JSON value to an R expression suitable for embedding inside a
/// `list(...)` that will be passed to `jsonlite::toJSON(..., auto_unbox = TRUE)`.
///
/// Differs from [`json_to_r`] in that any array-valued field is wrapped with
/// `I(...)` (jsonlite's `AsIs` marker) so it remains a JSON array after the
/// `auto_unbox` transform. Empty arrays become `I(list())` (→ `[]`) and
/// non-empty arrays become `I(c(...))` (→ `[..]`). Without this wrapping,
/// `Vec<String>` fields like `exclude_selectors` get unboxed to scalars and
/// serde deserialization on the Rust side fails with
/// `invalid type: string "foo", expected a sequence`.
pub(super) fn json_to_r_preserve_arrays(value: &serde_json::Value, lowercase_enum_values: bool) -> String {
    match value {
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "I(list())".to_string()
            } else {
                let items: Vec<String> = arr.iter().map(|v| json_to_r(v, lowercase_enum_values)).collect();
                format!("I(c({}))", items.join(", "))
            }
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "\"{}\" = {}",
                        escape_r(k),
                        json_to_r_preserve_arrays(v, lowercase_enum_values)
                    )
                })
                .collect();
            format!("list({})", entries.join(", "))
        }
        _ => json_to_r(value, lowercase_enum_values),
    }
}

/// * `lowercase_enum_values` - If true, convert PascalCase strings to snake_case (for enum values).
///   If false, preserve original case (for assertion expected values).
pub(super) fn json_to_r(value: &serde_json::Value, lowercase_enum_values: bool) -> String {
    match value {
        serde_json::Value::String(s) => {
            // Convert PascalCase enum values to snake_case only if requested.
            // e.g. "Backticks" → "backticks", "DoubleEqual" → "double_equal"
            let normalized = if lowercase_enum_values && s.chars().next().is_some_and(|c| c.is_uppercase()) {
                pascal_to_snake_case(s)
            } else {
                s.clone()
            };
            format!("\"{}\"", escape_r(&normalized))
        }
        serde_json::Value::Bool(true) => "TRUE".to_string(),
        serde_json::Value::Bool(false) => "FALSE".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| json_to_r(v, lowercase_enum_values)).collect();
            format!("c({})", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\" = {}", escape_r(k), json_to_r(v, lowercase_enum_values)))
                .collect();
            format!("list({})", entries.join(", "))
        }
    }
}
