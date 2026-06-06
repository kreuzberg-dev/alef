//! Kotlin JSON value helpers.

use crate::e2e::escape::escape_kotlin;

/// Convert a `serde_json::Value` to a Kotlin literal string.
pub(super) fn json_to_kotlin(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_kotlin(s)),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                // Kotlin Double literals use no suffix (or `.0` if integer-shaped).
                // `0.9d` would parse as identifier `d` following a malformed literal.
                let s = n.to_string();
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{s}.0")
                }
            } else {
                n.to_string()
            }
        }
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_kotlin).collect();
            format!("listOf({})", items.join(", "))
        }
        serde_json::Value::Object(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            format!("\"{}\"", escape_kotlin(&json_str))
        }
    }
}
