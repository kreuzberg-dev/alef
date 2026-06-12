//! Python test function body rendering (non-HTTP fixtures).

mod args;
mod error_assertions;
mod result_assertions;
mod typed_values;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;

use crate::e2e::config::E2eConfig;
use crate::e2e::escape::{escape_python, sanitize_ident};
use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::Fixture;

use super::helpers::{is_skipped, resolve_client_factory, resolve_function_name_for_call};
use super::visitors::emit_python_visitor_method;
use args::build_args_and_setup;
use error_assertions::emit_error_assertion;
use result_assertions::emit_result_and_assertions;
pub(super) use typed_values::resolve_field_enum_type;

/// Render a pytest test function for a non-HTTP fixture.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_test_function(
    out: &mut String,
    fixture: &Fixture,
    e2e_config: &E2eConfig,
    config: &crate::core::config::ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
    enums: &[crate::core::ir::EnumDef],
    options_type: Option<&str>,
    options_via: &str,
    enum_fields: &HashMap<String, String>,
    handle_nested_types: &HashMap<String, String>,
    handle_dict_types: &HashSet<String>,
) {
    let fn_name = sanitize_ident(&fixture.id);
    let description = &fixture.description;
    let mut call_config = e2e_config.resolve_call_for_fixture(
        fixture.call.as_deref(),
        &fixture.id,
        &fixture.resolved_category(),
        &fixture.tags,
        &fixture.input,
    );
    // Fallback: if the resolved call has required args missing from input,
    // try to find a better-matching call from the named calls.
    call_config = super::super::select_best_matching_call(call_config, e2e_config, fixture);
    let call_field_resolver = FieldResolver::new(
        e2e_config.effective_fields(call_config),
        e2e_config.effective_fields_optional(call_config),
        e2e_config.effective_result_fields(call_config),
        e2e_config.effective_fields_array(call_config),
        &std::collections::HashSet::new(),
    );
    let field_resolver = &call_field_resolver;
    let function_name = resolve_function_name_for_call(call_config);
    let result_var = &call_config.result_var;

    let python_override = call_config.overrides.get("python");
    // `result_is_simple` is a Rust-side property of the call's return type and
    // applies identically to every binding. Read it from the call-level field
    // first (preferred), and only fall back to the per-language override for
    // backwards compatibility with fixtures that still declare it there.
    let result_is_simple = call_config.result_is_simple || python_override.is_some_and(|o| o.result_is_simple);

    // options_type: prefer per-call override, fall back to file-level python override, then call parameter.
    let top_level_options_type = e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.options_type.as_deref());
    let effective_options_type = python_override
        .and_then(|o| o.options_type.as_deref())
        .or(top_level_options_type)
        .or(options_type);

    // options_via: prefer per-call override, fall back to file-level python override, then call parameter.
    let top_level_options_via = e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.options_via.as_deref());
    let effective_options_via = python_override
        .and_then(|o| o.options_via.as_deref())
        .or(top_level_options_via)
        .unwrap_or(options_via);

    let desc_with_period = if description.ends_with('.') {
        description.to_string()
    } else {
        format!("{description}.")
    };

    let skip_decorator = if is_skipped(fixture, "python") {
        let reason = fixture
            .skip
            .as_ref()
            .and_then(|s| s.reason.as_deref())
            .unwrap_or("skipped for python");
        let escaped = escape_python(reason);
        format!("@pytest.mark.skip(reason=\"{escaped}\")\n")
    } else {
        String::new()
    };

    let has_error_assertion = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    // Streaming fixtures require async test functions so the async iterator
    // (ChatStreamIterator.__anext__) can be driven with `async for`.
    let is_streaming =
        crate::e2e::codegen::streaming_assertions::resolve_is_streaming(fixture, call_config.streaming_enabled());
    // Streaming error tests: when a streaming call (declared via `streaming = true` or
    // heuristically detected by function name containing "stream") expects an error,
    // the Python binding returns the iterator synchronously; errors only surface when
    // iterating via __anext__. Make the test async and drain the iterator inside
    // `pytest.raises` so the error propagates before the `with` block exits.
    //
    // Triggers in two cases:
    // - Declared streaming call (`call_config.streaming_enabled() = true`) + error fixture.
    // - Heuristic name-based detection (function name contains "stream") for
    //   fixtures that pre-date the explicit `streaming` flag.
    let is_streaming_error_call =
        has_error_assertion && (is_streaming || function_name.to_lowercase().contains("stream"));
    let is_async = is_streaming
        || is_streaming_error_call
        || python_override.and_then(|o| o.r#async).unwrap_or(call_config.r#async);
    let async_decorator = if is_async {
        "@pytest.mark.asyncio\n".to_string()
    } else {
        String::new()
    };
    let async_kw = if is_async { "async " } else { "" };

    let (arg_bindings, kwarg_exprs, teardown_block) = build_args_and_setup(
        fixture,
        call_config,
        effective_options_type,
        effective_options_via,
        enum_fields,
        handle_nested_types,
        handle_dict_types,
        config,
        type_defs,
        enums,
    );

    // Build visitor class if present
    let mut visitor_class = String::new();
    if let Some(visitor_spec) = &fixture.visitor {
        let _ = writeln!(visitor_class, "    class _TestVisitor:");
        for (method_name, action) in &visitor_spec.callbacks {
            emit_python_visitor_method(&mut visitor_class, method_name, action);
        }
    }

    // Build arg bindings string
    let arg_bindings_str = arg_bindings.iter().map(|b| format!("{b}\n")).collect::<String>();

    let call_args_str = {
        let mut exprs = kwarg_exprs.clone();
        if fixture.visitor.is_some() {
            exprs.push("visitor=_TestVisitor()".to_string());
        }
        exprs.join(", ")
    };
    // For streaming fixtures, chat_stream() is synchronous (block_on) and returns
    // the iterator directly — do NOT await it even though the test function is async
    // (the async is needed to drive `async for chunk in result`).
    let await_prefix = if is_async && !is_streaming { "await " } else { "" };

    // Client factory: when configured, create a client and dispatch as a method.
    // Fixtures with mock_response point the client at the mock server via base_url so
    // the fixture response is served via prefix routing.
    // Fixtures without mock_response (real-API smoke tests) use no base_url override.
    let client_factory = resolve_client_factory(e2e_config);
    let mut client_setup = String::new();
    let call_expr = if let Some(ref factory) = client_factory {
        let has_mock = fixture.mock_response.is_some() || fixture.http.is_some();
        let api_key_opt = fixture.env.as_ref().and_then(|e| e.api_key_var.as_deref());
        if let Some(api_key_var) = api_key_opt.filter(|_| has_mock) {
            let fixture_id = &fixture.id;
            let mock_base_url_expr = if fixture.has_host_root_route() {
                format!(
                    "os.environ.get(\"MOCK_SERVER_{}\") or os.environ[\"MOCK_SERVER_URL\"] + \"/fixtures/{fixture_id}\"",
                    fixture_id.to_uppercase()
                )
            } else {
                format!("os.environ[\"MOCK_SERVER_URL\"] + \"/fixtures/{fixture_id}\"")
            };
            let _ = writeln!(client_setup, "    api_key = os.environ.get(\"{api_key_var}\")");
            let _ = writeln!(client_setup, "    if api_key:");
            let _ = writeln!(
                client_setup,
                "        print(\"{fixture_id}: using real API ({api_key_var} is set)\", flush=True)  # noqa: T201"
            );
            let _ = writeln!(client_setup, "        client = {factory}(api_key=api_key)");
            let _ = writeln!(client_setup, "    else:");
            let _ = writeln!(
                client_setup,
                "        print(\"{fixture_id}: using mock server ({api_key_var} not set)\", flush=True)  # noqa: T201"
            );
            let _ = writeln!(
                client_setup,
                "        client = {factory}(api_key=\"test-key\", base_url={mock_base_url_expr})"
            );
        } else if has_mock {
            let fixture_id = &fixture.id;
            let base_url_expr = if fixture.has_host_root_route() {
                format!(
                    "os.environ.get(\"MOCK_SERVER_{}\") or os.environ[\"MOCK_SERVER_URL\"] + \"/fixtures/{fixture_id}\"",
                    fixture_id.to_uppercase()
                )
            } else {
                format!("os.environ[\"MOCK_SERVER_URL\"] + \"/fixtures/{fixture_id}\"")
            };
            let _ = writeln!(
                client_setup,
                "    client = {factory}(api_key=\"test-key\", base_url={base_url_expr})"
            );
        } else if let Some(api_key_var) = api_key_opt {
            let _ = writeln!(client_setup, "    api_key = os.environ.get(\"{api_key_var}\")");
            let _ = writeln!(client_setup, "    if not api_key:  # noqa: SIM102");
            let _ = writeln!(client_setup, "        pytest.skip(\"{api_key_var} not set\")");
            let _ = writeln!(client_setup, "    client = {factory}(api_key=api_key)");
        } else {
            let _ = writeln!(client_setup, "    client = {factory}(api_key=\"test-key\")");
        }
        format!("{await_prefix}client.{function_name}({call_args_str})")
    } else {
        format!("{await_prefix}{function_name}({call_args_str})")
    };
    // Prepend client setup to arg bindings so it lands inside the test function body.
    let arg_bindings_str = format!("{client_setup}{arg_bindings_str}");

    if has_error_assertion {
        // For error-assertion fixtures, the engine creation and other arg bindings
        // must happen INSIDE the `pytest.raises` block — otherwise validation
        // errors raised at engine-creation time fly past the assertion uncaught
        // and crash the test (e.g. `validation_max_depth_too_high` raises in
        // `create_engine(CrawlConfig(max_depth=200))` before the `await scrape(...)`
        // call ever runs). Pass arg_bindings_str to emit_error_assertion so it
        // can emit them indented one level deeper, inside the with block.
        let mut error_assertion_block = String::new();
        emit_error_assertion(
            &mut error_assertion_block,
            fixture,
            &arg_bindings_str,
            &call_expr,
            is_streaming_error_call,
        );

        let ctx = minijinja::context! {
            skip_decorator => skip_decorator,
            async_decorator => async_decorator,
            async_kw => async_kw,
            fn_name => fn_name,
            docstring => desc_with_period,
            visitor_class => visitor_class,
            arg_bindings => String::new(),
            call_expr => call_expr,
            is_error_assertion => true,
            error_assertion_block => error_assertion_block,
            result_assertions => String::new(),
        };
        let rendered = crate::e2e::template_env::render("python/test_function.jinja", ctx);
        out.push_str(&rendered);
        return;
    }

    // Build result and assertions
    let mut result_assertions = String::new();
    emit_result_and_assertions(
        &mut result_assertions,
        fixture,
        e2e_config,
        call_config,
        &call_expr,
        result_var,
        field_resolver,
        result_is_simple,
        is_streaming,
    );

    // Append trait-bridge teardown after assertions. This restores shared
    // global state (e.g. plugin registries) between pytest
    // tests in the same process. See `emit_test_backend` for the rationale.
    if !teardown_block.is_empty() {
        if !result_assertions.ends_with('\n') {
            result_assertions.push('\n');
        }
        result_assertions.push_str(&teardown_block);
    }

    let ctx = minijinja::context! {
        skip_decorator => skip_decorator,
        async_decorator => async_decorator,
        async_kw => async_kw,
        fn_name => fn_name,
        docstring => desc_with_period,
        visitor_class => visitor_class,
        arg_bindings => arg_bindings_str,
        call_expr => call_expr,
        is_error_assertion => false,
        error_assertion_block => String::new(),
        result_assertions => result_assertions,
    };
    let rendered = crate::e2e::template_env::render("python/test_function.jinja", ctx);
    out.push_str(&rendered);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_test_function_skipped_fixture_emits_skip_decorator() {
        use crate::e2e::fixture::{Fixture, SkipDirective};
        let fixture = Fixture {
            id: "skipped_test".to_string(),
            description: "A skipped test".to_string(),
            input: serde_json::Value::Null,
            http: None,
            assertions: Vec::new(),
            call: None,
            skip: Some(SkipDirective {
                languages: vec!["python".to_string()],
                reason: Some("not supported".to_string()),
            }),
            env: None,
            setup: Vec::new(),
            visitor: None,
            args: vec![],
            assertion_recipes: vec![],
            mock_response: None,
            source: String::new(),
            category: None,
            tags: Vec::new(),
        };
        let e2e_config = crate::e2e::config::E2eConfig::default();
        let config = crate::core::config::ResolvedCrateConfig::default();
        let type_defs: Vec<crate::core::ir::TypeDef> = Vec::new();
        let enums: Vec<crate::core::ir::EnumDef> = Vec::new();
        let mut out = String::new();
        render_test_function(
            &mut out,
            &fixture,
            &e2e_config,
            &config,
            &type_defs,
            &enums,
            None,
            "kwargs",
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        );
        assert!(out.contains("pytest.mark.skip"), "got: {out}");
        assert!(out.contains("not supported"), "got: {out}");
    }
}
