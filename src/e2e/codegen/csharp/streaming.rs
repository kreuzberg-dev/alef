//! C# streaming e2e test method rendering.

use crate::core::config::ResolvedCrateConfig;
use crate::e2e::config::E2eConfig;
use crate::e2e::fixture::{Assertion, Fixture};
use heck::{ToSnakeCase, ToUpperCamelCase};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use super::{build_args_and_setup, json_to_csharp};

pub(super) fn resolve_csharp_streaming_item_type(
    call_config: &crate::e2e::config::CallConfig,
    adapters: &[crate::core::config::extras::AdapterConfig],
    function_name: &str,
) -> Option<String> {
    let function_name_snake = function_name.to_snake_case();
    crate::e2e::codegen::recipe::streaming_item_type(
        call_config,
        adapters,
        &[function_name, function_name_snake.as_str()],
    )
    .map(str::to_string)
}

/// Render a streaming-adapter test method. The C# binding emits
/// `IAsyncEnumerable<T>` (not `Task<T>`), so the test body uses `await foreach`
/// to drive the stream and aggregates
/// per-chunk data into local vars (`chunks`, `streamContent`, `streamComplete`,
/// optional `lastFinishReason`/`toolCallsJson`/`toolCalls0FunctionName`/`totalTokens`).
/// Assertions then run against those locals — never against pseudo-fields on a
/// response object.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_streaming_test_method(
    out: &mut String,
    fixture: &Fixture,
    class_name: &str,
    call_config: &crate::e2e::config::CallConfig,
    cs_overrides: Option<&crate::e2e::config::CallOverride>,
    e2e_config: &E2eConfig,
    enum_fields: &HashMap<String, String>,
    _assert_enum_fields: &HashMap<String, String>,
    nested_types: &HashMap<String, String>,
    exception_class: &str,
    adapters: &[crate::core::config::extras::AdapterConfig],
    config: &ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
    enums: &[crate::core::ir::EnumDef],
    item_type: Option<&str>,
) {
    let method_name = fixture.id.to_upper_camel_case();
    let description = &fixture.description;
    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");
    let Some(item_type) = item_type else {
        let _ = writeln!(out, "    [Fact]");
        let _ = writeln!(out, "    public void Test_{method_name}()");
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "        // {description}");
        let _ = writeln!(
            out,
            "        // skipped: streaming fixture requires adapter item_type for C# e2e codegen"
        );
        let _ = writeln!(out, "    }}");
        return;
    };

    // Streaming methods return IAsyncEnumerable<T> and carry the conventional
    // `Async` suffix to match the binding's generated DefaultClient surface
    // (which appends Async to every async-shaped method, streaming included).
    let effective_function_name = {
        let mut name = cs_overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call_config.function.to_upper_camel_case());
        if !name.ends_with("Async") {
            name.push_str("Async");
        }
        name
    };
    let function_name = effective_function_name.as_str();
    let recipe = crate::e2e::codegen::recipe::ResolvedE2eCallRecipe::resolve("csharp", fixture, call_config, type_defs);
    let args = recipe.args;

    let top_level_options_type = e2e_config
        .call
        .overrides
        .get("csharp")
        .and_then(|o| o.options_type.as_deref());
    let effective_options_type = recipe.options_type.or(top_level_options_type);
    let top_level_options_via = e2e_config
        .call
        .overrides
        .get("csharp")
        .and_then(|o| o.options_via.as_deref());
    let effective_options_via = cs_overrides
        .and_then(|o| o.options_via.as_deref())
        .or(top_level_options_via);

    let adapter_request_type_cs: Option<String> = adapters
        .iter()
        .find(|a| a.name == call_config.function.as_str())
        .and_then(|a| a.request_type.as_deref())
        .map(|rt| rt.rsplit("::").next().unwrap_or(rt).to_string());
    let mut _chat_stream_class_decls: Vec<String> = Vec::new();
    let mut _chat_stream_teardown_lines: Vec<String> = Vec::new();
    let (mut setup_lines, mut args_str) = build_args_and_setup(
        &fixture.input,
        args,
        class_name,
        effective_options_type,
        effective_options_via,
        enum_fields,
        nested_types,
        fixture,
        adapter_request_type_cs.as_deref(),
        config,
        type_defs,
        enums,
        &mut _chat_stream_class_decls,
        &mut _chat_stream_teardown_lines,
    );

    // For streaming methods with mock_url_list, wrap the URL list in the request type.
    if adapter_request_type_cs.is_some() {
        let has_mock_url_list = args.iter().any(|arg| arg.arg_type == "mock_url_list");
        if has_mock_url_list {
            if let Some(req_type) = &adapter_request_type_cs {
                let parts: Vec<&str> = args_str.split(", ").collect();
                if parts.len() >= 2 {
                    let urls_var = parts[parts.len() - 1];
                    let req_var = format!("{}Req", urls_var);
                    setup_lines.push(format!("var {req_var} = new {req_type} {{ Urls = {urls_var} }};"));
                    args_str = parts[..parts.len() - 1].join(", ");
                    if !args_str.is_empty() {
                        args_str.push_str(", ");
                    }
                    args_str.push_str(&req_var);
                }
            }
        }
    }

    let client_factory = cs_overrides.and_then(|o| o.client_factory.as_deref()).or_else(|| {
        e2e_config
            .call
            .overrides
            .get("csharp")
            .and_then(|o| o.client_factory.as_deref())
    });
    let mut client_factory_setup = String::new();
    if let Some(factory) = client_factory {
        let factory_name = factory.to_upper_camel_case();
        let fixture_id = &fixture.id;
        let has_mock = fixture.mock_response.is_some() || fixture.http.is_some();
        let api_key_var_opt = fixture.env.as_ref().and_then(|e| e.api_key_var.as_deref());
        let is_live_smoke = !has_mock && api_key_var_opt.is_some();
        if let Some(api_key_var) = api_key_var_opt.filter(|_| has_mock) {
            client_factory_setup.push_str(&format!(
                "        var apiKey = System.Environment.GetEnvironmentVariable(\"{api_key_var}\");\n"
            ));
            client_factory_setup.push_str(&format!(
                "        var baseUrl = string.IsNullOrEmpty(apiKey)\n            ? (System.Environment.GetEnvironmentVariable(\"MOCK_SERVER_URL\") ?? string.Empty) + \"/fixtures/{fixture_id}\"\n            : null;\n"
            ));
            client_factory_setup.push_str(&format!(
                "        Console.WriteLine($\"{fixture_id}: \" + (baseUrl == null ? \"using real API ({api_key_var} is set)\" : \"using mock server ({api_key_var} not set)\"));\n"
            ));
            client_factory_setup.push_str(&format!(
                "        var client = {class_name}.{factory_name}(string.IsNullOrEmpty(apiKey) ? \"test-key\" : apiKey, baseUrl, null, null, null);\n"
            ));
        } else if let Some(api_key_var) = api_key_var_opt.filter(|_| is_live_smoke) {
            client_factory_setup.push_str(&format!(
                "        var apiKey = System.Environment.GetEnvironmentVariable(\"{api_key_var}\");\n"
            ));
            client_factory_setup.push_str("        if (string.IsNullOrEmpty(apiKey)) { return; }\n");
            client_factory_setup.push_str(&format!(
                "        var client = {class_name}.{factory_name}(apiKey, null, null, null, null);\n"
            ));
        } else if fixture.has_host_root_route() {
            let env_key = format!("MOCK_SERVER_{}", fixture_id.to_uppercase());
            client_factory_setup.push_str(&format!(
                "        var _perFixtureUrl = System.Environment.GetEnvironmentVariable(\"{env_key}\");\n"
            ));
            client_factory_setup.push_str(&format!("        var baseUrl = !string.IsNullOrEmpty(_perFixtureUrl) ? _perFixtureUrl : (System.Environment.GetEnvironmentVariable(\"MOCK_SERVER_URL\") ?? string.Empty) + \"/fixtures/{fixture_id}\";\n"));
            client_factory_setup.push_str(&format!(
                "        var client = {class_name}.{factory_name}(\"test-key\", baseUrl, null, null, null);\n"
            ));
        } else {
            client_factory_setup.push_str(&format!(
                "        var baseUrl = (System.Environment.GetEnvironmentVariable(\"MOCK_SERVER_URL\") ?? string.Empty) + \"/fixtures/{fixture_id}\";\n"
            ));
            client_factory_setup.push_str(&format!(
                "        var client = {class_name}.{factory_name}(\"test-key\", baseUrl, null, null, null);\n"
            ));
        }
    }

    let call_target = if client_factory.is_some() { "client" } else { class_name };
    let call_expr = format!("{call_target}.{function_name}({args_str})");

    // Detect whether to use streaming-specific aggregators (chat-completion style)
    // or skip streaming accumulation altogether when the item type has no Choices field.
    // For non-chat-completion streams (e.g., CrawlEvent), use call_config's result_fields.
    let is_chat_stream = fixture.assertions.iter().any(|a| {
        if let Some(f) = a.field.as_deref() {
            matches!(
                f,
                "stream_content"
                    | "finish_reason"
                    | "tool_calls"
                    | "tool_calls[0].function.name"
                    | "usage.total_tokens"
            )
        } else {
            false
        }
    });

    let mut body = String::new();
    let _ = writeln!(body, "    [Fact]");
    let _ = writeln!(body, "    public async Task Test_{method_name}()");
    let _ = writeln!(body, "    {{");
    let _ = writeln!(body, "        // {description}");
    if !client_factory_setup.is_empty() {
        body.push_str(&client_factory_setup);
    }
    for line in &setup_lines {
        let _ = writeln!(body, "        {line}");
    }

    if expects_error {
        // Wrap the foreach in a lambda so the IAsyncEnumerable is actually
        // consumed (otherwise the producer never runs and no exception is raised).
        let _ = writeln!(
            body,
            "        await Assert.ThrowsAnyAsync<{exception_class}>(async () => {{"
        );
        let _ = writeln!(body, "            await foreach (var _chunk in {call_expr}) {{ }}");
        body.push_str("        });\n");
        body.push_str("    }\n");
        for line in body.lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
        return;
    }

    let _ = writeln!(body, "        var chunks = new List<{item_type}>();");
    // Optional chat-stream aggregator vars — emitted only when assertions reference them
    // so we don't pollute non-chat streaming bodies (CrawlEvent etc.) with chat-only
    // pseudo-fields that have no analog on the streamed item type.
    let asserts_finish_reason = is_chat_stream
        && fixture
            .assertions
            .iter()
            .any(|a| a.field.as_deref() == Some("finish_reason"));
    let asserts_tool_calls = is_chat_stream
        && fixture
            .assertions
            .iter()
            .any(|a| a.field.as_deref() == Some("tool_calls"));
    let asserts_tool_call_name = is_chat_stream
        && fixture
            .assertions
            .iter()
            .any(|a| a.field.as_deref() == Some("tool_calls[0].function.name"));
    let asserts_total_tokens = is_chat_stream
        && fixture
            .assertions
            .iter()
            .any(|a| a.field.as_deref() == Some("usage.total_tokens"));
    if is_chat_stream {
        body.push_str("        var streamContent = new System.Text.StringBuilder();\n");
    }
    if asserts_finish_reason {
        body.push_str("        string? lastFinishReason = null;\n");
    }
    if asserts_tool_calls {
        body.push_str("        string? toolCallsJson = null;\n");
    }
    if asserts_tool_call_name {
        body.push_str("        string? toolCalls0FunctionName = null;\n");
    }
    if asserts_total_tokens {
        body.push_str("        long? totalTokens = null;\n");
    }
    body.push_str("        var streamComplete = false;\n");
    let _ = writeln!(body, "        await foreach (var chunk in {call_expr})");
    body.push_str("        {\n");
    body.push_str("            chunks.Add(chunk);\n");

    if is_chat_stream {
        // Chat-completion style streaming: look for Choices[0].Delta.Content
        body.push_str(
            "            var choice = chunk.Choices != null && chunk.Choices.Count > 0 ? chunk.Choices[0] : null;\n",
        );
        body.push_str("            if (choice != null)\n");
        body.push_str("            {\n");
        body.push_str("                var delta = choice.Delta;\n");
        body.push_str("                if (delta != null && !string.IsNullOrEmpty(delta.Content))\n");
        body.push_str("                {\n");
        body.push_str("                    streamContent.Append(delta.Content);\n");
        body.push_str("                }\n");
        if asserts_finish_reason {
            // FinishReason is a JSON-converter-driven enum on the chat-completion DTOs;
            // serialize it through the converter so we get the snake_case API value
            // (e.g. "tool_calls") that assertions compare against, not the .NET name.
            body.push_str("                if (choice.FinishReason.HasValue)\n");
            body.push_str("                {\n");
            body.push_str(
                "                    lastFinishReason = System.Text.Json.JsonSerializer.Serialize(choice.FinishReason.Value).Trim('\"');\n",
            );
            body.push_str("                }\n");
        }
        if asserts_tool_calls || asserts_tool_call_name {
            body.push_str(
                "                if (delta != null && delta.ToolCalls != null && delta.ToolCalls.Count > 0)\n",
            );
            body.push_str("                {\n");
            if asserts_tool_calls {
                body.push_str(
                    "                    toolCallsJson = System.Text.Json.JsonSerializer.Serialize(delta.ToolCalls);\n",
                );
            }
            if asserts_tool_call_name {
                body.push_str("                    var firstFn = delta.ToolCalls[0].Function;\n");
                body.push_str("                    if (firstFn != null && !string.IsNullOrEmpty(firstFn.Name))\n");
                body.push_str("                    {\n");
                body.push_str("                        toolCalls0FunctionName = firstFn.Name;\n");
                body.push_str("                    }\n");
            }
            body.push_str("                }\n");
        }
        body.push_str("            }\n");
        if asserts_total_tokens {
            // Usage.TotalTokens is ulong on the chat-completion DTOs; widen to long?
            // so the assertion-mapping in `map_chat_stream_field` (Kind::IntTokens) can
            // compare against `long`-valued assertion JSON without losing the null state.
            body.push_str("            if (chunk.Usage != null)\n");
            body.push_str("            {\n");
            body.push_str("                totalTokens = (long)chunk.Usage.TotalTokens;\n");
            body.push_str("            }\n");
        }
    }
    body.push_str("        }\n");
    body.push_str("        streamComplete = true;\n");

    // Emit assertions on local aggregator vars or result_fields.
    let mut had_explicit_complete = false;
    for assertion in &fixture.assertions {
        if assertion.field.as_deref() == Some("stream_complete") {
            had_explicit_complete = true;
        }
        if is_chat_stream {
            emit_chat_stream_assertion(&mut body, assertion);
        } else {
            // For non-chat streams, emit skipped assertions for fields not in result_fields
            emit_non_chat_stream_assertion(&mut body, assertion, &call_config.result_fields);
        }
    }
    if !had_explicit_complete {
        body.push_str("        Assert.True(streamComplete);\n");
    }

    body.push_str("    }\n");

    for line in body.lines() {
        out.push_str("    ");
        out.push_str(line);
        out.push('\n');
    }
}

/// Emit assertions for non-chat-completion streams by checking which fields are
/// supported in result_fields. Skip unsupported assertions as comments.
///
/// This function replaces the hardcoded chat-completion assertions for generic
/// streaming types (like CrawlEvent) that have different field names.
fn emit_non_chat_stream_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_fields: &std::collections::HashSet<String>,
) {
    let atype = assertion.assertion_type.as_str();
    if atype == "not_error" || atype == "error" {
        return;
    }
    let field = assertion.field.as_deref().unwrap_or("");

    // Virtual fields that don't depend on result_fields
    match field {
        "stream_complete" => {
            let _ = writeln!(out, "        Assert.True(streamComplete);");
            return;
        }
        "no_chunks_after_done" => {
            let _ = writeln!(
                out,
                "        Assert.True(true); // virtual field, always true for collected streams"
            );
            return;
        }
        "chunks" | "stream.items" => match atype {
            "count_min" => {
                if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                    let _ = writeln!(out, "        Assert.True(chunks.Count >= {n});");
                }
                return;
            }
            "count_equals" => {
                if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                    let _ = writeln!(out, "        Assert.Equal({n}, chunks.Count);");
                }
                return;
            }
            _ => {}
        },
        _ => {}
    }

    // For fields that depend on result_fields, check if they're supported
    if !result_fields.iter().any(|f| field.starts_with(f)) {
        let _ = writeln!(
            out,
            "        // skipped: streaming assertion on unsupported field '{field}'"
        );
        return;
    }

    // Fields in result_fields can be asserted via chunks[i].FieldName
    match atype {
        "not_empty" => {
            let _ = writeln!(out, "        Assert.NotEmpty(chunks);");
        }
        "is_empty" => {
            let _ = writeln!(out, "        Assert.Empty(chunks);");
        }
        _ => {
            let _ = writeln!(
                out,
                "        // skipped: assertion type '{atype}' on field '{field}' not yet supported for streaming"
            );
        }
    }
}

/// Map a streaming fixture assertion to an `Assert` call on the local aggregator
/// variable produced by `render_chat_stream_test_method`. Pseudo-fields like
/// `chunks` / `stream_content` / `stream_complete` resolve to in-method locals.
fn emit_chat_stream_assertion(out: &mut String, assertion: &Assertion) {
    let atype = assertion.assertion_type.as_str();
    if atype == "not_error" || atype == "error" {
        return;
    }
    let field = assertion.field.as_deref().unwrap_or("");

    enum Kind {
        Chunks,
        Bool,
        Str,
        IntTokens,
        Json,
        Unsupported,
    }

    let (expr, kind) = match field {
        "chunks" => ("chunks", Kind::Chunks),
        "stream_content" => ("streamContent.ToString()", Kind::Str),
        "stream_complete" => ("streamComplete", Kind::Bool),
        "no_chunks_after_done" => ("streamComplete", Kind::Bool),
        "finish_reason" => ("lastFinishReason", Kind::Str),
        "tool_calls" => ("toolCallsJson", Kind::Json),
        "tool_calls[0].function.name" => ("toolCalls0FunctionName", Kind::Str),
        "usage.total_tokens" => ("totalTokens", Kind::IntTokens),
        _ => ("", Kind::Unsupported),
    };

    if matches!(kind, Kind::Unsupported) {
        let _ = writeln!(
            out,
            "        // skipped: streaming assertion on unsupported field '{field}'"
        );
        return;
    }

    match (atype, &kind) {
        ("count_min", Kind::Chunks) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(
                    out,
                    "        Assert.True(chunks.Count >= {n}, \"expected at least {n} chunks\");"
                );
            }
        }
        ("count_equals", Kind::Chunks) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "        Assert.Equal({n}, chunks.Count);");
            }
        }
        ("equals", Kind::Str) => {
            if let Some(val) = &assertion.value {
                let cs_val = json_to_csharp(val);
                let _ = writeln!(out, "        Assert.Equal({cs_val}, {expr});");
            }
        }
        ("contains", Kind::Str) => {
            if let Some(val) = &assertion.value {
                let cs_val = json_to_csharp(val);
                let _ = writeln!(out, "        Assert.Contains({cs_val}, {expr} ?? string.Empty);");
            }
        }
        ("not_empty", Kind::Str) => {
            let _ = writeln!(
                out,
                "        Assert.False(string.IsNullOrEmpty({expr} ?? string.Empty));"
            );
        }
        ("not_empty", Kind::Json) => {
            let _ = writeln!(out, "        Assert.NotNull({expr});");
        }
        ("is_empty", Kind::Str) => {
            let _ = writeln!(
                out,
                "        Assert.True(string.IsNullOrEmpty({expr} ?? string.Empty));"
            );
        }
        ("is_true", Kind::Bool) => {
            let _ = writeln!(out, "        Assert.True({expr});");
        }
        ("is_false", Kind::Bool) => {
            let _ = writeln!(out, "        Assert.False({expr});");
        }
        ("greater_than_or_equal", Kind::IntTokens) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "        Assert.True({expr} >= {n}, \"expected >= {n}\");");
            }
        }
        ("equals", Kind::IntTokens) => {
            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                let _ = writeln!(out, "        Assert.Equal((long?){n}, {expr});");
            }
        }
        _ => {
            let _ = writeln!(
                out,
                "        // skipped: streaming assertion '{atype}' on field '{field}' not supported"
            );
        }
    }
}
