//! Result and streaming assertion rendering for generated Python tests.

use std::fmt::Write as FmtWrite;

use crate::e2e::config::E2eConfig;
use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::{Assertion, Fixture};

use super::super::assertions::render_assertion;
use super::super::helpers::resolve_assert_enum_fields;
use super::super::json::value_to_python_string;

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_result_and_assertions(
    out: &mut String,
    fixture: &Fixture,
    e2e_config: &E2eConfig,
    call_config: &crate::e2e::config::CallConfig,
    call_expr: &str,
    result_var: &str,
    field_resolver: &FieldResolver,
    result_is_simple: bool,
    is_streaming: bool,
) {
    // For streaming fixtures, streaming virtual fields are always usable
    // (they resolve against the collected `chunks` list, not the result type).
    let chunks_var = "chunks";
    let _ = fixture.assertions.iter().any(|a| {
        if a.assertion_type == "not_error" || a.assertion_type == "error" {
            return false;
        }
        if is_streaming {
            if let Some(f) = &a.field {
                if crate::e2e::codegen::streaming_assertions::is_streaming_virtual_field(f) {
                    return true;
                }
            }
        }
        if result_is_simple {
            if let Some(f) = &a.field {
                let f_lower = f.to_lowercase();
                if !f.is_empty()
                    && f_lower != "content"
                    && f_lower != "result"
                    && (f_lower.starts_with("metadata")
                        || f_lower.starts_with("document")
                        || f_lower.starts_with("structure")
                        || f_lower.starts_with("pages")
                        || f_lower.starts_with("chunks")
                        || f_lower.starts_with("tables")
                        || f_lower.starts_with("images")
                        || f_lower.starts_with("mime_type")
                        || f_lower.starts_with("is_")
                        || f_lower == "byte_length"
                        || f_lower == "page_count"
                        || f_lower == "output_format"
                        || f_lower == "extraction_method")
                {
                    return false;
                }
            }
            return true;
        }
        match &a.field {
            Some(f) if !f.is_empty() => field_resolver.is_valid_for_result(f),
            _ => true,
        }
    });

    let fields_enum = e2e_config.effective_fields_enum(call_config);
    let assert_enum_fields = resolve_assert_enum_fields(call_config);

    // For streaming fixtures: bind the raw iterator, then drain it into a list.
    // The Python ChatStreamIterator exposes __aiter__/__anext__ (async iterator),
    // so the test function must be `async def` and we use `async for` to drain.
    // Note: chat_stream() itself is NOT a coroutine in Python — it returns the
    // iterator synchronously (blocking on stream acquisition via block_on), so
    // no `await` prefix is used on the call expression.
    if is_streaming {
        let _ = writeln!(out, "    {result_var} = {call_expr}");
        if let Some(collect) = crate::e2e::codegen::streaming_assertions::StreamingFieldResolver::collect_snippet(
            "python", result_var, chunks_var,
        ) {
            let _ = writeln!(out, "    {collect}");
        }
        // Render streaming assertions using the collected chunks
        for assertion in &fixture.assertions {
            if assertion.assertion_type == "not_error" || assertion.assertion_type == "error" {
                continue;
            }
            if let Some(f) = &assertion.field {
                if crate::e2e::codegen::streaming_assertions::is_streaming_virtual_field(f) {
                    emit_streaming_virtual_assertion(out, assertion, f, chunks_var);
                    continue;
                }
            }
            // Non-streaming-virtual assertions on streaming fixtures are skipped
            // (the result type doesn't have these fields during iteration).
        }
    } else {
        // For non-streaming: render assertions to a temporary buffer first,
        // then check if result_var is referenced. Only emit the assignment if it is.
        let mut temp_assertions = String::new();

        for assertion in &fixture.assertions {
            if assertion.assertion_type == "not_error" {
                if !call_config.returns_result {
                    continue;
                }
                continue;
            }
            render_assertion(
                &mut temp_assertions,
                assertion,
                result_var,
                field_resolver,
                fields_enum,
                assert_enum_fields,
                result_is_simple,
            );
        }

        // Check if result_var appears in actual code (not in comments).
        // Only count lines that start with "assert" or contain actual code tokens.
        // Comments (lines starting with #) are skipped to avoid false positives
        // from strings like "field 'result' not available" in comment text.
        let result_var_used = temp_assertions.lines().any(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with('#') && trimmed.contains(result_var)
        });

        let py_result_var = if result_var_used {
            result_var.to_string()
        } else {
            "_".to_string()
        };
        let _ = writeln!(out, "    {py_result_var} = {call_expr}");
        out.push_str(&temp_assertions);
    }
}

/// Emit a Python assertion for a streaming virtual field using the collected
/// `chunks` list.  Mirrors the pattern in rust/assertions.rs.
fn emit_streaming_virtual_assertion(out: &mut String, assertion: &Assertion, field: &str, chunks_var: &str) {
    use crate::e2e::codegen::streaming_assertions::StreamingFieldResolver;

    let Some(expr) = StreamingFieldResolver::accessor(field, "python", chunks_var) else {
        let _ = writeln!(out, "    # skipped: streaming field '{field}': no python accessor");
        return;
    };

    match assertion.assertion_type.as_str() {
        "count_min" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    assert len({expr}) >= {n}  # noqa: S101");
                }
            }
        }
        "count_equals" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    assert len({expr}) == {n}  # noqa: S101");
                }
            }
        }
        "equals" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let op = if val.is_boolean() || val.is_null() { "is" } else { "==" };
                if val.is_string() {
                    let _ = writeln!(out, "    assert {expr}.strip() {op} {expected}.strip()  # noqa: S101");
                } else {
                    let _ = writeln!(out, "    assert {expr} {op} {expected}  # noqa: S101");
                }
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "    assert {expr}  # noqa: S101");
        }
        "is_empty" => {
            let _ = writeln!(out, "    assert not {expr}  # noqa: S101");
        }
        "is_true" => {
            // Normalize "true"/"false" literals to Python's True/False.
            let py_expr = if expr == "true" {
                "True".to_string()
            } else if expr == "false" {
                "False".to_string()
            } else {
                expr.clone()
            };
            let _ = writeln!(out, "    assert {py_expr}  # noqa: S101");
        }
        "is_false" => {
            let py_expr = if expr == "true" {
                "True".to_string()
            } else if expr == "false" {
                "False".to_string()
            } else {
                expr.clone()
            };
            let _ = writeln!(out, "    assert not {py_expr}  # noqa: S101");
        }
        "greater_than" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {expr} > {expected}  # noqa: S101");
            }
        }
        "greater_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {expr} >= {expected}  # noqa: S101");
            }
        }
        "contains" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {expected} in {expr}  # noqa: S101");
            }
        }
        _ => {
            let _ = writeln!(
                out,
                "    # skipped: streaming field '{field}': assertion type '{}' not rendered",
                assertion.assertion_type
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assertion(assertion_type: &str, field: Option<&str>, value: Option<serde_json::Value>) -> Assertion {
        Assertion {
            assertion_type: assertion_type.to_string(),
            field: field.map(str::to_string),
            value,
            values: None,
            method: None,
            check: None,
            args: None,
            return_type: None,
        }
    }

    #[test]
    fn streaming_virtual_assertion_renders_collected_chunks_access() {
        let mut out = String::new();
        let assertion = assertion("count_min", Some("chunks"), Some(serde_json::Value::from(1)));

        emit_streaming_virtual_assertion(&mut out, &assertion, "chunks", "chunks");

        assert!(out.contains("assert len(chunks) >= 1"), "got: {out}");
    }
}
