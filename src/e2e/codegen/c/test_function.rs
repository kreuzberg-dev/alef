//! C e2e per-fixture test function rendering.

use crate::core::config::ResolvedCrateConfig;
use crate::e2e::codegen::transform_json_keys_for_language;
use crate::e2e::escape::{escape_c, sanitize_ident};
use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::Fixture;
use heck::ToSnakeCase;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;

use super::{
    build_args_string_c, emit_nested_accessor, infer_opaque_handle_type, is_primitive_c_type, is_skipped_c_field,
    json_to_c, render_assertion, render_bytes_test_function, render_c_diagnostic_skip,
    render_engine_factory_test_function, render_streaming_test_function, resolve_c_client_owner_type,
    resolve_c_streaming_adapter, try_emit_enum_accessor,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_test_function(
    out: &mut String,
    fixture: &Fixture,
    prefix: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::e2e::config::ArgMapping],
    field_resolver: &FieldResolver,
    fields_c_types: &HashMap<String, String>,
    fields_enum: &HashSet<String>,
    result_type_name: &str,
    options_type_name: &str,
    client_factory: Option<&str>,
    raw_c_result_type: Option<&str>,
    c_free_fn: Option<&str>,
    c_engine_factory: Option<&str>,
    result_is_option: bool,
    result_is_bytes: bool,
    streaming: Option<bool>,
    extra_args: &[String],
    config: &ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
) {
    let fn_name = sanitize_ident(&fixture.id);
    let description = &fixture.description;

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let _ = writeln!(out, "void test_{fn_name}(void) {{");
    let _ = writeln!(out, "    /* {description} */");

    // Smoke/live fixtures gated on a required env var (e.g. OPENAI_API_KEY).
    // When the var is missing, treat as a successful skip — mirrors Python's
    // `pytest.skip("OPENAI_API_KEY not set")` and Java's `Assumptions.assumeTrue(...)`
    // so CI runs without provider credentials don't fail every smoke test.
    //
    // When the fixture also has a mock_response/http block, we support an env+mock
    // fallback: if the API key is set, use the real API; otherwise fall back to the
    // mock server. This lets the same fixture exercise both paths.
    let has_mock = fixture.needs_mock_server();
    let api_key_var = fixture.env.as_ref().and_then(|e| e.api_key_var.as_deref());
    if let Some(env) = &fixture.env {
        if let Some(var) = &env.api_key_var {
            let fixture_id = &fixture.id;
            if has_mock {
                let _ = writeln!(out, "    const char* api_key = getenv(\"{var}\");");
                let _ = writeln!(out, "    const char* mock_base = getenv(\"MOCK_SERVER_URL\");");
                let _ = writeln!(out, "    char base_url_buf[512];");
                let _ = writeln!(out, "    int use_mock = !(api_key && api_key[0] != '\\0');");
                let _ = writeln!(out, "    if (!use_mock) {{");
                let _ = writeln!(
                    out,
                    "        fprintf(stderr, \"{fixture_id}: using real API ({var} is set)\\n\");"
                );
                let _ = writeln!(out, "    }} else {{");
                let _ = writeln!(
                    out,
                    "        fprintf(stderr, \"{fixture_id}: using mock server ({var} not set)\\n\");"
                );
                let _ = writeln!(
                    out,
                    "        snprintf(base_url_buf, sizeof(base_url_buf), \"%s/fixtures/{fixture_id}\", mock_base ? mock_base : \"\");"
                );
                let _ = writeln!(out, "        api_key = \"test-key\";");
                let _ = writeln!(out, "    }}");
            } else {
                let _ = writeln!(out, "    if (getenv(\"{var}\") == NULL) {{ return; }}");
            }
        }
    }

    let prefix_upper = prefix.to_uppercase();

    // Engine-factory pattern: used when c_engine_factory is configured.
    // Creates a config handle from JSON, builds an engine, calls {prefix}_{function}(engine, url),
    // frees result and engine.
    if let Some(config_type) = c_engine_factory {
        render_engine_factory_test_function(
            out,
            fixture,
            prefix,
            function_name,
            result_var,
            field_resolver,
            fields_c_types,
            fields_enum,
            result_type_name,
            config_type,
            expects_error,
            raw_c_result_type,
        );
        return;
    }

    // Streaming adapters use an FFI iterator handle instead of a single
    // response. Emit start/next/free loop and aggregate per-chunk data
    // into local vars (chunks_count, stream_content, stream_complete) so fixture
    // assertions on pseudo-fields resolve to those locals rather than to
    // non-existent accessor functions on a single chunk handle.
    if client_factory.is_some() && crate::e2e::codegen::streaming_assertions::resolve_is_streaming(fixture, streaming) {
        let Some(streaming) = resolve_c_streaming_adapter(config, function_name) else {
            render_c_diagnostic_skip(
                out,
                "streaming fixture requires matching [[crates.adapters]] metadata for C e2e codegen",
            );
            return;
        };
        render_streaming_test_function(
            out,
            fixture,
            prefix,
            result_var,
            args,
            client_factory.unwrap_or(""),
            &streaming,
            expects_error,
            api_key_var,
        );
        return;
    }

    // Byte-buffer pattern: methods like `speech` and `file_content` return raw
    // bytes via the out-pointer FFI shape:
    //   `int32_t fn(this, req, uint8_t** out_ptr, uintptr_t* out_len, uintptr_t* out_cap)`
    // rather than as an opaque `*Response` handle. The C codegen must declare
    // the out-params, check the int32_t status code, and free with
    // `<prefix>_free_bytes` rather than emitting non-existent
    // `<prefix>_<response>_audio` / `_content` accessors.
    if let Some(factory) = client_factory {
        if result_is_bytes {
            let Some(client_owner_type) = resolve_c_client_owner_type(config, type_defs, function_name) else {
                render_c_diagnostic_skip(
                    out,
                    "client_factory is configured but C e2e could not resolve the client owner type",
                );
                return;
            };
            render_bytes_test_function(
                out,
                fixture,
                prefix,
                function_name,
                result_var,
                args,
                options_type_name,
                result_type_name,
                factory,
                &client_owner_type,
                expects_error,
            );
            return;
        }
    }

    // Client pattern: used when client_factory is configured.
    // Builds typed request handles from json_object args, creates a client via the
    // factory function, calls {prefix}_default_client_{function_name}(client, req),
    // then frees result, request handles, and client.
    if let Some(factory) = client_factory {
        let Some(client_owner_type) = resolve_c_client_owner_type(config, type_defs, function_name) else {
            render_c_diagnostic_skip(
                out,
                "client_factory is configured but C e2e could not resolve the client owner type",
            );
            return;
        };
        let mut request_handle_vars: Vec<(String, String)> = Vec::new(); // (arg_name, var_name)
        // Inline argument expressions appended after request handles in the
        // method call (e.g. literal C strings for `string` args, `NULL` for
        // optional pointer args). Order matches the position in `args`.
        let mut inline_method_args: Vec<String> = Vec::new();

        for arg in args {
            if arg.arg_type == "json_object" {
                // Prefer options_type from the C override when set, since the result
                // type isn't always a clean strip-Response/append-Request transform
                // (e.g. transcribe -> Create**Transcription**Request, not TranscriptionRequest).
                // Fall back to deriving from result_type for backward-compat cases.
                let request_type_pascal = if !options_type_name.is_empty() {
                    options_type_name.to_string()
                } else if let Some(stripped) = result_type_name.strip_suffix("Response") {
                    format!("{}Request", stripped)
                } else {
                    format!("{result_type_name}Request")
                };
                let request_type_snake = request_type_pascal.to_snake_case();
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
                        if expects_error {
                            // For error fixtures (e.g. invalid enum value rejected by
                            // serde), `_from_json` may legitimately return NULL — that
                            // counts as the expected failure. Mirror Java's pattern of
                            // wrapping setup + call inside `assertThrows(...)` so error
                            // fixtures pass at *any* failure step. The test returns
                            // before attempting to create a client, leaving no
                            // resources to free.
                            let _ = writeln!(out, "    if ({var_name} == NULL) {{ return; }}");
                        } else {
                            let _ = writeln!(out, "    assert({var_name} != NULL && \"failed to build request\");");
                        }
                        request_handle_vars.push((arg.name.clone(), var_name));
                    }
                }
            } else if arg.arg_type == "string" {
                // String arg: read fixture input, emit as a C string literal inline.
                let field = arg.field.strip_prefix("input.").unwrap_or(&arg.field);
                let val = fixture.input.get(field);
                match val {
                    Some(v) if v.is_string() => {
                        let s = v.as_str().unwrap_or_default();
                        let escaped = escape_c(s);
                        inline_method_args.push(format!("\"{escaped}\""));
                    }
                    Some(serde_json::Value::Null) | None if arg.optional => {
                        inline_method_args.push("NULL".to_string());
                    }
                    None => {
                        inline_method_args.push("\"\"".to_string());
                    }
                    Some(other) => {
                        let s = serde_json::to_string(other).unwrap_or_default();
                        let escaped = escape_c(&s);
                        inline_method_args.push(format!("\"{escaped}\""));
                    }
                }
            } else if arg.optional {
                // Optional non-string, non-json_object arg: pass NULL.
                inline_method_args.push("NULL".to_string());
            }
        }

        let fixture_id = &fixture.id;
        // Pass UINT64_MAX/UINT32_MAX (≡ -1ULL/-1U) as the FFI's None sentinel for
        // optional numeric primitives — passing literal 0 makes the binding see
        // Some(0), which Rust core treats as `Duration::from_secs(0)` (immediate
        // request deadline) and breaks every HTTP fixture.
        if has_mock && api_key_var.is_some() {
            // api_key and base_url_buf are already declared in the env-fallback block above.
            // use_mock was captured before api_key was potentially reassigned to "test-key",
            // so it correctly reflects the original env state.
            let _ = writeln!(out, "    const char* _base_url_arg = use_mock ? base_url_buf : NULL;");
            let _ = writeln!(
                out,
                "    {prefix_upper}{client_owner_type}* client = {prefix}_{factory}(api_key, _base_url_arg, (uint64_t)-1, (uint32_t)-1, NULL);"
            );
        } else if has_mock {
            let _ = writeln!(out, "    const char* mock_base = getenv(\"MOCK_SERVER_URL\");");
            let _ = writeln!(out, "    assert(mock_base != NULL && \"MOCK_SERVER_URL must be set\");");
            let _ = writeln!(out, "    char base_url[1024];");
            let _ = writeln!(
                out,
                "    snprintf(base_url, sizeof(base_url), \"%s/fixtures/{fixture_id}\", mock_base);"
            );
            let _ = writeln!(
                out,
                "    {prefix_upper}{client_owner_type}* client = {prefix}_{factory}(\"test-key\", base_url, (uint64_t)-1, (uint32_t)-1, NULL);"
            );
        } else {
            let _ = writeln!(
                out,
                "    {prefix_upper}{client_owner_type}* client = {prefix}_{factory}(\"test-key\", NULL, (uint64_t)-1, (uint32_t)-1, NULL);"
            );
        }
        let _ = writeln!(out, "    assert(client != NULL && \"failed to create client\");");

        let method_args = if request_handle_vars.is_empty() && inline_method_args.is_empty() && extra_args.is_empty() {
            String::new()
        } else {
            let handles: Vec<String> = request_handle_vars.iter().map(|(_, v)| v.clone()).collect();
            let parts: Vec<String> = handles
                .into_iter()
                .chain(inline_method_args.iter().cloned())
                .chain(extra_args.iter().cloned())
                .collect();
            format!(", {}", parts.join(", "))
        };

        let call_fn = format!("{prefix}_default_client_{function_name}");

        if expects_error {
            let _ = writeln!(
                out,
                "    {prefix_upper}{result_type_name}* {result_var} = {call_fn}(client{method_args});"
            );
            for (_, var_name) in &request_handle_vars {
                let req_snake = var_name.strip_suffix("_handle").unwrap_or(var_name);
                let _ = writeln!(out, "    {prefix}_{req_snake}_free({var_name});");
            }
            let _ = writeln!(out, "    {prefix}_default_client_free(client);");
            let _ = writeln!(out, "    assert({result_var} == NULL && \"expected call to fail\");");
            let _ = writeln!(out, "}}");
            return;
        }

        let _ = writeln!(
            out,
            "    {prefix_upper}{result_type_name}* {result_var} = {call_fn}(client{method_args});"
        );
        let _ = writeln!(out, "    assert({result_var} != NULL && \"expected call to succeed\");");

        let mut intermediate_handles: Vec<(String, String)> = Vec::new();
        let mut accessed_fields: Vec<(String, String, bool)> = Vec::new();
        // Locals declared as primitive C scalars (uint64_t, double, bool, ...).
        // Locals not present here default to char* (heap-allocated accessor result).
        let mut primitive_locals: HashMap<String, String> = HashMap::new();
        // Locals declared as opaque struct handles (e.g. SAMPLELLMUsage*).
        // Keyed by local_var, value is the snake_case type name used for free().
        let mut opaque_handle_locals: HashMap<String, String> = HashMap::new();

        for assertion in &fixture.assertions {
            if let Some(f) = &assertion.field {
                if !f.is_empty() && !accessed_fields.iter().any(|(k, _, _)| k == f) {
                    let resolved_raw = field_resolver.resolve(f);
                    // Strip virtual namespace prefixes (e.g. "interaction.action_results[0].x"
                    // → "action_results[0].x") matching the same logic as FieldResolver::accessor.
                    let resolved = if let Some(stripped) = field_resolver.namespace_stripped_path(resolved_raw) {
                        let stripped_first = stripped.split('.').next().unwrap_or(stripped);
                        let stripped_first = stripped_first.split('[').next().unwrap_or(stripped_first);
                        if field_resolver.is_valid_for_result(stripped_first) {
                            stripped
                        } else {
                            resolved_raw
                        }
                    } else {
                        resolved_raw
                    };
                    let local_var = f.replace(['.', '['], "_").replace(']', "");
                    let has_map_access = resolved.contains('[');
                    if resolved.contains('.') {
                        let leaf_primitive = emit_nested_accessor(
                            out,
                            prefix,
                            resolved,
                            &local_var,
                            result_var,
                            fields_c_types,
                            fields_enum,
                            &mut intermediate_handles,
                            result_type_name,
                            f,
                        );
                        if let Some(prim) = leaf_primitive {
                            primitive_locals.insert(local_var.clone(), prim);
                        }
                    } else {
                        let result_type_snake = result_type_name.to_snake_case();
                        let accessor_fn = format!("{prefix}_{result_type_snake}_{resolved}");
                        let lookup_key = format!("{result_type_snake}.{resolved}");
                        if is_skipped_c_field(fields_c_types, &result_type_snake, resolved) {
                            // Field marked "skip" — record sentinel so render_assertion skips it.
                            primitive_locals.insert(local_var.clone(), "__skip__".to_string());
                        } else if let Some(t) = fields_c_types.get(&lookup_key).filter(|t| is_primitive_c_type(t)) {
                            let _ = writeln!(out, "    {t} {local_var} = {accessor_fn}({result_var});");
                            primitive_locals.insert(local_var.clone(), t.clone());
                        } else if try_emit_enum_accessor(
                            out,
                            prefix,
                            &prefix_upper,
                            f,
                            resolved,
                            &result_type_snake,
                            &accessor_fn,
                            result_var,
                            &local_var,
                            fields_c_types,
                            fields_enum,
                            &mut intermediate_handles,
                        ) {
                            // accessor emitted with enum-to-string conversion
                        } else if let Some(handle_pascal) =
                            infer_opaque_handle_type(fields_c_types, &result_type_snake, resolved)
                        {
                            // Opaque struct handle: cannot be read as char*.
                            let _ = writeln!(
                                out,
                                "    {prefix_upper}{handle_pascal}* {local_var} = {accessor_fn}({result_var});"
                            );
                            opaque_handle_locals.insert(local_var.clone(), handle_pascal.to_snake_case());
                        } else {
                            let _ = writeln!(out, "    char* {local_var} = {accessor_fn}({result_var});");
                        }
                    }
                    accessed_fields.push((f.clone(), local_var, has_map_access));
                }
            }
        }

        for assertion in &fixture.assertions {
            render_assertion(
                out,
                assertion,
                result_var,
                prefix,
                field_resolver,
                &accessed_fields,
                &primitive_locals,
                &opaque_handle_locals,
            );
        }

        for (_f, local_var, from_json) in &accessed_fields {
            if primitive_locals.contains_key(local_var) {
                continue;
            }
            if let Some(snake_type) = opaque_handle_locals.get(local_var) {
                let _ = writeln!(out, "    {prefix}_{snake_type}_free({local_var});");
                continue;
            }
            if *from_json {
                let _ = writeln!(out, "    free({local_var});");
            } else {
                let _ = writeln!(out, "    {prefix}_free_string({local_var});");
            }
        }
        for (handle_var, snake_type) in intermediate_handles.iter().rev() {
            if snake_type == "free_string" {
                let _ = writeln!(out, "    {prefix}_free_string({handle_var});");
            } else if snake_type == "free" {
                // Intermediate JSON-key extraction (alef_json_get_string) — heap
                // char* allocated by malloc-class helper; freed via plain free().
                let _ = writeln!(out, "    free({handle_var});");
            } else {
                let _ = writeln!(out, "    {prefix}_{snake_type}_free({handle_var});");
            }
        }
        let result_type_snake = result_type_name.to_snake_case();
        let _ = writeln!(out, "    {prefix}_{result_type_snake}_free({result_var});");
        for (_, var_name) in &request_handle_vars {
            let req_snake = var_name.strip_suffix("_handle").unwrap_or(var_name);
            let _ = writeln!(out, "    {prefix}_{req_snake}_free({var_name});");
        }
        let _ = writeln!(out, "    {prefix}_default_client_free(client);");
        let _ = writeln!(out, "}}");
        return;
    }

    // Raw C result type path: functions returning a primitive C type (char*, int32_t,
    // uintptr_t) rather than an opaque handle pointer.
    if let Some(raw_type) = raw_c_result_type {
        // Build argument string. Void-arg functions pass nothing.
        let args_str = if args.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = args
                .iter()
                .filter_map(|arg| {
                    let field = arg.field.strip_prefix("input.").unwrap_or(&arg.field);
                    let val = fixture.input.get(field);
                    match val {
                        None if arg.optional => Some("NULL".to_string()),
                        None => None,
                        Some(v) if v.is_null() && arg.optional => Some("NULL".to_string()),
                        Some(v) => Some(json_to_c(v)),
                    }
                })
                .collect();
            parts.join(", ")
        };

        // Declare result variable.
        let _ = writeln!(out, "    {raw_type} {result_var} = {function_name}({args_str});");

        // not_error assertion.
        let has_not_error = fixture.assertions.iter().any(|a| a.assertion_type == "not_error");
        if has_not_error {
            match raw_type {
                "char*" if !result_is_option => {
                    let _ = writeln!(out, "    assert({result_var} != NULL && \"expected call to succeed\");");
                }
                "int32_t" => {
                    let _ = writeln!(out, "    assert({result_var} >= 0 && \"expected call to succeed\");");
                }
                "uintptr_t" => {
                    let _ = writeln!(
                        out,
                        "    assert({prefix}_last_error_code() == 0 && \"expected call to succeed\");"
                    );
                }
                _ => {}
            }
        }

        // Other assertions.
        for assertion in &fixture.assertions {
            match assertion.assertion_type.as_str() {
                "not_error" | "error" => {} // handled above / not applicable
                "not_empty" => {
                    let _ = writeln!(
                        out,
                        "    assert({result_var} != NULL && strlen({result_var}) > 0 && \"expected non-empty value\");"
                    );
                }
                "is_empty" => {
                    if result_is_option && raw_type == "char*" {
                        let _ = writeln!(
                            out,
                            "    assert({result_var} == NULL && \"expected empty/null value\");"
                        );
                    } else {
                        let _ = writeln!(
                            out,
                            "    assert(strlen({result_var}) == 0 && \"expected empty value\");"
                        );
                    }
                }
                "count_min" => {
                    if let Some(val) = &assertion.value {
                        if let Some(n) = val.as_u64() {
                            match raw_type {
                                "char*" => {
                                    let _ = writeln!(out, "    {{");
                                    let _ = writeln!(
                                        out,
                                        "        assert({result_var} != NULL && \"expected non-null JSON array\");"
                                    );
                                    let _ =
                                        writeln!(out, "        int elem_count = alef_json_array_count({result_var});");
                                    let _ = writeln!(
                                        out,
                                        "        assert(elem_count >= {n} && \"expected at least {n} elements\");"
                                    );
                                    let _ = writeln!(out, "    }}");
                                }
                                _ => {
                                    let _ = writeln!(
                                        out,
                                        "    assert((size_t){result_var} >= {n} && \"expected at least {n} elements\");"
                                    );
                                }
                            }
                        }
                    }
                }
                "greater_than_or_equal" => {
                    if let Some(val) = &assertion.value {
                        let c_val = json_to_c(val);
                        let _ = writeln!(
                            out,
                            "    assert({result_var} >= {c_val} && \"expected greater than or equal\");"
                        );
                    }
                }
                "contains" => {
                    if let Some(val) = &assertion.value {
                        let c_val = json_to_c(val);
                        let _ = writeln!(
                            out,
                            "    assert(strstr({result_var}, {c_val}) != NULL && \"expected to contain substring\");"
                        );
                    }
                }
                "contains_all" => {
                    if let Some(values) = &assertion.values {
                        for val in values {
                            let c_val = json_to_c(val);
                            let _ = writeln!(
                                out,
                                "    assert(strstr({result_var}, {c_val}) != NULL && \"expected to contain substring\");"
                            );
                        }
                    }
                }
                "equals" => {
                    if let Some(val) = &assertion.value {
                        let c_val = json_to_c(val);
                        if val.is_string() {
                            let _ = writeln!(
                                out,
                                "    assert({result_var} != NULL && str_trim_eq({result_var}, {c_val}) == 0 && \"equals assertion failed\");"
                            );
                        } else {
                            let _ = writeln!(
                                out,
                                "    assert({result_var} == {c_val} && \"equals assertion failed\");"
                            );
                        }
                    }
                }
                "not_contains" => {
                    if let Some(val) = &assertion.value {
                        let c_val = json_to_c(val);
                        let _ = writeln!(
                            out,
                            "    assert(strstr({result_var}, {c_val}) == NULL && \"expected NOT to contain substring\");"
                        );
                    }
                }
                "starts_with" => {
                    if let Some(val) = &assertion.value {
                        let c_val = json_to_c(val);
                        let _ = writeln!(
                            out,
                            "    assert(strncmp({result_var}, {c_val}, strlen({c_val})) == 0 && \"expected to start with\");"
                        );
                    }
                }
                "is_true" => {
                    let _ = writeln!(out, "    assert({result_var});");
                }
                "is_false" => {
                    let _ = writeln!(out, "    assert(!{result_var});");
                }
                other => {
                    panic!("C e2e raw-result generator: unsupported assertion type: {other}");
                }
            }
        }

        // Free char* results.
        if raw_type == "char*" {
            let free_fn = c_free_fn
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{prefix}_free_string"));
            if result_is_option {
                let _ = writeln!(out, "    if ({result_var} != NULL) {{ {free_fn}({result_var}); }}");
            } else {
                let _ = writeln!(out, "    {free_fn}({result_var});");
            }
        }

        let _ = writeln!(out, "}}");
        return;
    }

    // Legacy (non-client) path: call the function directly.
    // Used for libraries that expose standalone FFI functions.

    // Use the function name directly — the override already includes the prefix
    // (e.g. "htm_convert"), so we must NOT prepend it again.
    let prefixed_fn = function_name.to_string();

    // For json_object args, emit a from_json call to construct the options handle.
    let mut has_options_handle = false;
    for arg in args {
        if arg.arg_type == "json_object" {
            let field = arg.field.strip_prefix("input.").unwrap_or(&arg.field);
            if let Some(val) = fixture.input.get(field) {
                if !val.is_null() {
                    // Fixture keys are camelCase; generated FFI from_json helpers
                    // deserialize into Rust types using serde's configured casing.
                    // Normalize keys before serializing.
                    let normalized = transform_json_keys_for_language(val, "snake_case");
                    let json_str = serde_json::to_string(&normalized).unwrap_or_default();
                    let escaped = escape_c(&json_str);
                    let upper = prefix.to_uppercase();
                    let options_type_pascal = options_type_name;
                    let options_type_snake = options_type_name.to_snake_case();
                    let _ = writeln!(
                        out,
                        "    {upper}{options_type_pascal}* options_handle = {prefix}_{options_type_snake}_from_json(\"{escaped}\");"
                    );
                    has_options_handle = true;
                }
            }
        }
    }

    let args_str = build_args_string_c(&fixture.input, args, has_options_handle, config, type_defs, fixture);

    // Host-capsule passthrough: a free function whose result type is a configured
    // capsule (e.g. `get_language` → `const TSLanguage *`) returns a borrowed,
    // host-owned pointer — NOT an alef opaque handle. The exported symbol's C
    // return type is `const {c_return_type} *`, and the pointer must never be passed
    // to `{prefix}_{type}_free` (that frees an alef Box; the capsule points at a
    // static grammar / registry-owned object, so freeing it corrupts the heap).
    // Emit a minimal declare + null-check with no free, mirroring the borrowed
    // semantics the Go/Zig bindings get for free via GC / borrow checking.
    if let Some(capsule) = config.ffi.as_ref().and_then(|f| f.capsule_types.get(result_type_name)) {
        let c_return_type = &capsule.c_return_type;
        let _ = writeln!(
            out,
            "    const {c_return_type} *{result_var} = {prefixed_fn}({args_str});"
        );
        if has_options_handle {
            let options_type_snake = options_type_name.to_snake_case();
            let _ = writeln!(out, "    {prefix}_{options_type_snake}_free(options_handle);");
        }
        if expects_error {
            let _ = writeln!(out, "    assert({result_var} == NULL && \"expected call to fail\");");
        } else {
            let _ = writeln!(out, "    assert({result_var} != NULL && \"expected call to succeed\");");
        }
        let _ = writeln!(out, "}}");
        return;
    }

    if expects_error {
        let _ = writeln!(
            out,
            "    {prefix_upper}{result_type_name}* {result_var} = {prefixed_fn}({args_str});"
        );
        if has_options_handle {
            let options_type_snake = options_type_name.to_snake_case();
            let _ = writeln!(out, "    {prefix}_{options_type_snake}_free(options_handle);");
        }
        let _ = writeln!(out, "    assert({result_var} == NULL && \"expected call to fail\");");
        let _ = writeln!(out, "}}");
        return;
    }

    // The FFI returns an opaque handle; extract the content string from it.
    let _ = writeln!(
        out,
        "    {prefix_upper}{result_type_name}* {result_var} = {prefixed_fn}({args_str});"
    );
    let _ = writeln!(out, "    assert({result_var} != NULL && \"expected call to succeed\");");

    // Collect fields accessed by assertions so we can emit accessor calls.
    // C FFI uses the opaque handle pattern: {prefix}_conversion_result_{field}(handle).
    // For nested paths we generate chained FFI accessor calls using the type
    // chain from `fields_c_types`.
    // Each entry: (fixture_field, local_var, from_json_extract).
    // `from_json_extract` is true when the variable was extracted from a JSON
    // map via alef_json_get_string and needs free() instead of {prefix}_free_string().
    let mut accessed_fields: Vec<(String, String, bool)> = Vec::new();
    // Track intermediate handles emitted so we can free them and avoid duplicates.
    // Each entry: (handle_var_name, snake_type_name) — freed in reverse order.
    let mut intermediate_handles: Vec<(String, String)> = Vec::new();
    // Locals declared as primitive C scalars (uint64_t, double, bool, ...).
    let mut primitive_locals: HashMap<String, String> = HashMap::new();
    // Locals declared as opaque struct handles (e.g. SAMPLELLMUsage*).
    let mut opaque_handle_locals: HashMap<String, String> = HashMap::new();

    for assertion in &fixture.assertions {
        if let Some(f) = &assertion.field {
            if !f.is_empty() && !accessed_fields.iter().any(|(k, _, _)| k == f) {
                let resolved_raw = field_resolver.resolve(f);
                // Strip virtual namespace prefixes (e.g. "interaction.action_results[0].x"
                // → "action_results[0].x") matching the same logic as FieldResolver::accessor.
                let resolved = if let Some(stripped) = field_resolver.namespace_stripped_path(resolved_raw) {
                    let stripped_first = stripped.split('.').next().unwrap_or(stripped);
                    let stripped_first = stripped_first.split('[').next().unwrap_or(stripped_first);
                    if field_resolver.is_valid_for_result(stripped_first) {
                        stripped
                    } else {
                        resolved_raw
                    }
                } else {
                    resolved_raw
                };
                let local_var = f.replace(['.', '['], "_").replace(']', "");
                let has_map_access = resolved.contains('[');

                if resolved.contains('.') {
                    let leaf_result = emit_nested_accessor(
                        out,
                        prefix,
                        resolved,
                        &local_var,
                        result_var,
                        fields_c_types,
                        fields_enum,
                        &mut intermediate_handles,
                        result_type_name,
                        f,
                    );
                    if let Some(returned_type) = leaf_result {
                        // Could be a primitive type (primitive_locals) or opaque handle type
                        if is_primitive_c_type(&returned_type) {
                            primitive_locals.insert(local_var.clone(), returned_type);
                        } else {
                            // Opaque handle returned — register for cleanup
                            opaque_handle_locals.insert(local_var.clone(), returned_type);
                        }
                    }
                } else {
                    let result_type_snake = result_type_name.to_snake_case();
                    let accessor_fn = format!("{prefix}_{result_type_snake}_{resolved}");
                    let lookup_key = format!("{result_type_snake}.{resolved}");
                    if is_skipped_c_field(fields_c_types, &result_type_snake, resolved) {
                        // Field marked "skip" — record sentinel so render_assertion skips it.
                        primitive_locals.insert(local_var.clone(), "__skip__".to_string());
                    } else if let Some(t) = fields_c_types.get(&lookup_key).filter(|t| is_primitive_c_type(t)) {
                        let _ = writeln!(out, "    {t} {local_var} = {accessor_fn}({result_var});");
                        primitive_locals.insert(local_var.clone(), t.clone());
                    } else if try_emit_enum_accessor(
                        out,
                        prefix,
                        &prefix_upper,
                        f,
                        resolved,
                        &result_type_snake,
                        &accessor_fn,
                        result_var,
                        &local_var,
                        fields_c_types,
                        fields_enum,
                        &mut intermediate_handles,
                    ) {
                        // accessor emitted with enum-to-string conversion
                    } else if let Some(handle_pascal) =
                        infer_opaque_handle_type(fields_c_types, &result_type_snake, resolved)
                    {
                        let _ = writeln!(
                            out,
                            "    {prefix_upper}{handle_pascal}* {local_var} = {accessor_fn}({result_var});"
                        );
                        opaque_handle_locals.insert(local_var.clone(), handle_pascal.to_snake_case());
                    } else {
                        let _ = writeln!(out, "    char* {local_var} = {accessor_fn}({result_var});");
                    }
                }
                accessed_fields.push((f.clone(), local_var.clone(), has_map_access));
            }
        }
    }

    for assertion in &fixture.assertions {
        render_assertion(
            out,
            assertion,
            result_var,
            prefix,
            field_resolver,
            &accessed_fields,
            &primitive_locals,
            &opaque_handle_locals,
        );
    }

    // Free extracted leaf strings.
    for (_f, local_var, from_json) in &accessed_fields {
        if primitive_locals.contains_key(local_var) {
            continue;
        }
        if let Some(snake_type) = opaque_handle_locals.get(local_var) {
            let _ = writeln!(out, "    {prefix}_{snake_type}_free({local_var});");
            continue;
        }
        if *from_json {
            let _ = writeln!(out, "    free({local_var});");
        } else {
            let _ = writeln!(out, "    {prefix}_free_string({local_var});");
        }
    }
    // Free intermediate handles in reverse order.
    for (handle_var, snake_type) in intermediate_handles.iter().rev() {
        if snake_type == "free_string" {
            // free_string handles are freed with the free_string function directly.
            let _ = writeln!(out, "    {prefix}_free_string({handle_var});");
        } else if snake_type == "free" {
            // Intermediate JSON-key extraction (e.g. alef_json_array_get_index) — freed via plain free().
            let _ = writeln!(out, "    free({handle_var});");
        } else {
            let _ = writeln!(out, "    {prefix}_{snake_type}_free({handle_var});");
        }
    }
    if has_options_handle {
        let options_type_snake = options_type_name.to_snake_case();
        let _ = writeln!(out, "    {prefix}_{options_type_snake}_free(options_handle);");
    }
    let result_type_snake = result_type_name.to_snake_case();
    let _ = writeln!(out, "    {prefix}_{result_type_snake}_free({result_var});");
    let _ = writeln!(out, "}}");
}
