use super::helpers::resolve_module_for_call;
use super::*;

#[test]
fn resolve_module_for_call_prefers_crate_name_override() {
    use crate::e2e::config::CallConfig;
    use std::collections::HashMap;
    let mut overrides = HashMap::new();
    overrides.insert(
        "rust".to_string(),
        crate::e2e::config::CallOverride {
            crate_name: Some("custom_crate".to_string()),
            module: Some("ignored_module".to_string()),
            ..Default::default()
        },
    );
    let call = CallConfig {
        overrides,
        ..Default::default()
    };
    let result = resolve_module_for_call(&call, "dep_name");
    assert_eq!(result, "custom_crate");
}

/// Regression test: a non-streaming fixture whose result struct has a `chunks`
/// field (registered in `fields_array`) must emit `let chunks = &result.chunks;`
/// before any assertion so the streaming-virtual-field arm's hardcoded `chunks`
/// identifier resolves.  Without the fix this generated
/// `assert!(chunks.len() >= 2 as usize, ...)` with `chunks` undeclared.
#[test]
fn fields_array_binding_emitted_before_count_min_assertion_for_non_streaming_fixture() {
    use crate::e2e::config::{CallConfig, StreamingConfig};
    use crate::e2e::fixture::{Assertion, Fixture};
    use std::collections::HashSet;

    let mut fields_array = HashSet::new();
    fields_array.insert("chunks".to_string());

    let call = CallConfig {
        function: "process".to_string(),
        module: "my_crate".to_string(),
        result_var: "result".to_string(),
        fields_array,
        returns_result: true,
        streaming: Some(StreamingConfig::Enabled(false)),
        ..Default::default()
    };

    let e2e_config = crate::e2e::config::E2eConfig {
        call,
        ..Default::default()
    };

    let fixture = Fixture {
        id: "chunking_test".to_string(),
        description: "Chunking produces multiple pieces".to_string(),
        tags: Vec::new(),
        skip: None,
        env: None,
        setup: Vec::new(),
        call: None,
        input: serde_json::Value::Null,
        mock_response: None,
        visitor: None,
        args: vec![],
        assertion_recipes: vec![],
        assertions: vec![Assertion {
            assertion_type: "count_min".to_string(),
            field: Some("chunks".to_string()),
            value: Some(serde_json::Value::Number(serde_json::Number::from(2u64))),
            values: None,
            method: None,
            check: None,
            args: None,
            return_type: None,
        }],
        source: String::new(),
        http: None,
        category: None,
    };

    let mut out = String::new();
    let cfg: crate::core::config::NewAlefConfig = toml::from_str(
        "[workspace]\nlanguages = [\"rust\"]\n[[crates]]\nname = \"my_crate\"\nsources = [\"src/lib.rs\"]\n",
    )
    .unwrap();
    let test_config = cfg.resolve().unwrap().remove(0);
    render_test_function(&mut out, &fixture, &e2e_config, &test_config, &[], "my_crate", None);

    assert!(
        out.contains("let chunks = &result.chunks"),
        "expected `let chunks = &result.chunks` binding before assertion; got:\n{out}"
    );
    assert!(
        out.contains("chunks.len() >= 2"),
        "expected count_min assertion referencing `chunks`; got:\n{out}"
    );
    // The binding must appear before the assertion in the output.
    let binding_pos = out.find("let chunks = &result.chunks").unwrap();
    let assert_pos = out.find("chunks.len() >= 2").unwrap();
    assert!(
        binding_pos < assert_pos,
        "binding must appear before assertion; got:\n{out}"
    );
}

/// Regression test: a `result_is_simple` call with a `count_equals` assertion whose
/// `field` is NOT a real field on the (plain Vec) result type must still bind the
/// call to the result variable.  The assertion renderer emits `result.len()` for
/// `result_is_simple` calls regardless of the field, so binding to `_` would leave
/// `result` undefined.
#[test]
fn result_is_simple_count_assertion_binds_to_result_variable() {
    use crate::e2e::config::{CallConfig, StreamingConfig};
    use crate::e2e::fixture::{Assertion, Fixture};

    let call = CallConfig {
        function: "embed_texts".to_string(),
        module: "my_crate".to_string(),
        result_var: "result".to_string(),
        result_is_simple: true,
        returns_result: true,
        streaming: Some(StreamingConfig::Enabled(false)),
        ..Default::default()
    };

    let e2e_config = crate::e2e::config::E2eConfig {
        call,
        ..Default::default()
    };

    let fixture = Fixture {
        id: "embed_empty".to_string(),
        description: "embed_texts: empty input".to_string(),
        tags: Vec::new(),
        skip: None,
        env: None,
        setup: Vec::new(),
        call: None,
        input: serde_json::Value::Null,
        mock_response: None,
        visitor: None,
        args: vec![],
        assertion_recipes: vec![],
        assertions: vec![
            Assertion {
                assertion_type: "not_error".to_string(),
                field: None,
                value: None,
                values: None,
                method: None,
                check: None,
                args: None,
                return_type: None,
            },
            Assertion {
                assertion_type: "count_equals".to_string(),
                field: Some("embeddings".to_string()),
                value: Some(serde_json::Value::Number(serde_json::Number::from(0u64))),
                values: None,
                method: None,
                check: None,
                args: None,
                return_type: None,
            },
        ],
        source: String::new(),
        http: None,
        category: None,
    };

    let mut out = String::new();
    let cfg: crate::core::config::NewAlefConfig = toml::from_str(
        "[workspace]\nlanguages = [\"rust\"]\n[[crates]]\nname = \"my_crate\"\nsources = [\"src/lib.rs\"]\n",
    )
    .unwrap();
    let test_config = cfg.resolve().unwrap().remove(0);
    render_test_function(&mut out, &fixture, &e2e_config, &test_config, &[], "my_crate", None);

    assert!(
        out.contains("let result = embed_texts"),
        "expected the call to bind to `result`, not `_`; got:\n{out}"
    );
    assert!(
        out.contains("assert_eq!(result.len(), 0"),
        "expected `count_equals` assertion to render `result.len()`; got:\n{out}"
    );
    assert!(
        !out.contains("let _ = embed_texts"),
        "call must not bind to `_` when an assertion references the result; got:\n{out}"
    );
}

#[test]
fn handle_config_import_uses_resolved_options_type() {
    use crate::e2e::config::{ArgMapping, CallConfig, CallOverride};
    use crate::e2e::fixture::Fixture;
    use std::collections::HashMap;

    let mut overrides = HashMap::new();
    overrides.insert(
        "rust".to_string(),
        CallOverride {
            options_type: Some("SessionConfig".to_string()),
            ..Default::default()
        },
    );
    let call = CallConfig {
        function: "run_session".to_string(),
        module: "my_crate".to_string(),
        result_var: "result".to_string(),
        returns_result: false,
        args: vec![ArgMapping {
            name: "session".to_string(),
            field: "input.config".to_string(),
            arg_type: "handle".to_string(),
            optional: false,
            owned: false,
            element_type: None,
            go_type: None,
            vec_inner_is_ref: false,
            trait_name: None,
        }],
        overrides,
        ..Default::default()
    };
    let e2e_config = crate::e2e::config::E2eConfig {
        call,
        ..Default::default()
    };
    let fixture = Fixture {
        id: "session_fixture".to_string(),
        description: "session fixture".to_string(),
        tags: Vec::new(),
        skip: None,
        env: None,
        setup: Vec::new(),
        call: None,
        input: serde_json::json!({ "config": { "limit": 3 } }),
        mock_response: None,
        visitor: None,
        args: vec![],
        assertion_recipes: vec![],
        assertions: vec![],
        source: String::new(),
        http: None,
        category: Some("sessions".to_string()),
    };
    let cfg: crate::core::config::NewAlefConfig = toml::from_str(
        "[workspace]\nlanguages = [\"rust\"]\n[[crates]]\nname = \"my_crate\"\nsources = [\"src/lib.rs\"]\n",
    )
    .unwrap();
    let test_config = cfg.resolve().unwrap().remove(0);
    let out = render_test_file(
        "sessions",
        &[&fixture],
        &e2e_config,
        &test_config,
        &[],
        "my_crate",
        false,
    );

    assert!(
        out.contains("use my_crate::SessionConfig;"),
        "expected SessionConfig import, got:\n{out}"
    );
    assert!(out.contains("let session_config: SessionConfig = serde_json::from_str"));
    assert!(!out.contains("CrawlConfig"));
}
