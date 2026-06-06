//! Go e2e JSON fixture conversion helpers.

use crate::e2e::escape::go_string_literal;

/// Convert a `serde_json::Value` to a Go literal string.
/// Recursively convert a JSON value for Go struct unmarshalling.
///
/// The Go binding's configured options struct uses:
/// - `snake_case` JSON field tags (e.g. `"code_block_style"` not `"codeBlockStyle"`)
/// - lowercase/snake_case string values for enums (e.g. `"indented"`, `"atx_closed"`)
///
/// Fixture JSON uses camelCase keys and PascalCase enum values (Python/TS conventions).
/// This function remaps both so the generated Go tests can unmarshal correctly.
pub(super) fn convert_json_for_go(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| (camel_to_snake_case(&k), convert_json_for_go(v)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            // Check if this is a byte array (array of integers 0-255).
            // If so, encode as base64 string for Go json.Unmarshal compatibility.
            if is_byte_array(&arr) {
                let bytes: Vec<u8> = arr
                    .iter()
                    .filter_map(|v| v.as_u64().and_then(|n| if n <= 255 { Some(n as u8) } else { None }))
                    .collect();
                // Encode bytes as base64 for Go json.Unmarshal (Go expects []byte as base64 strings)
                let encoded = base64_encode(&bytes);
                serde_json::Value::String(encoded)
            } else {
                serde_json::Value::Array(arr.into_iter().map(convert_json_for_go).collect())
            }
        }
        serde_json::Value::String(s) => {
            // Convert PascalCase enum values to snake_case.
            // Only convert values that look like PascalCase (start with uppercase, no spaces).
            serde_json::Value::String(pascal_to_snake_case(&s))
        }
        other => other,
    }
}

/// Check if an array looks like a byte array (all elements are integers 0-255).
fn is_byte_array(arr: &[serde_json::Value]) -> bool {
    if arr.is_empty() {
        return false;
    }
    arr.iter().all(|v| {
        if let serde_json::Value::Number(n) = v {
            n.is_u64() && n.as_u64().is_some_and(|u| u <= 255)
        } else {
            false
        }
    })
}

/// Encode bytes as base64 string (standard alphabet without padding in this output,
/// though Go's json.Unmarshal handles both).
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;

    while i + 2 < bytes.len() {
        let b1 = bytes[i];
        let b2 = bytes[i + 1];
        let b3 = bytes[i + 2];

        result.push(TABLE[(b1 >> 2) as usize] as char);
        result.push(TABLE[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
        result.push(TABLE[(((b2 & 0x0f) << 2) | (b3 >> 6)) as usize] as char);
        result.push(TABLE[(b3 & 0x3f) as usize] as char);

        i += 3;
    }

    // Handle remaining bytes
    if i < bytes.len() {
        let b1 = bytes[i];
        result.push(TABLE[(b1 >> 2) as usize] as char);

        if i + 1 < bytes.len() {
            let b2 = bytes[i + 1];
            result.push(TABLE[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
            result.push(TABLE[((b2 & 0x0f) << 2) as usize] as char);
            result.push('=');
        } else {
            result.push(TABLE[((b1 & 0x03) << 4) as usize] as char);
            result.push_str("==");
        }
    }

    result
}

/// Convert a camelCase or PascalCase string to snake_case.
fn camel_to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_upper = false;
    for (i, c) in s.char_indices() {
        if c.is_uppercase() {
            if i > 0 && !prev_upper {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
            prev_upper = true;
        } else {
            if prev_upper && i > 1 {
                // Handles sequences like "URLPath" → "url_path": insert _ before last uppercase
                // when transitioning from a run of uppercase back to lowercase.
                // This is tricky — use simple approach: detect Aa pattern.
            }
            result.push(c);
            prev_upper = false;
        }
    }
    result
}

/// Convert a PascalCase string to snake_case (for enum values).
///
/// Only converts if the string looks like PascalCase (starts uppercase, no spaces/underscores).
/// Values that are already lowercase/snake_case are returned unchanged.
fn pascal_to_snake_case(s: &str) -> String {
    // Skip conversion for strings that already contain underscores, spaces, or start lowercase.
    let first_char = s.chars().next();
    if first_char.is_none() || !first_char.unwrap().is_uppercase() || s.contains('_') || s.contains(' ') {
        return s.to_string();
    }
    camel_to_snake_case(s)
}

/// Map an `ArgMapping.element_type` to a Go slice type. Used for `json_object` args
/// whose fixture value is a JSON array. The element type is wrapped in `[]…` so an
/// element of `String` becomes `[]string` and `Vec<String>` becomes `[][]string`.
pub(super) fn element_type_to_go_slice(element_type: Option<&str>, import_alias: &str) -> String {
    let elem = element_type.unwrap_or("String").trim();
    let go_elem = rust_type_to_go(elem, import_alias);
    format!("[]{go_elem}")
}

/// Map a small subset of Rust scalar / `Vec<T>` types to their Go equivalents.
/// For unknown types, qualify with the import alias (e.g., "sample_core.FileJob").
fn rust_type_to_go(rust: &str, import_alias: &str) -> String {
    let trimmed = rust.trim();
    if let Some(inner) = trimmed.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        return format!("[]{}", rust_type_to_go(inner, import_alias));
    }
    match trimmed {
        "String" | "&str" | "str" => "string".to_string(),
        "bool" => "bool".to_string(),
        "f32" => "float32".to_string(),
        "f64" => "float64".to_string(),
        "i8" => "int8".to_string(),
        "i16" => "int16".to_string(),
        "i32" => "int32".to_string(),
        "i64" | "isize" => "int64".to_string(),
        "u8" => "uint8".to_string(),
        "u16" => "uint16".to_string(),
        "u32" => "uint32".to_string(),
        "u64" | "usize" => "uint64".to_string(),
        _ => format!("{import_alias}.{trimmed}"),
    }
}

pub(super) fn json_to_go(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => go_string_literal(s),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "nil".to_string(),
        // For complex types, serialize to JSON string and pass as literal.
        other => go_string_literal(&other.to_string()),
    }
}
