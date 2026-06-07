use crate::core::ir::{EnumDef, TypeDef};
use heck::ToUpperCamelCase;

use super::super::json::{json_to_js, json_to_js_camel, snake_to_camel};
use super::wasm::{derive_nested_types_for_wasm, is_tagged_data_enum};
use crate::e2e::escape::escape_js;

/// Pre-process a JSON value so that napi-rs (node) binding can deserialize it.
///
/// The napi-rs backend always emits `#[napi(js_name = "kind")]` for the
/// discriminant field of every tagged-data enum, regardless of the original
/// Rust `#[serde(tag = "...")]` attribute. For example, `Message` has
/// `#[serde(tag = "role")]`, but `JsMessage.role_tag` is exposed to
/// TypeScript as `"kind"`. A fixture that sends `{ role: "user" }` causes
/// napi-rs to return `Error: Missing field 'kind'`.
///
/// This function walks the JSON tree and renames any serde_tag key to
/// `"kind"` when the key's value is a string that matches a known variant
/// of the corresponding tagged-data enum. Renaming is limited to exact
/// variant matches so that plain struct fields that happen to share the
/// same key name as a serde_tag (e.g. `type: "function"` on
/// `ChatCompletionTool` where "function" is not a `ContentPart` variant)
/// are left unchanged.
pub(super) fn rename_napi_serde_tags_to_kind(value: &serde_json::Value, enums: &[EnumDef]) -> serde_json::Value {
    // Build map: serde_tag_key → (set of variant serde-names, actual_tag_name).
    // Only include tagged-data enums (serde_tag present AND at least one
    // variant with fields so the binding is a flattened struct, not a plain
    // string enum).
    let mut tag_map: std::collections::HashMap<&str, (std::collections::HashSet<String>, &str)> =
        std::collections::HashMap::new();
    for e in enums {
        if let Some(tag) = e.serde_tag.as_deref() {
            if e.variants.iter().any(|v| !v.fields.is_empty()) {
                let variants: std::collections::HashSet<String> = e
                    .variants
                    .iter()
                    .map(|v| v.serde_rename.as_deref().unwrap_or(&v.name).to_string())
                    .collect();
                tag_map.insert(tag, (variants, tag));
            }
        }
    }

    rename_napi_serde_tags_recursive(value, &tag_map)
}

fn rename_napi_serde_tags_recursive(
    value: &serde_json::Value,
    tag_map: &std::collections::HashMap<&str, (std::collections::HashSet<String>, &str)>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                // Preserve the original serde_tag key name when:
                //  1. the key is a known serde_tag name, AND
                //  2. the value is a string that matches a known variant of that enum.
                // The actual tag field name is already correct in the fixture; we only need
                // to validate and recurse.
                let new_key = key.clone();
                if let Some((variants, _)) = tag_map.get(key.as_str()) {
                    if !val.as_str().is_some_and(|s| variants.contains(s)) {
                        // Not a valid variant value for this tag; leave as-is and recurse
                    }
                }
                new_map.insert(new_key, rename_napi_serde_tags_recursive(val, tag_map));
            }
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.iter()
                .map(|item| rename_napi_serde_tags_recursive(item, tag_map))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Convert a JS numeric literal expression to a BigInt-compatible literal
/// (`123n`, `-7n`) for wasm-bindgen `u64`/`i64` setters which reject Number.
/// Non-integer or non-numeric expressions are wrapped in `BigInt(...)` so the
/// runtime conversion still happens.
fn to_bigint_literal(value_expr: &str) -> String {
    let trimmed = value_expr.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return format!("{trimmed}n");
    }
    if let Some(rest) = trimmed.strip_prefix('-') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return format!("-{rest}n");
        }
    }
    format!("BigInt({trimmed})")
}

pub(super) fn ts_builder_expression(
    obj: &serde_json::Map<String, serde_json::Value>,
    type_name: &str,
    nested_types: &std::collections::HashMap<String, String>,
    lang: &str,
    enum_fields: &std::collections::HashMap<String, String>,
    bigint_fields: &std::collections::BTreeSet<String>,
    type_defs: &[TypeDef],
    enums: &[EnumDef],
    wasm_type_prefix: &str,
) -> String {
    ts_builder_expression_inner(
        obj,
        type_name,
        nested_types,
        lang,
        enum_fields,
        bigint_fields,
        type_defs,
        enums,
        wasm_type_prefix,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ts_builder_expression_inner(
    obj: &serde_json::Map<String, serde_json::Value>,
    type_name: &str,
    nested_types: &std::collections::HashMap<String, String>,
    lang: &str,
    enum_fields: &std::collections::HashMap<String, String>,
    bigint_fields: &std::collections::BTreeSet<String>,
    type_defs: &[TypeDef],
    enums: &[EnumDef],
    wasm_type_prefix: &str,
    depth: usize,
) -> String {
    // Use a depth-indexed variable name so nested IFEs don't shadow each other.
    // Without this, `const _u = WasmOptions.default(); _u.preprocessing =
    // (() => { const _u = WasmOptions.default(); ... })()` triggers
    // oxlint `no-shadow` on every nested-options expression.
    let var = format!("_u{depth}");
    if lang == "node" || (lang == "wasm" && is_tagged_data_enum(type_name, enums, wasm_type_prefix)) {
        // For node: if this type itself is a tagged-data enum, rename its serde_tag
        // key to "kind". The napi-rs backend hardcodes `#[napi(js_name = "kind")]`
        // for every tagged-data enum discriminant, regardless of the original
        // `#[serde(tag = "...")]` attribute. For wasm tagged-data enums the plain
        // JS object is deserialized via serde_wasm_bindgen which reads the original
        // serde_tag name, so the rename only applies to the node language path.
        let serde_tag_for_this_type = if lang == "node" {
            let ir_name = type_name.strip_prefix(wasm_type_prefix).unwrap_or(type_name);
            enums
                .iter()
                .find(|e| e.name == ir_name && e.serde_tag.is_some() && e.variants.iter().any(|v| !v.fields.is_empty()))
                .and_then(|e| e.serde_tag.as_deref())
        } else {
            None
        };

        let mut fields = Vec::new();
        for (key, val) in obj {
            // Rename serde_tag key → "kind" for node-bound tagged-data enum objects.
            let js_key = if lang == "node" {
                match serde_tag_for_this_type {
                    Some(tag) if key == tag => "kind".to_string(),
                    _ => snake_to_camel(key),
                }
            } else {
                snake_to_camel(key)
            };
            let field_expr = if lang == "node" {
                // Apply the napi serde_tag rename recursively into nested objects
                // and arrays so that tagged-enum elements inside arrays also get
                // their discriminant renamed to "kind".
                let preprocessed = rename_napi_serde_tags_to_kind(val, enums);
                // If the field is an enum (e.g. urlEscapeStyle, codeBlockStyle),
                // napi-rs constants are PascalCase variant names. Fixtures may
                // use the lowercase wire form (e.g. "percent"); convert it.
                let camel_key = snake_to_camel(key);
                let is_enum_field =
                    enum_fields.contains_key(key.as_str()) || enum_fields.contains_key(camel_key.as_str());
                if is_enum_field {
                    if let serde_json::Value::String(s) = &preprocessed {
                        format!("\"{}\"", escape_js(&s.to_upper_camel_case()))
                    } else {
                        json_to_js(&preprocessed)
                    }
                } else {
                    match val {
                        serde_json::Value::Object(_) => json_to_js_camel(&preprocessed),
                        _ => json_to_js(&preprocessed),
                    }
                }
            } else {
                match val {
                    serde_json::Value::Object(_) => json_to_js_camel(val),
                    _ => json_to_js(val),
                }
            };
            fields.push(format!("{js_key}: {field_expr}"));
        }
        let obj_literal = format!("{{ {} }}", fields.join(", "));
        return format!("{obj_literal} as {type_name}");
    }

    // WASM path: construct the main type via its synthetic `default()` static
    // factory rather than `new WasmFoo()`. wasm-bindgen's `(constructor)` mirrors
    // the Rust ctor's arity, so any struct with a non-Optional field requires
    // positional args — `new WasmChatCompletionTool()` (no args) throws
    // because `tool_type` and `function` are required. The `default()` factory
    // (emitted unconditionally on every wasm wrapper that derives `Default`)
    // returns a fresh instance the test body can then drive via setters.
    let init_stmt = if type_name.starts_with("Wasm") {
        format!("const {var} = {type_name}.default();")
    } else {
        format!("const {var} = new {type_name}();")
    };

    // Build derived nested_types from the IR registry and merge with the
    // explicit overrides (explicit wins on collision).
    let derived = derive_nested_types_for_wasm(type_name, type_defs, wasm_type_prefix);
    let effective_nested_types: std::collections::HashMap<String, String> = {
        let mut m = derived;
        for (k, v) in nested_types {
            m.insert(k.clone(), v.clone());
        }
        m
    };

    let mut stmts: Vec<String> = vec![init_stmt];
    for (key, val) in obj {
        let camel_key = snake_to_camel(key);
        let is_bigint = bigint_fields.contains(&camel_key) || bigint_fields.contains(key);
        if let serde_json::Value::Object(nested_obj) = val {
            if let Some(nested_type) = effective_nested_types.get(key.as_str()) {
                let nested_expr = ts_builder_expression_inner(
                    nested_obj,
                    nested_type,
                    nested_types,
                    lang,
                    enum_fields,
                    bigint_fields,
                    type_defs,
                    enums,
                    wasm_type_prefix,
                    depth + 1,
                );
                stmts.push(format!("{var}.{camel_key} = {nested_expr};"));
            } else {
                stmts.push(format!("{var}.{camel_key} = {};", json_to_js_camel(val)));
            }
        } else if let serde_json::Value::Array(items) = val {
            // wasm-bindgen rejects plain object literals where it expects class
            // instances. When the array element type is a known binding class
            // (registered in `effective_nested_types`), wrap each object element
            // via the same builder-expression emitter; primitive elements pass
            // through as JS literals.
            if let Some(elem_type) = effective_nested_types.get(key.as_str()) {
                let element_exprs: Vec<String> = items
                    .iter()
                    .map(|item| {
                        if let serde_json::Value::Object(item_obj) = item {
                            ts_builder_expression_inner(
                                item_obj,
                                elem_type,
                                nested_types,
                                lang,
                                enum_fields,
                                bigint_fields,
                                type_defs,
                                enums,
                                wasm_type_prefix,
                                depth + 1,
                            )
                        } else {
                            json_to_js(item)
                        }
                    })
                    .collect();
                stmts.push(format!("{var}.{camel_key} = [{}];", element_exprs.join(", ")));
            } else {
                stmts.push(format!("{var}.{camel_key} = {};", json_to_js(val)));
            }
        } else if let Some(enum_type) = enum_fields
            .get(key.as_str())
            .or_else(|| enum_fields.get(camel_key.as_str()))
        {
            // This is an enum field — generate EnumType.EnumValue.
            // Look up by both snake_case (fixture key) and camelCase (alef.toml override key
            // convention) so the alef.toml `enum_fields = { codeBlockStyle = "..." }` style
            // matches fixtures written with snake_case keys.
            if let serde_json::Value::String(s) = val {
                stmts.push(format!("{var}.{camel_key} = {enum_type}.{};", s.to_upper_camel_case()));
            } else {
                stmts.push(format!("{var}.{camel_key} = {};", json_to_js(val)));
            }
        } else if is_bigint {
            // wasm-bindgen u64/i64 setters require BigInt. Plain numeric
            // literals must be suffixed with `n`; non-literal numeric
            // values are wrapped in `BigInt(...)`.
            let raw = json_to_js(val);
            stmts.push(format!("{var}.{camel_key} = {};", to_bigint_literal(&raw)));
        } else {
            stmts.push(format!("{var}.{camel_key} = {};", json_to_js(val)));
        }
    }

    stmts.push(format!("return {var};"));
    let body = stmts.join(" ");
    format!("(() => {{ {body} }})()")
}
