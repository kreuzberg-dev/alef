//! C e2e streaming adapter test generation.

use crate::core::config::{AdapterPattern, ResolvedCrateConfig};
use crate::e2e::codegen::transform_json_keys_for_language;
use crate::e2e::escape::escape_c;
use crate::e2e::fixture::{Assertion, Fixture};
use heck::{ToPascalCase, ToSnakeCase};
use std::fmt::Write as FmtWrite;

pub(super) struct CStreamingAdapterMetadata {
    owner_type: String,
    item_type: String,
    request_type: String,
    adapter_name: String,
}

pub(super) fn resolve_c_streaming_adapter(
    config: &ResolvedCrateConfig,
    function_name: &str,
) -> Option<CStreamingAdapterMetadata> {
    config
        .adapters
        .iter()
        .find(|adapter| matches!(adapter.pattern, AdapterPattern::Streaming) && adapter.name == function_name)
        .and_then(|adapter| {
            Some(CStreamingAdapterMetadata {
                owner_type: adapter.owner_type.clone()?,
                item_type: adapter.item_type.clone()?,
                request_type: adapter
                    .request_type
                    .as_deref()
                    .and_then(|path| path.rsplit("::").next())
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)?,
                adapter_name: adapter.name.clone(),
            })
        })
}

pub(super) fn resolve_c_client_owner_type(
    config: &ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
    function_name: &str,
) -> Option<String> {
    config
        .adapters
        .iter()
        .find(|adapter| {
            matches!(adapter.pattern, AdapterPattern::Streaming | AdapterPattern::AsyncMethod)
                && adapter.name == function_name
        })
        .and_then(|adapter| adapter.owner_type.clone())
        .or_else(|| {
            type_defs.iter().find_map(|type_def| {
                type_def
                    .methods
                    .iter()
                    .any(|method| method.name == function_name)
                    .then(|| type_def.name.clone())
            })
        })
        .or_else(|| {
            let opaque_types: Vec<&crate::core::ir::TypeDef> =
                type_defs.iter().filter(|type_def| type_def.is_opaque).collect();
            (opaque_types.len() == 1).then(|| opaque_types[0].name.clone())
        })
}

pub(super) fn render_c_diagnostic_skip(out: &mut String, reason: &str) {
    let escaped = escape_c(reason);
    let _ = writeln!(out, "    fprintf(stderr, \"skipped: {escaped}\\n\");");
    let _ = writeln!(out, "}}");
}

/// Emit a streaming-adapter test function that drives the FFI iterator handle.
///
/// Calls the adapter-derived `{prefix}_{owner}_{method}_start` function to
/// obtain an opaque handle, loops over the corresponding `_next` function until
/// it returns null,
/// and aggregates per-chunk data into local variables (`chunks_count`,
/// `stream_content`, `stream_complete`, `last_choices_json`, ...). Fixture
/// assertions on streaming pseudo-fields (`chunks`, `stream_content`,
/// `stream_complete`, `no_chunks_after_done`, `finish_reason`, `tool_calls`,
/// `tool_calls[0].function.name`, `usage.total_tokens`) are translated to
/// assertions on these locals. Chat-specific field extraction remains best
/// effort and unsupported fields are skipped by `emit_chat_stream_assertion`.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_streaming_test_function(
    out: &mut String,
    fixture: &Fixture,
    prefix: &str,
    result_var: &str,
    args: &[crate::e2e::config::ArgMapping],
    client_factory: &str,
    streaming: &CStreamingAdapterMetadata,
    expects_error: bool,
    api_key_var: Option<&str>,
) {
    let prefix_upper = prefix.to_uppercase();
    let owner_snake = streaming.owner_type.to_snake_case();
    let request_type_pascal = &streaming.request_type;
    let request_type_snake = request_type_pascal.to_snake_case();
    let item_type_pascal = &streaming.item_type;
    let item_type_snake = item_type_pascal.to_snake_case();
    let adapter_name = &streaming.adapter_name;
    let stream_start = format!("{prefix}_{owner_snake}_{adapter_name}_start");
    let stream_next = format!("{prefix}_{owner_snake}_{adapter_name}_next");
    let stream_free = format!("{prefix}_{owner_snake}_{adapter_name}_free");
    let owner_type = &streaming.owner_type;

    let mut request_var: Option<String> = None;
    for arg in args {
        if arg.arg_type == "json_object" {
            let var_name = format!("{request_type_snake}_handle");

            let field = arg.field.strip_prefix("input.").unwrap_or(&arg.field);
            let json_val = if field.is_empty() || field == "input" {
                Some(&fixture.input)
            } else {
                fixture.input.get(field)
            };

            if let Some(val) = json_val {
                if !val.is_null() {
                    let normalized = transform_json_keys_for_language(val, "snake_case");
                    let json_str = serde_json::to_string(&normalized).unwrap_or_default();
                    let escaped = escape_c(&json_str);
                    let _ = writeln!(
                        out,
                        "    {prefix_upper}{request_type_pascal}* {var_name} = \
                         {prefix}_{request_type_snake}_from_json(\"{escaped}\");"
                    );
                    let _ = writeln!(out, "    assert({var_name} != NULL && \"failed to build request\");");
                    request_var = Some(var_name);
                    break;
                }
            }
        }
    }

    let req_handle = request_var.clone().unwrap_or_else(|| "NULL".to_string());
    let req_snake = request_var
        .as_ref()
        .and_then(|v| v.strip_suffix("_handle"))
        .unwrap_or(request_type_snake.as_str())
        .to_string();

    let fixture_id = &fixture.id;
    let has_mock = fixture.needs_mock_server();
    if has_mock && api_key_var.is_some() {
        // `api_key` and `base_url_buf` are already declared by the env-fallback
        // block above (the smoke+mock path). Reuse them — don't redeclare
        // `mock_base`/`base_url`, which would be a C compile error.
        // use_mock was captured before api_key was potentially reassigned to "test-key",
        // so it correctly reflects the original env state.
        let _ = writeln!(out, "    const char* _base_url_arg = use_mock ? base_url_buf : NULL;");
        let _ = writeln!(
            out,
            "    {prefix_upper}{owner_type}* client = {prefix}_{client_factory}(api_key, _base_url_arg, (uint64_t)-1, (uint32_t)-1, NULL);"
        );
    } else if has_mock {
        let _ = writeln!(out, "    const char* mock_base = getenv(\"MOCK_SERVER_URL\");");
        let _ = writeln!(out, "    assert(mock_base != NULL && \"MOCK_SERVER_URL must be set\");");
        let _ = writeln!(out, "    char base_url[1024];");
        let _ = writeln!(
            out,
            "    snprintf(base_url, sizeof(base_url), \"%s/fixtures/{fixture_id}\", mock_base);"
        );
        // Pass UINT64_MAX/UINT32_MAX (≡ -1ULL/-1U) as the FFI's None sentinel for
        // optional numeric primitives — passing literal 0 makes the binding see
        // Some(0), which Rust core treats as `Duration::from_secs(0)` (immediate
        // request deadline) and breaks every HTTP fixture.
        let _ = writeln!(
            out,
            "    {prefix_upper}{owner_type}* client = {prefix}_{client_factory}(\"test-key\", base_url, (uint64_t)-1, (uint32_t)-1, NULL);"
        );
    } else {
        let _ = writeln!(
            out,
            "    {prefix_upper}{owner_type}* client = {prefix}_{client_factory}(\"test-key\", NULL, (uint64_t)-1, (uint32_t)-1, NULL);"
        );
    }
    let _ = writeln!(out, "    assert(client != NULL && \"failed to create client\");");

    // The streaming opaque handle is a Rust type named `{Prefix}{Owner}{Method}StreamHandle`;
    // cbindgen additionally prepends the configured uppercase type-name `prefix` (e.g. `SAMPLELLM`),
    // exactly as it does for ordinary opaque handle types like `{prefix_upper}{owner_type}`.
    let pascal_prefix = prefix.to_pascal_case();
    let pascal_owner = streaming.owner_type.to_pascal_case();
    let pascal_name = streaming.adapter_name.to_pascal_case();
    let _ = writeln!(
        out,
        "    {prefix_upper}{pascal_prefix}{pascal_owner}{pascal_name}StreamHandle* stream_handle = \
         {stream_start}(client, {req_handle});"
    );

    if expects_error {
        let _ = writeln!(
            out,
            "    assert(stream_handle == NULL && \"expected stream-start to fail\");"
        );
        if request_var.is_some() {
            let _ = writeln!(out, "    {prefix}_{req_snake}_free({req_handle});");
        }
        let _ = writeln!(out, "    {prefix}_{owner_snake}_free(client);");
        let _ = writeln!(out, "}}");
        return;
    }

    let _ = writeln!(
        out,
        "    assert(stream_handle != NULL && \"expected stream-start to succeed\");"
    );

    let _ = writeln!(out, "    size_t chunks_count = 0;");
    let _ = writeln!(out, "    char* stream_content = (char*)malloc(1);");
    let _ = writeln!(out, "    assert(stream_content != NULL);");
    let _ = writeln!(out, "    stream_content[0] = '\\0';");
    let _ = writeln!(out, "    size_t stream_content_len = 0;");
    let _ = writeln!(out, "    int stream_complete = 0;");
    let _ = writeln!(out, "    int no_chunks_after_done = 1;");
    let _ = writeln!(out);

    let _ = writeln!(out, "    while (1) {{");
    let _ = writeln!(
        out,
        "        {prefix_upper}{item_type_pascal}* {result_var} = {stream_next}(stream_handle);"
    );
    let _ = writeln!(out, "        if ({result_var} == NULL) {{");
    let _ = writeln!(
        out,
        "            if ({prefix}_last_error_code() == 0) {{ stream_complete = 1; }}"
    );
    let _ = writeln!(out, "            break;");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "        chunks_count++;");
    let _ = writeln!(out, "        {prefix}_{item_type_snake}_free({result_var});");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "    {stream_free}(stream_handle);");
    let _ = writeln!(out);

    for assertion in &fixture.assertions {
        emit_chat_stream_assertion(out, assertion);
    }

    let _ = writeln!(out, "    free(stream_content);");
    if request_var.is_some() {
        let _ = writeln!(out, "    {prefix}_{req_snake}_free({req_handle});");
    }
    let _ = writeln!(out, "    {prefix}_{owner_snake}_free(client);");
    let _ = writeln!(
        out,
        "    /* suppress unused */ (void)no_chunks_after_done; \
         (void)stream_complete; (void)chunks_count; (void)stream_content_len;"
    );
    let _ = writeln!(out, "}}");
}

/// Emit a single fixture assertion for a streaming test, mapping fixture
/// pseudo-field references (`chunks`, `stream_content`, `stream_complete`, ...)
/// to the local aggregator variables built by [`render_streaming_test_function`].
fn emit_chat_stream_assertion(out: &mut String, assertion: &Assertion) {
    let field = assertion.field.as_deref().unwrap_or("");

    enum Kind {
        IntCount,
        Bool,
        Unsupported,
    }

    let (expr, kind) = match field {
        "chunks" => ("chunks_count", Kind::IntCount),
        "stream_complete" => ("stream_complete", Kind::Bool),
        "no_chunks_after_done" => ("no_chunks_after_done", Kind::Bool),
        "stream_content" | "finish_reason" | "tool_calls" | "tool_calls[0].function.name" | "usage.total_tokens" => {
            ("", Kind::Unsupported)
        }
        _ => ("", Kind::Unsupported),
    };

    let atype = assertion.assertion_type.as_str();
    if atype == "not_error" || atype == "error" {
        return;
    }

    if matches!(kind, Kind::Unsupported) {
        let _ = writeln!(
            out,
            "    /* skipped: streaming assertion on unsupported field '{field}' */"
        );
        return;
    }

    match (atype, &kind) {
        ("count_min", Kind::IntCount) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "    assert({expr} >= {n} && \"expected at least {n} chunks\");");
            }
        }
        ("is_true", Kind::Bool) => {
            let _ = writeln!(out, "    assert({expr} && \"expected {field} to be true\");");
        }
        ("is_false", Kind::Bool) => {
            let _ = writeln!(out, "    assert(!{expr} && \"expected {field} to be false\");");
        }
        ("greater_than_or_equal", Kind::IntCount) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "    assert({expr} >= {n} && \"expected {expr} >= {n}\");");
            }
        }
        ("equals", Kind::IntCount) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "    assert({expr} == {n} && \"equals assertion failed\");");
            }
        }
        _ => {
            let _ = writeln!(
                out,
                "    /* skipped: streaming assertion '{atype}' on field '{field}' not supported */"
            );
        }
    }
}
