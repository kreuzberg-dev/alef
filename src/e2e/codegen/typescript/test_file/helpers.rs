use super::super::json::snake_to_camel;

pub(super) fn is_typescript_primitive_element_type(element_type: &str) -> bool {
    matches!(
        element_type,
        "string"
            | "String"
            | "&str"
            | "number"
            | "float"
            | "f32"
            | "f64"
            | "int"
            | "integer"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "boolean"
            | "bool"
            | "bytes"
            | "Uint8Array"
    )
}

pub(in crate::e2e::codegen::typescript) fn resolve_node_function_name(
    call_config: &crate::e2e::config::CallConfig,
) -> String {
    call_config
        .overrides
        .get("node")
        .and_then(|o| o.function.clone())
        .unwrap_or_else(|| snake_to_camel(&call_config.function))
}

/// Return the package-level helper function name to import for a method_result method,
/// or `None` if the method maps to a property access (no import needed).
pub(super) fn ts_method_helper_import(method_name: &str) -> Option<String> {
    match method_name {
        "has_error_nodes" => Some("treeHasErrorNodes".to_string()),
        "error_count" | "tree_error_count" => Some("treeErrorCount".to_string()),
        "tree_to_sexp" => Some("treeToSexp".to_string()),
        "contains_node_type" => Some("treeContainsNodeType".to_string()),
        "find_nodes_by_type" => Some("findNodesByType".to_string()),
        "run_query" => Some("runQuery".to_string()),
        _ => None,
    }
}

pub(super) fn strip_setup_metadata(input: &serde_json::Value) -> serde_json::Value {
    match input {
        serde_json::Value::Object(map) => {
            let mut cleaned = map.clone();
            cleaned.remove("setup");
            serde_json::Value::Object(cleaned)
        }
        other => other.clone(),
    }
}

pub(super) fn canonical_ts_type_name(
    lang: &str,
    type_name: &str,
    config: &crate::core::config::ResolvedCrateConfig,
) -> String {
    if lang == "node" {
        type_name
            .strip_prefix(&config.node_type_prefix())
            .unwrap_or(type_name)
            .to_string()
    } else {
        type_name.to_string()
    }
}
