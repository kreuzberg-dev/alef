//! R e2e assertion rendering.

use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::Assertion;
use std::fmt::Write as FmtWrite;

use super::values::json_to_r;

pub(super) struct RAssertionContext<'a> {
    pub(super) field_resolver: &'a FieldResolver,
    pub(super) result_is_simple: bool,
    pub(super) result_is_bytes: bool,
    pub(super) assert_enum_fields: &'a std::collections::HashMap<String, String>,
}

pub(super) fn render_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_var: &str,
    context: &RAssertionContext<'_>,
) {
    // Handle synthetic / derived fields before the is_valid_for_result check
    // so they are never treated as struct attribute accesses on the result.
    if let Some(f) = &assertion.field {
        match f.as_str() {
            "chunks_have_content" => {
                let pred = format!("all(sapply({result_var}$chunks %||% list(), function(c) nchar(c$content) > 0))");
                match assertion.assertion_type.as_str() {
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({pred})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_false({pred})");
                    }
                    _ => {
                        let _ = writeln!(out, "  # skipped: unsupported assertion type on synthetic field '{f}'");
                    }
                }
                return;
            }
            "chunks_have_embeddings" => {
                let pred = format!(
                    "all(sapply({result_var}$chunks %||% list(), function(c) !is.null(c$embedding) && length(c$embedding) > 0))"
                );
                match assertion.assertion_type.as_str() {
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({pred})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_false({pred})");
                    }
                    _ => {
                        let _ = writeln!(out, "  # skipped: unsupported assertion type on synthetic field '{f}'");
                    }
                }
                return;
            }
            "chunks_have_heading_context" => {
                // prepend_heading_context adds heading text to chunk content, so verify chunks
                // exist and every chunk has non-empty content.
                let pred_true = format!(
                    "!is.null({result_var}$chunks) && length({result_var}$chunks) > 0 && all(sapply({result_var}$chunks, function(c) nchar(c$content) > 0))"
                );
                let pred_false = format!("is.null({result_var}$chunks) || length({result_var}$chunks) == 0");
                match assertion.assertion_type.as_str() {
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({pred_true})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_true({pred_false})");
                    }
                    _ => {
                        let _ = writeln!(out, "  # skipped: unsupported assertion type on synthetic field '{f}'");
                    }
                }
                return;
            }
            "first_chunk_starts_with_heading" => {
                // First chunk's content should start with a markdown heading marker (`#`)
                // when prepend_heading_context is enabled.
                let pred_true = format!(
                    "!is.null({result_var}$chunks) && length({result_var}$chunks) > 0 && startsWith(trimws({result_var}$chunks[[1]]$content), \"#\")"
                );
                let pred_false = format!(
                    "is.null({result_var}$chunks) || length({result_var}$chunks) == 0 || !startsWith(trimws({result_var}$chunks[[1]]$content), \"#\")"
                );
                match assertion.assertion_type.as_str() {
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({pred_true})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_true({pred_false})");
                    }
                    _ => {
                        let _ = writeln!(out, "  # skipped: unsupported assertion type on synthetic field '{f}'");
                    }
                }
                return;
            }
            // ---- EmbedResponse virtual fields ----
            // The extendr binding cannot return `Vec<Vec<f32>>` directly (extendr's
            // Robj conversion has no impl for nested numeric vectors), so the
            // wrapper serializes the result to a JSON string at the FFI boundary.
            // Parse it on demand here so length/index assertions operate on the
            // matrix structure rather than on the single string scalar.
            "embeddings" => {
                let parsed = format!(
                    "(if (is.character({result_var}) && length({result_var}) == 1) jsonlite::fromJSON({result_var}, simplifyVector = FALSE) else {result_var})"
                );
                match assertion.assertion_type.as_str() {
                    "count_equals" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_equal(length({parsed}), {r_val})");
                        }
                    }
                    "count_min" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_gte(length({parsed}), {r_val})");
                        }
                    }
                    "not_empty" => {
                        let _ = writeln!(out, "  expect_gt(length({parsed}), 0)");
                    }
                    "is_empty" => {
                        let _ = writeln!(out, "  expect_equal(length({parsed}), 0)");
                    }
                    _ => {
                        let _ = writeln!(
                            out,
                            "  # skipped: unsupported assertion type on synthetic field 'embeddings'"
                        );
                    }
                }
                return;
            }
            "embedding_dimensions" => {
                let expr = format!("(if (length({result_var}) == 0) 0L else length({result_var}[[1]]))");
                match assertion.assertion_type.as_str() {
                    "equals" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_equal({expr}, {r_val})");
                        }
                    }
                    "greater_than" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_gt({expr}, {r_val})");
                        }
                    }
                    _ => {
                        let _ = writeln!(
                            out,
                            "  # skipped: unsupported assertion type on synthetic field 'embedding_dimensions'"
                        );
                    }
                }
                return;
            }
            "embeddings_valid" | "embeddings_finite" | "embeddings_non_zero" | "embeddings_normalized" => {
                let pred = match f.as_str() {
                    "embeddings_valid" => {
                        format!("all(sapply({result_var}, function(e) length(e) > 0))")
                    }
                    "embeddings_finite" => {
                        format!("all(sapply({result_var}, function(e) all(is.finite(e))))")
                    }
                    "embeddings_non_zero" => {
                        format!("all(sapply({result_var}, function(e) any(e != 0.0)))")
                    }
                    "embeddings_normalized" => {
                        format!("all(sapply({result_var}, function(e) abs(sum(e * e) - 1.0) < 1e-3))")
                    }
                    _ => unreachable!(),
                };
                match assertion.assertion_type.as_str() {
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({pred})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_false({pred})");
                    }
                    _ => {
                        let _ = writeln!(out, "  # skipped: unsupported assertion type on synthetic field '{f}'");
                    }
                }
                return;
            }
            // ---- keywords / keywords_count ----
            // R ProcessingResult does not expose result_keywords; skip.
            "keywords" | "keywords_count" => {
                let _ = writeln!(out, "  # skipped: field '{f}' not available on R ProcessingResult");
                return;
            }
            _ => {}
        }
    }

    // Skip assertions on fields that don't exist on the result type.
    // Exception: for result_is_simple, "result" is valid because it refers to the
    // result variable directly (which holds the plain string/array value).
    if let Some(f) = &assertion.field {
        if !f.is_empty() && !context.field_resolver.is_valid_for_result(f) {
            // Allow "result" field on simple-type returns
            if !(context.result_is_simple && f == "result") {
                let _ = writeln!(out, "  # skipped: field '{f}' not available on result type");
                return;
            }
        }
    }

    // When result_is_simple, skip assertions that reference non-content fields
    // (e.g., metadata, document, structure) since the binding returns a plain value.
    if context.result_is_simple {
        if let Some(f) = &assertion.field {
            let f_lower = f.to_lowercase();
            if !f.is_empty()
                && f_lower != "content"
                && (f_lower.starts_with("metadata")
                    || f_lower.starts_with("document")
                    || f_lower.starts_with("structure"))
            {
                let _ = writeln!(
                    out,
                    "  # skipped: result_is_simple for field '{f}' not available on result type"
                );
                return;
            }
        }
    }

    let field_expr = if context.result_is_simple {
        result_var.to_string()
    } else {
        match &assertion.field {
            Some(f) if !f.is_empty() => context.field_resolver.accessor(f, "r", result_var),
            _ => result_var.to_string(),
        }
    };

    // Fields declared in `assert_enum_fields` map to sealed/internally-tagged enum
    // types.  Under `simplifyVector = FALSE`, such fields deserialize as named lists
    // keyed by the active variant.  Wrap the accessor with `.alef_format_value`
    // (defined in setup-fixtures.R) so the assertion sees the display string rather
    // than the raw list structure.
    let field_expr = match &assertion.field {
        Some(f) if context.assert_enum_fields.contains_key(f.as_str()) => {
            format!(".alef_format_value({field_expr})")
        }
        _ => field_expr,
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_equal(trimws({field_expr}), {r_val})");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_true(grepl({r_val}, {field_expr}, fixed = TRUE))");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let r_val = json_to_r(val, false);
                    let _ = writeln!(out, "  expect_true(any(grepl({r_val}, {field_expr}, fixed = TRUE)))");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_false(grepl({r_val}, {field_expr}, fixed = TRUE))");
            }
        }
        "not_empty" => {
            // Multi-element character vectors (e.g. `list_embedding_presets`)
            // would otherwise evaluate `nchar(x) > 0` element-wise and fail
            // `expect_true`'s scalar-logical contract. Reduce with `any()` so
            // the predicate stays a single TRUE/FALSE regardless of length,
            // and treat zero-length vectors as empty.
            let _ = writeln!(
                out,
                "  expect_true(if (is.character({field_expr})) length({field_expr}) > 0 && any(nchar({field_expr}) > 0) else length({field_expr}) > 0)"
            );
        }
        "is_empty" => {
            // Rust `Option<String>::None` surfaces as `NA_character_` through
            // extendr, and `Vec<...>` empties as a zero-length vector. Treat
            // NULL, NA, "", and zero-length collections as "empty" so the same
            // assertion works for scalar Option returns (`get_embedding_preset`)
            // and collection returns alike.
            let _ = writeln!(
                out,
                "  expect_true(is.null({field_expr}) || length({field_expr}) == 0 || (length({field_expr}) == 1 && (is.na({field_expr}) || identical({field_expr}, \"\"))))"
            );
        }
        "contains_any" => {
            if let Some(values) = &assertion.values {
                let items: Vec<String> = values.iter().map(|v| json_to_r(v, false)).collect();
                let vec_str = items.join(", ");
                let _ = writeln!(
                    out,
                    "  expect_true(any(sapply(c({vec_str}), function(v) grepl(v, {field_expr}, fixed = TRUE))))"
                );
            }
        }
        "greater_than" => {
            if let Some(val) = &assertion.value {
                let r_val = json_to_r(val, false);
                let _ = writeln!(out, "  expect_true({field_expr} > {r_val})");
            }
        }
        "less_than" => {
            if let Some(val) = &assertion.value {
                let r_val = json_to_r(val, false);
                let _ = writeln!(out, "  expect_true({field_expr} < {r_val})");
            }
        }
        "greater_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let r_val = json_to_r(val, false);
                let _ = writeln!(out, "  expect_true({field_expr} >= {r_val})");
            }
        }
        "less_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let r_val = json_to_r(val, false);
                let _ = writeln!(out, "  expect_true({field_expr} <= {r_val})");
            }
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_true(startsWith({field_expr}, {r_val}))");
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_true(endsWith({field_expr}, {r_val}))");
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    // Raw byte returns (`result_is_bytes`) come back as an R
                    // raw vector; `nchar()` element-wises and breaks the
                    // expect_true scalar contract. Use `length()` to compare
                    // the byte count instead.
                    let size_fn = if context.result_is_bytes { "length" } else { "nchar" };
                    let _ = writeln!(out, "  expect_true({size_fn}({field_expr}) >= {n})");
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let size_fn = if context.result_is_bytes { "length" } else { "nchar" };
                    let _ = writeln!(out, "  expect_true({size_fn}({field_expr}) <= {n})");
                }
            }
        }
        "count_min" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "  expect_true(length({field_expr}) >= {n})");
                }
            }
        }
        "count_equals" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "  expect_equal(length({field_expr}), {n})");
                }
            }
        }
        "is_true" => {
            let _ = writeln!(out, "  expect_true({field_expr})");
        }
        "is_false" => {
            let _ = writeln!(out, "  expect_false({field_expr})");
        }
        "method_result" => {
            if let Some(method_name) = &assertion.method {
                let call_expr = build_r_method_call(result_var, method_name, assertion.args.as_ref());
                let check = assertion.check.as_deref().unwrap_or("is_true");
                match check {
                    "equals" => {
                        if let Some(val) = &assertion.value {
                            if val.is_boolean() {
                                if val.as_bool() == Some(true) {
                                    let _ = writeln!(out, "  expect_true({call_expr})");
                                } else {
                                    let _ = writeln!(out, "  expect_false({call_expr})");
                                }
                            } else {
                                let r_val = json_to_r(val, false);
                                let _ = writeln!(out, "  expect_equal({call_expr}, {r_val})");
                            }
                        }
                    }
                    "is_true" => {
                        let _ = writeln!(out, "  expect_true({call_expr})");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "  expect_false({call_expr})");
                    }
                    "greater_than_or_equal" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_true({call_expr} >= {r_val})");
                        }
                    }
                    "count_min" => {
                        if let Some(val) = &assertion.value {
                            let n = val.as_u64().unwrap_or(0);
                            let _ = writeln!(out, "  expect_true(length({call_expr}) >= {n})");
                        }
                    }
                    "is_error" => {
                        let _ = writeln!(out, "  expect_error({call_expr})");
                    }
                    "contains" => {
                        if let Some(val) = &assertion.value {
                            let r_val = json_to_r(val, false);
                            let _ = writeln!(out, "  expect_true(grepl({r_val}, {call_expr}, fixed = TRUE))");
                        }
                    }
                    other_check => {
                        panic!("R e2e generator: unsupported method_result check type: {other_check}");
                    }
                }
            } else {
                panic!("R e2e generator: method_result assertion missing 'method' field");
            }
        }
        "matches_regex" => {
            if let Some(expected) = &assertion.value {
                let r_val = json_to_r(expected, false);
                let _ = writeln!(out, "  expect_true(grepl({r_val}, {field_expr}))");
            }
        }
        "not_error" => {
            // The call itself stops the test on error; emit an explicit
            // `expect_true(TRUE)` so testthat doesn't report the test as
            // empty when this is the only assertion.
            let _ = writeln!(out, "  expect_true(TRUE)");
        }
        "error" => {
            // Handled at the test level.
        }
        other => {
            panic!("R e2e generator: unsupported assertion type: {other}");
        }
    }
}

/// Build an R call expression for a `method_result` assertion.
/// Maps method names to the appropriate R function or method calls.
fn build_r_method_call(result_var: &str, method_name: &str, args: Option<&serde_json::Value>) -> String {
    match method_name {
        "root_child_count" => format!("{result_var}$root_child_count()"),
        "root_node_type" => format!("{result_var}$root_node_type()"),
        "named_children_count" => format!("{result_var}$named_children_count()"),
        "has_error_nodes" => format!("tree_has_error_nodes({result_var})"),
        "error_count" | "tree_error_count" => format!("tree_error_count({result_var})"),
        "tree_to_sexp" => format!("tree_to_sexp({result_var})"),
        "contains_node_type" => {
            let node_type = args
                .and_then(|a| a.get("node_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("tree_contains_node_type({result_var}, \"{node_type}\")")
        }
        "find_nodes_by_type" => {
            let node_type = args
                .and_then(|a| a.get("node_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("find_nodes_by_type({result_var}, \"{node_type}\")")
        }
        "run_query" => {
            let query_source = args
                .and_then(|a| a.get("query_source"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let language = args
                .and_then(|a| a.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("run_query({result_var}, \"{language}\", \"{query_source}\", source)")
        }
        _ => {
            if let Some(args_val) = args {
                let arg_str = args_val
                    .as_object()
                    .map(|obj| {
                        obj.iter()
                            .map(|(k, v)| {
                                let r_val = json_to_r(v, false);
                                format!("{k} = {r_val}")
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!("{result_var}${method_name}({arg_str})")
            } else {
                format!("{result_var}${method_name}()")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RAssertionContext, render_assertion};
    use crate::e2e::field_access::FieldResolver;
    use crate::e2e::fixture::Assertion;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn render_simple_result_contains_assertion() {
        let resolver = FieldResolver::new(
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        let enum_fields = HashMap::new();
        let assertion = Assertion {
            assertion_type: "contains".to_string(),
            field: Some("result".to_string()),
            value: Some(json!("needle")),
            ..Assertion::default()
        };
        let context = RAssertionContext {
            field_resolver: &resolver,
            result_is_simple: true,
            result_is_bytes: false,
            assert_enum_fields: &enum_fields,
        };
        let mut out = String::new();

        render_assertion(&mut out, &assertion, "result", &context);

        assert_eq!(out, "  expect_true(grepl(\"needle\", result, fixed = TRUE))\n");
    }

    #[test]
    fn render_bytes_min_length_uses_length_not_nchar() {
        let resolver = FieldResolver::new(
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        let enum_fields = HashMap::new();
        let assertion = Assertion {
            assertion_type: "min_length".to_string(),
            value: Some(json!(4)),
            ..Assertion::default()
        };
        let context = RAssertionContext {
            field_resolver: &resolver,
            result_is_simple: true,
            result_is_bytes: true,
            assert_enum_fields: &enum_fields,
        };
        let mut out = String::new();

        render_assertion(&mut out, &assertion, "result", &context);

        assert_eq!(out, "  expect_true(length(result) >= 4)\n");
    }
}
