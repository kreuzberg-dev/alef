//! C e2e visitor fixture test generation.

use crate::core::config::ResolvedCrateConfig;
use crate::core::hash::{self, CommentStyle};
use crate::e2e::config::E2eConfig;
use crate::e2e::escape::{escape_c, sanitize_ident};
use crate::e2e::fixture::Fixture;
use heck::{ToPascalCase, ToSnakeCase};
use std::fmt::Write as FmtWrite;

use super::{json_to_c, resolve_call_info};

// ---------------------------------------------------------------------------
// Visitor test file generation for C FFI
// ---------------------------------------------------------------------------

/// Generate `test_visitor.c` — one test function per visitor-bearing fixture.
///
/// Each test:
/// 1. Defines static C callback functions for each configured callback slot.
/// 2. Zero-initialises the generated visitor callback struct and wires each slot.
/// 3. Creates a visitor handle via the configured FFI prefix.
/// 4. Creates an options handle via the resolved options type's `from_json` symbol.
/// 5. Attaches the visitor via the configured FFI prefix.
/// 6. Calls the configured C FFI function and serialises the result to JSON.
/// 7. Extracts fields via `alef_json_get_string` and runs `contains`/`not_contains`
///    assertions with `assert(…)`.
/// 8. Frees all handles in reverse allocation order.
pub(super) fn render_visitor_test_file(
    fixtures: &[&Fixture],
    header: &str,
    prefix: &str,
    e2e_config: &E2eConfig,
    _config: &ResolvedCrateConfig,
) -> String {
    use crate::e2e::fixture::CallbackAction;

    let mut out = String::new();
    out.push_str(&hash::header(CommentStyle::Block));
    let _ = writeln!(out, "/* E2e tests for category: visitor */");
    let _ = writeln!(out);
    let _ = writeln!(out, "#include <assert.h>");
    let _ = writeln!(out, "#include <stdint.h>");
    let _ = writeln!(out, "#include <string.h>");
    let _ = writeln!(out, "#include <stdio.h>");
    let _ = writeln!(out, "#include <stdlib.h>");
    let _ = writeln!(out, "#include \"{header}\"");
    let _ = writeln!(out, "#include \"test_runner.h\"");
    let _ = writeln!(out);

    let prefix_upper = prefix.to_uppercase();
    let visitor_type_stem = prefix.to_pascal_case();
    let visitor_callbacks_type = format!("{prefix_upper}{visitor_type_stem}VisitorCallbacks");
    // The C FFI re-defines the visitor context as a stem-prefixed struct
    // (mirror of `HtmVisitorCallbacks`) so the callback signatures take the
    // C-friendly view, NOT the opaque Rust `NodeContext` handle. Match the
    // FFI naming pattern instead of reading the trait_bridge.context_type
    // (which names the Rust-core type, not the FFI re-export).
    let visitor_context_type = format!("{prefix_upper}{visitor_type_stem}Context");
    let visitor_handle_type = format!("{prefix_upper}{visitor_type_stem}Visitor");

    for (i, fixture) in fixtures.iter().enumerate() {
        let fn_name = sanitize_ident(&fixture.id);
        let description = &fixture.description;
        let call_config = e2e_config.resolve_call_for_fixture(
            fixture.call.as_deref(),
            &fixture.id,
            &fixture.resolved_category(),
            &fixture.tags,
            &fixture.input,
        );
        let call_info = resolve_call_info(call_config, "c");
        let function_name = call_info.function_name.as_str();
        let options_type_name = call_info.options_type_name.as_str();
        let options_type_snake = options_type_name.to_snake_case();
        let result_type_name = call_info.result_type_name.as_str();
        let result_type_snake = result_type_name.to_snake_case();

        let visitor_spec = match &fixture.visitor {
            Some(v) => v,
            None => continue,
        };

        let html = fixture.input.get("html").and_then(|v| v.as_str()).unwrap_or("");
        let html_escaped = escape_c(html);

        let options_json = match fixture.input.get("options") {
            Some(opts) => serde_json::to_string(opts).unwrap_or_else(|_| "{}".to_string()),
            None => "{}".to_string(),
        };
        let options_escaped = escape_c(&options_json);

        // Emit static callback functions for this fixture. Each callback is named
        // `c_visitor_<fixture_id>_<method>` to avoid collisions across fixtures.
        let mut sorted_callbacks: Vec<(&String, &CallbackAction)> = visitor_spec.callbacks.iter().collect();
        sorted_callbacks.sort_by(|a, b| a.0.cmp(b.0));

        for (method, action) in &sorted_callbacks {
            let cb_name = format!("c_visitor_{fn_name}_{method}");
            let params = c_visitor_callback_params(method, &visitor_context_type);
            let body = c_visitor_callback_body(method, action);
            let _ = writeln!(out, "static int32_t {cb_name}({params}) {{");
            out.push_str(&body);
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }

        // Emit the test function.
        let _ = writeln!(out, "void test_{fn_name}(void) {{");
        let _ = writeln!(out, "    /* {description} */");
        let _ = writeln!(out);

        // Build callbacks struct and wire each slot.
        let _ = writeln!(out, "    {visitor_callbacks_type} _callbacks;");
        let _ = writeln!(out, "    memset(&_callbacks, 0, sizeof(_callbacks));");
        for (method, _) in &sorted_callbacks {
            let cb_name = format!("c_visitor_{fn_name}_{method}");
            let _ = writeln!(out, "    _callbacks.{method} = {cb_name};");
        }
        let _ = writeln!(out);

        // Create visitor handle.
        let _ = writeln!(
            out,
            "    {visitor_handle_type}* _visitor = {prefix}_visitor_create(&_callbacks);"
        );
        let _ = writeln!(out, "    assert(_visitor != NULL && \"visitor create failed\");");
        let _ = writeln!(out);

        // Create options handle.
        let _ = writeln!(
            out,
            "    {prefix_upper}{options_type_name}* _options = {prefix}_{options_type_snake}_from_json(\"{options_escaped}\");"
        );
        let _ = writeln!(out, "    assert(_options != NULL && \"options from_json failed\");");
        let _ = writeln!(out);

        // Attach visitor to options.
        let _ = writeln!(out, "    {prefix}_options_set_visitor(_options, _visitor);");
        let _ = writeln!(out);

        // Call the configured C FFI function.
        let _ = writeln!(
            out,
            "    {prefix_upper}{result_type_name}* _result = {function_name}(\"{html_escaped}\", _options);"
        );
        let _ = writeln!(out, "    assert(_result != NULL && \"visitor call failed\");");
        let _ = writeln!(out);

        // Serialise result to JSON and extract the content field.
        let _ = writeln!(out, "    char* _json = {prefix}_{result_type_snake}_to_json(_result);");
        let _ = writeln!(out, "    assert(_json != NULL && \"result to_json failed\");");
        let _ = writeln!(out, "    char* _content = alef_json_get_string(_json, \"content\");");
        let _ = writeln!(out);

        // Emit assertions (only contains/not_contains; visitor fixtures use only these).
        for assertion in &fixture.assertions {
            match assertion.assertion_type.as_str() {
                "contains" => {
                    if let Some(expected) = &assertion.value {
                        let c_val = json_to_c(expected);
                        let _ = writeln!(
                            out,
                            "    assert(_content != NULL && strstr(_content, {c_val}) != NULL && \"expected to contain substring\");"
                        );
                    }
                }
                "not_contains" => {
                    if let Some(expected) = &assertion.value {
                        let c_val = json_to_c(expected);
                        let _ = writeln!(
                            out,
                            "    assert((_content == NULL || strstr(_content, {c_val}) == NULL) && \"expected NOT to contain substring\");"
                        );
                    }
                }
                other => {
                    let _ = writeln!(
                        out,
                        "    /* assertion type '{other}' not supported in C visitor tests */"
                    );
                }
            }
        }

        let _ = writeln!(out);

        // Free in reverse allocation order.
        let _ = writeln!(out, "    free(_content);");
        let _ = writeln!(out, "    {prefix}_free_string(_json);");
        let _ = writeln!(out, "    {prefix}_{result_type_snake}_free(_result);");
        let _ = writeln!(out, "    {prefix}_{options_type_snake}_free(_options);");
        let _ = writeln!(out, "    {prefix}_visitor_free(_visitor);");
        let _ = writeln!(out, "}}");

        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    out
}

/// C function-pointer parameter list for a given visitor callback method.
///
/// Mirrors the cbindgen-emitted visitor callback slot signatures from
/// the generated FFI header. Named parameters
/// are prefixed with `_` so the C compiler does not warn about unused params when
/// the callback body ignores them.
fn c_visitor_callback_params(method: &str, context_type: &str) -> String {
    match method {
        "visit_text" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _text, char** out_custom, size_t* out_len"
            )
        }
        "visit_element_start" => {
            format!("const {context_type}* _ctx, void* _user_data, char** out_custom, size_t* out_len")
        }
        "visit_element_end" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _output, char** out_custom, size_t* out_len"
            )
        }
        "visit_link" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _href, const char* _text, const char* _title, char** out_custom, size_t* out_len"
            )
        }
        "visit_image" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _src, const char* _alt, const char* _title, char** out_custom, size_t* out_len"
            )
        }
        "visit_heading" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, uint32_t _level, const char* _text, const char* _id, char** out_custom, size_t* out_len"
            )
        }
        "visit_code_block" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _lang, const char* _code, char** out_custom, size_t* out_len"
            )
        }
        "visit_code_inline" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _code, char** out_custom, size_t* out_len"
            )
        }
        "visit_list_item" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, int32_t _ordered, const char* _marker, const char* _text, char** out_custom, size_t* out_len"
            )
        }
        "visit_list_start" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, int32_t _ordered, char** out_custom, size_t* out_len"
            )
        }
        "visit_list_end" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, int32_t _ordered, const char* _output, char** out_custom, size_t* out_len"
            )
        }
        "visit_table_start" => {
            format!("const {context_type}* _ctx, void* _user_data, char** out_custom, size_t* out_len")
        }
        "visit_table_row" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* const* _cells, size_t _cell_count, int32_t _is_header, char** out_custom, size_t* out_len"
            )
        }
        "visit_table_end" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _output, char** out_custom, size_t* out_len"
            )
        }
        "visit_blockquote" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _content, size_t _depth, char** out_custom, size_t* out_len"
            )
        }
        "visit_line_break" | "visit_horizontal_rule" | "visit_definition_list_start" | "visit_figure_start" => {
            format!("const {context_type}* _ctx, void* _user_data, char** out_custom, size_t* out_len")
        }
        "visit_custom_element" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _tag_name, const char* _html, char** out_custom, size_t* out_len"
            )
        }
        "visit_form" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _action, const char* _method, char** out_custom, size_t* out_len"
            )
        }
        "visit_input" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _input_type, const char* _name, const char* _value, char** out_custom, size_t* out_len"
            )
        }
        "visit_audio" | "visit_video" | "visit_iframe" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _src, char** out_custom, size_t* out_len"
            )
        }
        "visit_details" => {
            format!("const {context_type}* _ctx, void* _user_data, int32_t _open, char** out_custom, size_t* out_len")
        }
        "visit_figure_end" | "visit_definition_list_end" => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _output, char** out_custom, size_t* out_len"
            )
        }
        // Default: single text payload (covers visit_strong, visit_emphasis,
        // visit_strikethrough, visit_underline, visit_subscript, visit_superscript,
        // visit_mark, visit_button, visit_summary, visit_figcaption,
        // visit_definition_term, visit_definition_description).
        _ => {
            format!(
                "const {context_type}* _ctx, void* _user_data, const char* _text, char** out_custom, size_t* out_len"
            )
        }
    }
}

/// Build the body of a C visitor callback function for a given action.
///
/// Return values mirror the legacy visitor FFI discriminants:
///   0 = Continue, 1 = Skip, 2 = PreserveHtml, 3 = Custom.
///
/// For `Custom` and `CustomTemplate`, we heap-allocate a copy of the output string
/// with `strdup` (or a sprintf-allocated buffer) and pass its pointer and length back
/// via `out_custom`/`out_len`. The FFI runtime takes ownership and frees it.
fn c_visitor_callback_body(method: &str, action: &crate::e2e::fixture::CallbackAction) -> String {
    use crate::e2e::fixture::CallbackAction;

    let mut out = String::new();
    // Suppress unused-parameter warnings for context and user_data — always ignored
    // in simple e2e test callbacks.
    let _ = writeln!(out, "    (void)_ctx;");
    let _ = writeln!(out, "    (void)_user_data;");

    match action {
        CallbackAction::Skip => {
            let _ = writeln!(out, "    (void)out_custom;");
            let _ = writeln!(out, "    (void)out_len;");
            // Suppress method-specific params not used by Skip.
            for param in c_visitor_unused_params(method) {
                let _ = writeln!(out, "    (void){param};");
            }
            let _ = writeln!(out, "    return 1;");
        }
        CallbackAction::Continue => {
            let _ = writeln!(out, "    (void)out_custom;");
            let _ = writeln!(out, "    (void)out_len;");
            for param in c_visitor_unused_params(method) {
                let _ = writeln!(out, "    (void){param};");
            }
            let _ = writeln!(out, "    return 0;");
        }
        CallbackAction::PreserveHtml => {
            let _ = writeln!(out, "    (void)out_custom;");
            let _ = writeln!(out, "    (void)out_len;");
            for param in c_visitor_unused_params(method) {
                let _ = writeln!(out, "    (void){param};");
            }
            let _ = writeln!(out, "    return 2;");
        }
        CallbackAction::Custom { output } => {
            let escaped = escape_c(output);
            for param in c_visitor_unused_params(method) {
                let _ = writeln!(out, "    (void){param};");
            }
            let _ = writeln!(out, "    char* _buf = strdup(\"{escaped}\");");
            let _ = writeln!(out, "    if (out_custom) *out_custom = _buf;");
            let _ = writeln!(out, "    if (out_len) *out_len = _buf ? strlen(_buf) : 0;");
            let _ = writeln!(out, "    return 3;");
        }
        CallbackAction::CustomTemplate { template, .. } => {
            // Build a sprintf format string and map fixture placeholders to C params.
            let (c_fmt, placeholders) = c_visitor_template_to_sprintf(template);
            let escaped_fmt = escape_c(&c_fmt);

            // Determine which method-specific params are used by the template.
            let used: std::collections::HashSet<&str> = placeholders.iter().map(|s| s.as_str()).collect();
            for param in c_visitor_unused_params(method) {
                let stripped = param.trim_start_matches('_');
                if !used.contains(stripped) {
                    let _ = writeln!(out, "    (void){param};");
                }
            }

            if placeholders.is_empty() {
                let _ = writeln!(out, "    char* _buf = strdup(\"{escaped_fmt}\");");
            } else {
                // Compute the max output length. We over-estimate by adding 256 per
                // placeholder plus the template length.
                let max_len = template.len() + placeholders.len() * 256 + 64;
                let _ = writeln!(out, "    char* _buf = (char*)malloc({max_len});");
                let _ = writeln!(out, "    if (!_buf) {{ (void)out_custom; (void)out_len; return 0; }}");
                // Build the sprintf argument list.
                let args: Vec<String> = placeholders
                    .iter()
                    .map(|name| c_visitor_placeholder_to_arg(method, name))
                    .collect();
                let args_str = args.join(", ");
                let _ = writeln!(out, "    snprintf(_buf, {max_len}, \"{escaped_fmt}\", {args_str});");
            }

            let _ = writeln!(out, "    if (out_custom) *out_custom = _buf;");
            let _ = writeln!(out, "    if (out_len) *out_len = _buf ? strlen(_buf) : 0;");
            let _ = writeln!(out, "    return 3;");
        }
    }

    out
}

/// List of method-specific typed C parameter names to suppress with `(void)` when
/// the callback body does not reference them.  Mirrors `unused_params_for` in
/// `zig_visitors.rs` but uses the C parameter names from `c_visitor_callback_params`.
fn c_visitor_unused_params(method: &str) -> Vec<&'static str> {
    match method {
        "visit_text" => vec!["_text"],
        "visit_element_start"
        | "visit_table_start"
        | "visit_line_break"
        | "visit_horizontal_rule"
        | "visit_definition_list_start"
        | "visit_figure_start" => vec![],
        "visit_element_end" | "visit_table_end" | "visit_figure_end" | "visit_definition_list_end" => {
            vec!["_output"]
        }
        "visit_link" => vec!["_href", "_text", "_title"],
        "visit_image" => vec!["_src", "_alt", "_title"],
        "visit_heading" => vec!["_level", "_text", "_id"],
        "visit_code_block" => vec!["_lang", "_code"],
        "visit_code_inline" => vec!["_code"],
        "visit_list_item" => vec!["_ordered", "_marker", "_text"],
        "visit_list_start" => vec!["_ordered"],
        "visit_list_end" => vec!["_ordered", "_output"],
        "visit_table_row" => vec!["_cells", "_cell_count", "_is_header"],
        "visit_blockquote" => vec!["_content", "_depth"],
        "visit_custom_element" => vec!["_tag_name", "_html"],
        "visit_form" => vec!["_action", "_method"],
        "visit_input" => vec!["_input_type", "_name", "_value"],
        "visit_audio" | "visit_video" | "visit_iframe" => vec!["_src"],
        "visit_details" => vec!["_open"],
        // Default: text-only methods.
        _ => vec!["_text"],
    }
}

/// Convert a fixture `{placeholder}` template into a `printf`/`snprintf` format string
/// and an ordered list of placeholder names.  Integer placeholders use `%d` or `%u`;
/// everything else uses `%s`.
fn c_visitor_template_to_sprintf(template: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(template.len());
    let mut placeholders: Vec<String> = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut name = String::new();
                while let Some(&peek) = chars.peek() {
                    if peek == '}' {
                        chars.next();
                        break;
                    }
                    name.push(peek);
                    chars.next();
                }
                let is_int = matches!(name.as_str(), "level" | "depth" | "ordered" | "open" | "is_header");
                if is_int {
                    out.push_str("%d");
                } else {
                    out.push_str("%s");
                }
                placeholders.push(name);
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                }
                out.push('}');
            }
            '%' => {
                // Escape literal percent signs for printf.
                out.push_str("%%");
            }
            other => out.push(other),
        }
    }
    (out, placeholders)
}

/// Map a fixture placeholder name (e.g. `href`, `text`) to the C expression that
/// yields the value for that parameter slot in the callback's sprintf call.
fn c_visitor_placeholder_to_arg(method: &str, name: &str) -> String {
    let int_placeholder = matches!(
        (method, name),
        ("visit_heading", "level")
            | ("visit_blockquote", "depth")
            | ("visit_list_item", "ordered")
            | ("visit_list_start", "ordered")
            | ("visit_list_end", "ordered")
            | ("visit_details", "open")
            | ("visit_table_row", "is_header")
    );
    if int_placeholder {
        return format!("_{name}");
    }
    // String parameters — use the named `_<name>` C param directly.
    // The C param is already a `const char*`; pass it directly to `%s`.
    // Guard against NULL to avoid UB in printf (some implementations crash on NULL %s).
    format!("(_{name} ? _{name} : \"\")")
}

#[cfg(test)]
mod visitor_tests {
    use super::super::c_visitor_fixture_has_typed_call;
    use super::render_visitor_test_file;
    use crate::core::config::e2e::{CallConfig, CallOverride, E2eConfig};
    use crate::core::config::{ResolvedCrateConfig, TraitBridgeConfig};
    use crate::e2e::fixture::{Assertion, CallbackAction, Fixture, VisitorSpec};
    use std::collections::BTreeMap;

    fn visitor_fixture() -> Fixture {
        let mut callbacks = BTreeMap::new();
        callbacks.insert("visit_text".to_string(), CallbackAction::Continue);

        Fixture {
            id: "custom_names".to_string(),
            category: None,
            description: "uses configured names".to_string(),
            tags: vec![],
            skip: None,
            env: None,
            call: None,
            input: serde_json::json!({
                "html": "<p>Hello</p>",
                "options": { "trim": true }
            }),
            mock_response: None,
            visitor: Some(VisitorSpec { callbacks }),
            args: vec![],
            assertion_recipes: vec![],
            assertions: vec![Assertion {
                assertion_type: "contains".to_string(),
                field: None,
                value: Some(serde_json::json!("Hello")),
                values: None,
                method: None,
                check: None,
                args: None,
                return_type: None,
            }],
            source: String::new(),
            http: None,
        }
    }

    fn e2e_config_with_c_call() -> E2eConfig {
        let c_override = CallOverride {
            function: Some("krz_render_document".to_string()),
            prefix: Some("krz".to_string()),
            options_type: Some("RenderConfig".to_string()),
            result_type: Some("RenderOutput".to_string()),
            ..Default::default()
        };
        let call = CallConfig {
            function: "render_document".to_string(),
            overrides: [("c".to_string(), c_override)].into(),
            ..Default::default()
        };
        E2eConfig {
            call,
            ..Default::default()
        }
    }

    fn crate_config_with_visitor_metadata() -> ResolvedCrateConfig {
        ResolvedCrateConfig {
            trait_bridges: vec![TraitBridgeConfig {
                trait_name: "Renderer".to_string(),
                context_type: Some("RenderContext".to_string()),
                result_type: Some("RenderDecision".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn c_visitor_file_uses_configured_call_types_and_symbols() {
        let fixture = visitor_fixture();
        let fixtures = vec![&fixture];
        let config = crate_config_with_visitor_metadata();
        let content = render_visitor_test_file(&fixtures, "krz.h", "krz", &e2e_config_with_c_call(), &config);

        assert!(content.contains("KRZKrzVisitorCallbacks _callbacks"));
        assert!(content.contains("const KRZRenderContext* _ctx"));
        assert!(content.contains("KRZRenderConfig* _options = krz_render_config_from_json"));
        assert!(content.contains("KRZRenderOutput* _result = krz_render_document"));
        assert!(content.contains("char* _json = krz_render_output_to_json(_result);"));
        assert!(content.contains("krz_render_output_free(_result);"));
        assert!(content.contains("krz_render_config_free(_options);"));

        for hardcoded in [
            "DefaultOptions",
            "DefaultResult",
            "conversion_options_from_json",
            "conversion_result_to_json",
            "default_convert",
            "DEFDftVisitorCallbacks",
            "DEFDftSyntaxContext",
            "KRZKrzSyntaxContext",
        ] {
            assert!(
                !content.contains(hardcoded),
                "visitor C output leaked `{hardcoded}`:\n{content}"
            );
        }
    }

    #[test]
    fn c_visitor_fixture_without_typed_c_call_is_not_eligible() {
        let fixture = visitor_fixture();
        let config = E2eConfig::default();

        assert!(
            !c_visitor_fixture_has_typed_call(&fixture, &config),
            "visitor fixtures need a configured C function and options type"
        );
    }
}
