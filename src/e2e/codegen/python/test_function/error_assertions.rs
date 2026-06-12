//! Error assertion rendering for generated Python tests.

use std::fmt::Write as FmtWrite;

use crate::e2e::escape::escape_python;
use crate::e2e::fixture::Fixture;

pub(super) fn emit_error_assertion(
    out: &mut String,
    fixture: &Fixture,
    arg_bindings_str: &str,
    call_expr: &str,
    is_streaming_error_call: bool,
) {
    let error_assertion = fixture.assertions.iter().find(|a| a.assertion_type == "error");
    let has_message = error_assertion
        .and_then(|a| a.value.as_ref())
        .and_then(|v| v.as_str())
        .is_some();

    // Re-indent arg_bindings by an extra 4 spaces so they land inside the `with`
    // block. arg_bindings already begin with 4 spaces (function-body level);
    // prepending 4 more puts them at the with-body level (8 spaces).
    let indented_bindings: String = arg_bindings_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| format!("    {l}\n"))
        .collect();

    if has_message {
        let _ = writeln!(out, "    with pytest.raises(Exception) as exc_info:  # noqa: B017");
        out.push_str(&indented_bindings);
        if is_streaming_error_call {
            // The streaming iterator returns synchronously (chat_stream returns the
            // iterator without await); errors only appear when iterating via
            // __anext__. Strip the `await ` prefix the async-call codegen would
            // attach, then drain the iterator inside the raises block so the
            // exception propagates before the with-block exits.
            let sync_call_expr = call_expr.strip_prefix("await ").unwrap_or(call_expr);
            let _ = writeln!(out, "        _iterator = {sync_call_expr}");
            let _ = writeln!(out, "        async for _ in _iterator:");
            let _ = writeln!(out, "            pass");
        } else {
            let _ = writeln!(out, "        {call_expr}");
        }
        if let Some(msg) = error_assertion.and_then(|a| a.value.as_ref()).and_then(|v| v.as_str()) {
            let escaped = escape_python(msg);
            // Match against EITHER the rendered exception message OR the
            // exception class name. Different crates use different
            // fixture-shape conventions:
            //   * config-validation fixtures may use field names that are substrings
            //     of the user-facing error message, never of a class name.
            //   * API-error fixtures may use class-name prefixes such as
            //     `Authentication`, `BadRequest`, or `ContentPolicy`.
            //     `BadRequestError`, `ContentPolicyError`), not message text.
            // The disjunction lets a single codegen path satisfy both.
            let _ = writeln!(
                out,
                "    assert \"{escaped}\" in str(exc_info.value) or \"{escaped}\" in type(exc_info.value).__name__  # noqa: S101"
            );
        }
    } else {
        let _ = writeln!(out, "    with pytest.raises(Exception):  # noqa: B017");
        out.push_str(&indented_bindings);
        if is_streaming_error_call {
            let _ = writeln!(out, "        _iterator = {call_expr}");
            let _ = writeln!(out, "        async for _ in _iterator:");
            let _ = writeln!(out, "            pass");
        } else {
            let _ = writeln!(out, "        {call_expr}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_with_error(value: Option<serde_json::Value>) -> Fixture {
        Fixture {
            id: "streaming_error".to_string(),
            description: "streaming error".to_string(),
            input: serde_json::Value::Null,
            http: None,
            assertions: vec![crate::e2e::fixture::Assertion {
                assertion_type: "error".to_string(),
                field: None,
                value,
                values: None,
                method: None,
                check: None,
                args: None,
                return_type: None,
            }],
            call: None,
            skip: None,
            env: None,
            setup: Vec::new(),
            visitor: None,
            args: Vec::new(),
            assertion_recipes: Vec::new(),
            mock_response: None,
            source: String::new(),
            category: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn streaming_error_assertion_drains_iterator_inside_raises() {
        let fixture = fixture_with_error(Some(serde_json::Value::String("BadRequest".to_string())));
        let mut out = String::new();

        emit_error_assertion(
            &mut out,
            &fixture,
            "    payload = {}\n",
            "await client.chat_stream(payload)",
            true,
        );

        assert!(out.contains("with pytest.raises(Exception) as exc_info"), "got: {out}");
        assert!(out.contains("        payload = {}"), "got: {out}");
        assert!(
            out.contains("        _iterator = client.chat_stream(payload)"),
            "got: {out}"
        );
        assert!(out.contains("        async for _ in _iterator:"), "got: {out}");
        assert!(out.contains("BadRequest"), "got: {out}");
    }

    #[test]
    fn plain_error_assertion_emits_call_inside_raises() {
        let fixture = fixture_with_error(None);
        let mut out = String::new();

        emit_error_assertion(
            &mut out,
            &fixture,
            "    payload = {}\n",
            "client.create(payload)",
            false,
        );

        assert!(out.contains("with pytest.raises(Exception):"), "got: {out}");
        assert!(out.contains("        payload = {}"), "got: {out}");
        assert!(out.contains("        client.create(payload)"), "got: {out}");
        assert!(!out.contains("async for _ in _iterator"), "got: {out}");
    }
}
