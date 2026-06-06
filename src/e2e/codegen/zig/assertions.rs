use super::*;

const FORMAT_METADATA_VARIANTS: &[&str] = &[
    "pdf",
    "docx",
    "excel",
    "email",
    "pptx",
    "archive",
    "image",
    "xml",
    "text",
    "html",
    "ocr",
    "csv",
    "bibtex",
    "citation",
    "fiction_book",
    "dbf",
    "jats",
    "epub",
    "pst",
    "code",
];

fn json_path_expr(result_var: &str, field_path: &str) -> String {
    let segments: Vec<&str> = field_path.split('.').collect();
    let mut expr = result_var.to_string();
    let mut prev_seg: Option<&str> = None;
    for seg in &segments {
        // Skip variant-name accessor segments that follow a `format` key.
        // FormatMetadata is an internally-tagged enum (`#[serde(tag = "format_type")]`),
        // so variant fields are flattened directly into the format object — there is no
        // intermediate JSON key for the variant name.
        if prev_seg == Some("format") && FORMAT_METADATA_VARIANTS.contains(seg) {
            prev_seg = Some(seg);
            continue;
        }
        // Handle array accessor notation:
        //   "links[]"     → access the array, then first element.
        //   "results[0]"  → access the array, then specific index N.
        if let Some(key) = seg.strip_suffix("[]") {
            expr = format!("{expr}.object.get(\"{key}\").?.array.items[0]");
        } else if let Some(bracket_pos) = seg.find('[') {
            if let Some(end_pos) = seg.find(']') {
                if end_pos > bracket_pos + 1 && end_pos == seg.len() - 1 {
                    let key = &seg[..bracket_pos];
                    let idx = &seg[bracket_pos + 1..end_pos];
                    if idx.chars().all(|c| c.is_ascii_digit()) {
                        expr = format!("{expr}.object.get(\"{key}\").?.array.items[{idx}]");
                        prev_seg = Some(seg);
                        continue;
                    }
                    // Non-numeric bracket: HashMap<String, _> key access. FRB / serde
                    // serialize maps as JSON objects, so `field[key]` resolves to
                    // `.object.get("field").?.object.get("key").?`. Used by nested fixture objects.
                    // `metadata.document.open_graph[title]` alias pattern where
                    // `open_graph` is a `HashMap<String, String>`.
                    expr = format!("{expr}.object.get(\"{key}\").?.object.get(\"{idx}\").?");
                    prev_seg = Some(seg);
                    continue;
                }
            }
            expr = format!("{expr}.object.get(\"{seg}\").?");
        } else {
            expr = format!("{expr}.object.get(\"{seg}\").?");
        }
        prev_seg = Some(seg);
    }
    expr
}

/// Emit a Zig predicate over the `chunks` array of a JSON-parsed extraction
/// result. The predicate body should be a Zig expression yielding an
/// `?std.json.Value` for each chunk element bound as `c`. When `require_non_empty_string`
/// is `true`, the predicate also requires the value to be a non-empty string.
fn emit_zig_chunks_predicate(
    out: &mut String,
    result_var: &str,
    assertion_type: &str,
    chunk_field_accessor: &str,
    field_name: &str,
    require_non_empty_string: bool,
) {
    let _ = writeln!(out, "    {{");
    let _ = writeln!(out, "        const _chunks_opt = {result_var}.object.get(\"chunks\");");
    let _ = writeln!(out, "        var _all: bool = true;");
    let _ = writeln!(out, "        if (_chunks_opt) |_chunks_val| {{");
    let _ = writeln!(out, "            if (_chunks_val == .array) {{");
    let _ = writeln!(
        out,
        "                if (_chunks_val.array.items.len == 0) _all = false;"
    );
    let _ = writeln!(out, "                for (_chunks_val.array.items) |c| {{");
    let _ = writeln!(out, "                    if (c != .object) {{ _all = false; break; }}");
    let _ = writeln!(out, "                    const _v = {chunk_field_accessor};");
    if require_non_empty_string {
        let _ = writeln!(
            out,
            "                    if (_v == null or _v.? != .string or _v.?.string.len == 0) {{ _all = false; break; }}"
        );
    } else {
        let _ = writeln!(
            out,
            "                    if (_v == null or _v.? == .null) {{ _all = false; break; }}"
        );
    }
    let _ = writeln!(out, "                }}");
    let _ = writeln!(out, "            }} else {{ _all = false; }}");
    let _ = writeln!(out, "        }} else {{ _all = false; }}");
    match assertion_type {
        "is_true" => {
            let _ = writeln!(out, "        try testing.expect(_all);");
        }
        "is_false" => {
            let _ = writeln!(out, "        try testing.expect(!_all);");
        }
        _ => {
            let _ = writeln!(
                out,
                "        // skipped: unsupported assertion type on synthetic field '{field_name}'"
            );
        }
    }
    let _ = writeln!(out, "    }}");
}

/// Render a single assertion for a JSON-struct result (result_is_json_struct = true).
///
/// The `result_var` variable is `*std.json.Value` (pointer to the parsed root object).
/// Field paths are traversed via `.object.get("key").?` chains.
pub(super) fn render_json_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_var: &str,
    field_resolver: &FieldResolver,
    uses_streaming: bool,
) {
    // Intercept streaming-virtual fields before the result-type validity check,
    // but ONLY when the test is actually using the streaming-virtual path.
    // When `uses_streaming = false` the `chunks` local is never declared, so
    // generating `chunks.items.len` would produce a compile error. Fields like
    // "chunks" that happen to share a streaming-virtual name are regular JSON
    // fields in non-streaming results and must fall through to the JSON path.
    if let Some(f) = &assertion.field {
        if uses_streaming && !f.is_empty() && is_streaming_virtual_field(f) {
            if let Some(expr) = StreamingFieldResolver::accessor(f, "zig", "chunks") {
                match assertion.assertion_type.as_str() {
                    "count_min" => {
                        if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                            let _ = writeln!(out, "    try testing.expect({expr}.len >= {n});");
                        }
                    }
                    "count_equals" => {
                        if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                            let _ = writeln!(out, "    try testing.expectEqual(@as(usize, {n}), {expr}.len);");
                        }
                    }
                    "equals" => {
                        if let Some(serde_json::Value::String(s)) = &assertion.value {
                            let escaped = escape_zig(s);
                            let _ = writeln!(out, "    try testing.expectEqualStrings(\"{escaped}\", {expr});");
                        } else if let Some(v) = &assertion.value {
                            let zig_val = json_to_zig(v);
                            let _ = writeln!(out, "    try testing.expectEqual({zig_val}, {expr});");
                        }
                    }
                    "not_empty" => {
                        let _ = writeln!(out, "    try testing.expect({expr}.len > 0);");
                    }
                    "is_true" => {
                        let _ = writeln!(out, "    try testing.expect({expr});");
                    }
                    "is_false" => {
                        let _ = writeln!(out, "    try testing.expect(!{expr});");
                    }
                    _ => {
                        let atype = &assertion.assertion_type;
                        let _ = writeln!(
                            out,
                            "    // streaming virtual field '{f}' assertion '{atype}' not implemented for zig"
                        );
                    }
                }
            }
            return;
        }
    }

    // Synthetic `embeddings` field on a JSON-array result (e.g. embed_texts
    // returns `Vec<Vec<f32>>` → JSON `[[...],[...]]`). The field name is a
    // convention from the fixture schema — the JSON value IS the embeddings
    // array. Apply the assertion against `result.array.items` directly. The
    // synthetic path is only used when no explicit result_fields configure
    // `embeddings` as a real struct field.
    if let Some(f) = &assertion.field {
        if f == "embeddings" && !field_resolver.has_explicit_field("embeddings") {
            match assertion.assertion_type.as_str() {
                "count_min" => {
                    if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                        let _ = writeln!(out, "    try testing.expect({result_var}.array.items.len >= {n});");
                    }
                    return;
                }
                "count_equals" => {
                    if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                        let _ = writeln!(
                            out,
                            "    try testing.expectEqual(@as(usize, {n}), {result_var}.array.items.len);"
                        );
                    }
                    return;
                }
                "not_empty" => {
                    let _ = writeln!(out, "    try testing.expect({result_var}.array.items.len > 0);");
                    return;
                }
                "is_empty" => {
                    let _ = writeln!(
                        out,
                        "    try testing.expectEqual(@as(usize, 0), {result_var}.array.items.len);"
                    );
                    return;
                }
                _ => {}
            }
        }
    }

    // Synthesised chunk-inspection virtual fields. These are not real JSON
    // fields but are derived predicates over a result object's `chunks` array.
    // Other backends (python, ruby, java, etc.) compute
    // these inline; zig parses to `std.json.Value`, so we compute them
    // against `result.object.get("chunks").?.array`.
    if let Some(f) = &assertion.field {
        match f.as_str() {
            "chunks_have_content" => {
                emit_zig_chunks_predicate(
                    out,
                    result_var,
                    assertion.assertion_type.as_str(),
                    "c.object.get(\"content\")",
                    "chunks_have_content",
                    true,
                );
                return;
            }
            "chunks_have_heading_context" => {
                // `heading_context` is `Option<HeadingContext>` and serde drops
                // `None` from the JSON, so chunks without a heading produce no
                // key — making an "all chunks have it" predicate spuriously
                // fail. Matching the Ruby codegen, skip this synthetic field.
                let _ = writeln!(
                    out,
                    "    // skipped: synthetic field 'chunks_have_heading_context' not derivable from JSON value alone"
                );
                return;
            }
            "first_chunk_starts_with_heading" => {
                let _ = writeln!(
                    out,
                    "    // skipped: synthetic field 'first_chunk_starts_with_heading' not derivable from JSON value alone"
                );
                return;
            }
            "chunks_have_embeddings" => {
                emit_zig_chunks_predicate(
                    out,
                    result_var,
                    assertion.assertion_type.as_str(),
                    "c.object.get(\"embedding\")",
                    "chunks_have_embeddings",
                    false,
                );
                return;
            }
            // `keywords` is a fixture alias that does not map cleanly onto the
            // serialized JSON result shape. Matching the Python codegen, skip.
            "keywords" | "keywords_count" => {
                let _ = writeln!(
                    out,
                    "    // skipped: field '{f}' not available on the JSON-struct result"
                );
                return;
            }
            _ => {}
        }
    }

    // Skip assertions on fields that don't exist on the result type.
    if let Some(f) = &assertion.field {
        if !f.is_empty() && !field_resolver.is_valid_for_result(f) {
            let _ = writeln!(out, "    // skipped: field '{f}' not available on result type");
            return;
        }
    }
    // error/not_error are handled at the call level, not assertion level.
    if matches!(assertion.assertion_type.as_str(), "not_error" | "error") {
        return;
    }

    let raw_field_path = assertion.field.as_deref().unwrap_or("").trim();
    let field_path = if raw_field_path.is_empty() {
        raw_field_path.to_string()
    } else {
        field_resolver.resolve(raw_field_path).to_string()
    };
    let field_path = field_path.trim();

    // "{array_field}.length" → strip suffix; use .array.items.len in the template.
    let (field_path_for_expr, is_length_access) = if let Some(parent) = field_path.strip_suffix(".length") {
        (parent, true)
    } else {
        (field_path, false)
    };

    let field_expr = if field_path_for_expr.is_empty() {
        result_var.to_string()
    } else {
        json_path_expr(result_var, field_path_for_expr)
    };

    // Special-case `metadata.format` equals-string: `FormatMetadata` is an
    // internally-tagged enum serialized as a JSON object (`{"format_type": "image",
    // "format": "PNG", ...}`), so `metadata.format` resolves to a JSON object,
    // not a string. The fixture asserts the `Display` impl: for Image variant
    // emit the inner `format` field; otherwise emit the `format_type` discriminant.
    if field_path_for_expr == "metadata.format"
        && matches!(
            assertion.assertion_type.as_str(),
            "equals" | "contains" | "not_empty" | "is_empty" | "starts_with" | "ends_with"
        )
    {
        let base = json_path_expr(result_var, field_path_for_expr);
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "        const _fmt_obj = {base}.object;");
        let _ = writeln!(out, "        const _fmt_type = _fmt_obj.get(\"format_type\").?.string;");
        let _ = writeln!(
            out,
            "        const _fmt_display: []const u8 = if (std.mem.eql(u8, _fmt_type, \"image\")) _fmt_obj.get(\"format\").?.string else _fmt_type;"
        );
        match assertion.assertion_type.as_str() {
            "equals" => {
                if let Some(serde_json::Value::String(s)) = &assertion.value {
                    let escaped = escape_zig(s);
                    let _ = writeln!(
                        out,
                        "        try testing.expectEqualStrings(\"{escaped}\", std.mem.trim(u8, _fmt_display, \" \\n\\r\\t\"));"
                    );
                }
            }
            "contains" => {
                if let Some(serde_json::Value::String(s)) = &assertion.value {
                    let escaped = escape_zig(s);
                    let _ = writeln!(
                        out,
                        "        try testing.expect(std.mem.indexOf(u8, _fmt_display, \"{escaped}\") != null);"
                    );
                }
            }
            "starts_with" => {
                if let Some(serde_json::Value::String(s)) = &assertion.value {
                    let escaped = escape_zig(s);
                    let _ = writeln!(
                        out,
                        "        try testing.expect(std.mem.startsWith(u8, _fmt_display, \"{escaped}\"));"
                    );
                }
            }
            "ends_with" => {
                if let Some(serde_json::Value::String(s)) = &assertion.value {
                    let escaped = escape_zig(s);
                    let _ = writeln!(
                        out,
                        "        try testing.expect(std.mem.endsWith(u8, _fmt_display, \"{escaped}\"));"
                    );
                }
            }
            "not_empty" => {
                let _ = writeln!(out, "        try testing.expect(_fmt_display.len > 0);");
            }
            "is_empty" => {
                let _ = writeln!(out, "        try testing.expectEqual(@as(usize, 0), _fmt_display.len);");
            }
            _ => {}
        }
        let _ = writeln!(out, "    }}");
        return;
    }

    // Compute context variables for the template.
    let zig_val = match &assertion.value {
        Some(serde_json::Value::String(s)) => format!("\"{}\"", escape_zig(s)),
        _ => String::new(),
    };
    let is_string_val = matches!(&assertion.value, Some(serde_json::Value::String(_)));
    let is_bool_val = matches!(&assertion.value, Some(serde_json::Value::Bool(_)));
    let bool_val = match &assertion.value {
        Some(serde_json::Value::Bool(b)) if *b => "true",
        _ => "false",
    };
    let is_null_val = matches!(&assertion.value, Some(serde_json::Value::Null));
    let n = assertion.value.as_ref().map(json_to_zig).unwrap_or_default();
    let has_n = assertion.value.as_ref().is_some_and(|v| v.is_number() || v.is_u64());
    // Distinguish float vs integer JSON values: `std.json.Value` exposes
    // `.integer` (i64) and `.float` (f64) as separate variants. Comparing
    // `.integer` against a literal with a fractional part (e.g. `0.9`) is a
    // Zig compile error, so the template must select the right tag.
    let is_float_val = matches!(&assertion.value, Some(serde_json::Value::Number(n)) if !n.is_i64() && !n.is_u64());
    let n_as_i64 = if has_n {
        format!("@as(i64, {})", n)
    } else {
        String::new()
    };
    // For usize comparisons, use i64 if n is negative (can't cast -1 to usize directly).
    // Zig comparison operators handle i64 on both sides implicitly.
    let n_as_usize = if has_n {
        if n.starts_with('-') {
            format!("@as(i64, {})", n)
        } else {
            format!("@as(usize, {})", n)
        }
    } else {
        String::new()
    };
    let n_as_f64 = if is_float_val {
        format!("@as(f64, {})", n)
    } else {
        String::new()
    };
    let values_list: Vec<String> = assertion
        .values
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| {
            if let serde_json::Value::String(s) = v {
                Some(format!("\"{}\"", escape_zig(s)))
            } else {
                None
            }
        })
        .collect();

    let rendered = crate::e2e::template_env::render(
        "zig/json_assertion.jinja",
        minijinja::context! {
            assertion_type => assertion.assertion_type.as_str(),
            field_expr => field_expr,
            is_length_access => is_length_access,
            zig_val => zig_val,
            is_string_val => is_string_val,
            is_bool_val => is_bool_val,
            bool_val => bool_val,
            is_null_val => is_null_val,
            n => n,
            n_as_i64 => n_as_i64,
            n_as_usize => n_as_usize,
            n_as_f64 => n_as_f64,
            has_n => has_n,
            is_float_val => is_float_val,
            values_list => values_list,
        },
    );
    out.push_str(&rendered);
}

/// Predicate matching `render_assertion`: returns true when the assertion
/// would emit at least one statement that references the result variable.
pub(super) fn assertion_emits_code(assertion: &Assertion, field_resolver: &FieldResolver) -> bool {
    if let Some(f) = &assertion.field {
        if !f.is_empty() && is_streaming_virtual_field(f) {
            // Streaming virtual fields always emit code — they are handled in a
            // dedicated collect path, not skipped.
        } else if !f.is_empty() && !field_resolver.is_valid_for_result(f) {
            return false;
        }
    }
    matches!(
        assertion.assertion_type.as_str(),
        "equals"
            | "contains"
            | "contains_all"
            | "not_contains"
            | "not_empty"
            | "is_empty"
            | "starts_with"
            | "ends_with"
            | "min_length"
            | "max_length"
            | "count_min"
            | "count_equals"
            | "is_true"
            | "is_false"
            | "greater_than"
            | "less_than"
            | "greater_than_or_equal"
            | "less_than_or_equal"
            | "contains_any"
    )
}

/// Build setup lines and the argument list for the function call.
///
/// Returns `(setup_lines, args_str, setup_needs_gpa)` where `setup_needs_gpa`
/// is `true` when at least one setup line requires the GPA `allocator` binding.
pub(super) fn render_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_var: &str,
    field_resolver: &FieldResolver,
    enum_fields: &HashSet<String>,
    result_is_option: bool,
    result_is_simple: bool,
) {
    // Bare-result assertions on `?T` (Optional) translate to null-checks instead
    // of `.len`. Mirrors the same behaviour in kotlin.rs (bare_result_is_option).
    let bare_result_is_option = result_is_option && assertion.field.as_deref().filter(|f| !f.is_empty()).is_none();
    if bare_result_is_option {
        match assertion.assertion_type.as_str() {
            "is_empty" => {
                let _ = writeln!(out, "    try testing.expect({result_var} == null);");
                return;
            }
            "not_empty" => {
                let _ = writeln!(out, "    try testing.expect({result_var} != null);");
                return;
            }
            "not_error" => {
                // not_error is covered by `try` propagation — the call would have
                // returned early on error. Emit a comment-only line so the assertion
                // is visible but inert, avoiding contradictory checks when paired
                // with `is_empty` on an Optional result.
                let _ = writeln!(out, "    // not_error: covered by try propagation");
                return;
            }
            "equals" => {
                if let Some(expected) = &assertion.value {
                    let zig_val = json_to_zig(expected);
                    let _ = writeln!(out, "    try testing.expectEqualStrings({zig_val}, {result_var}.?);");
                    return;
                }
            }
            _ => {}
        }
    }
    // Synthetic-field 'embeddings' on a JSON-bytes result (e.g. embed_texts
    // returns `Vec<Vec<f32>>` serialised as JSON). Parse the JSON array and
    // apply count_min/count_equals/not_empty/is_empty against the element count.
    //
    // The Zig binding for `Vec<T>`/`result_is_array` returns `[]u8` (the JSON
    // payload), not a typed struct — so a fixture field named `embeddings` is
    // a convention for "the bare JSON array is the embeddings". Gate on
    // `has_explicit_field` rather than `is_valid_for_result`, because the
    // latter is permissive (returns true) when `result_fields` is empty —
    // which is the common case for these bare-JSON returns and would
    // wrongly route through `result.embeddings.len` direct field access on
    // a `[]u8` slice.
    if let Some(f) = &assertion.field {
        if f == "embeddings" && !field_resolver.has_explicit_field(f) {
            match assertion.assertion_type.as_str() {
                "count_min" | "count_equals" | "not_empty" | "is_empty" => {
                    let _ = writeln!(out, "    {{");
                    let _ = writeln!(
                        out,
                        "        var _eparse = try std.json.parseFromSlice(std.json.Value, std.heap.c_allocator, {result_var}, .{{}});"
                    );
                    let _ = writeln!(out, "        defer _eparse.deinit();");
                    let _ = writeln!(out, "        const _embeddings_len = _eparse.value.array.items.len;");
                    match assertion.assertion_type.as_str() {
                        "count_min" => {
                            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                                let _ = writeln!(out, "        try testing.expect(_embeddings_len >= {n});");
                            }
                        }
                        "count_equals" => {
                            if let Some(n) = assertion.value.as_ref().and_then(|v| v.as_u64()) {
                                let _ = writeln!(
                                    out,
                                    "        try testing.expectEqual(@as(usize, {n}), _embeddings_len);"
                                );
                            }
                        }
                        "not_empty" => {
                            let _ = writeln!(out, "        try testing.expect(_embeddings_len > 0);");
                        }
                        "is_empty" => {
                            let _ = writeln!(out, "        try testing.expectEqual(@as(usize, 0), _embeddings_len);");
                        }
                        _ => {}
                    }
                    let _ = writeln!(out, "    }}");
                    return;
                }
                _ => {}
            }
        }
    }

    // When result_is_simple, the Zig binding returns a scalar type like []u8 or ?T.
    // Skip assertions on fields that don't exist on the scalar (e.g., metadata,
    // document, structure fields).
    if result_is_simple {
        if let Some(f) = &assertion.field {
            let f_lower = f.to_lowercase();
            if !f.is_empty()
                && f_lower != "content"
                && (f_lower.starts_with("metadata")
                    || f_lower.starts_with("document")
                    || f_lower.starts_with("structure"))
            {
                let _ = writeln!(out, "    // skipped: field '{}' not available when result_is_simple", f);
                return;
            }
        }
    }

    // Synthetic-field 'result' on a bare-string/JSON-bytes return (e.g.
    // `detect_mime_type_from_bytes` returns `String` → Zig `[]u8`). The
    // fixture convention is `field: "result", contains: "pdf"` meaning the
    // bare result itself contains the substring. The Zig binding returns
    // `[]u8`, so the substring check applies directly to `result_var`.
    if let Some(f) = &assertion.field {
        if f == "result" && !field_resolver.has_explicit_field(f) {
            match assertion.assertion_type.as_str() {
                "contains" => {
                    if let Some(expected) = &assertion.value {
                        let zig_val = json_to_zig(expected);
                        let _ = writeln!(
                            out,
                            "    try testing.expect(std.mem.indexOf(u8, {result_var}, {zig_val}) != null);"
                        );
                        return;
                    }
                }
                "not_contains" => {
                    if let Some(expected) = &assertion.value {
                        let zig_val = json_to_zig(expected);
                        let _ = writeln!(
                            out,
                            "    try testing.expect(std.mem.indexOf(u8, {result_var}, {zig_val}) == null);"
                        );
                        return;
                    }
                }
                "equals" => {
                    if let Some(expected) = &assertion.value {
                        let zig_val = json_to_zig(expected);
                        let _ = writeln!(out, "    try testing.expectEqualStrings({zig_val}, {result_var});");
                        return;
                    }
                }
                "not_empty" => {
                    let _ = writeln!(out, "    try testing.expect({result_var}.len > 0);");
                    return;
                }
                "is_empty" => {
                    let _ = writeln!(out, "    try testing.expectEqual(@as(usize, 0), {result_var}.len);");
                    return;
                }
                _ => {}
            }
        }
    }

    // Skip assertions on fields that don't exist on the result type.
    if let Some(f) = &assertion.field {
        if !f.is_empty() && !field_resolver.is_valid_for_result(f) {
            let _ = writeln!(out, "    // skipped: field '{{f}}' not available on result type");
            return;
        }
    }

    // Determine if this field is an enum type.
    let _field_is_enum = assertion
        .field
        .as_deref()
        .is_some_and(|f| enum_fields.contains(f) || enum_fields.contains(field_resolver.resolve(f)));

    let field_expr = match &assertion.field {
        // When result_is_simple, the result is a scalar ([]u8 or ?T, etc.) — any
        // field access on it would fail. Treat all assertions as referring to the
        // result itself.
        _ if result_is_simple => result_var.to_string(),
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "zig", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let zig_val = json_to_zig(expected);
                let _ = writeln!(out, "    try testing.expectEqual({zig_val}, {field_expr});");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let zig_val = json_to_zig(expected);
                let _ = writeln!(
                    out,
                    "    try testing.expect(std.mem.indexOf(u8, {field_expr}, {zig_val}) != null);"
                );
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let zig_val = json_to_zig(val);
                    let _ = writeln!(
                        out,
                        "    try testing.expect(std.mem.indexOf(u8, {field_expr}, {zig_val}) != null);"
                    );
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let zig_val = json_to_zig(expected);
                let _ = writeln!(
                    out,
                    "    try testing.expect(std.mem.indexOf(u8, {field_expr}, {zig_val}) == null);"
                );
            } else if let Some(values) = &assertion.values {
                // not_contains with a plural `values` list: assert none of the entries
                // appear in the field. Emit one expect line per needle so failures
                // pinpoint the offending value.
                for val in values {
                    let zig_val = json_to_zig(val);
                    let _ = writeln!(
                        out,
                        "    try testing.expect(std.mem.indexOf(u8, {field_expr}, {zig_val}) == null);"
                    );
                }
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "    try testing.expect({field_expr}.len > 0);");
        }
        "is_empty" => {
            let _ = writeln!(out, "    try testing.expect({field_expr}.len == 0);");
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let zig_val = json_to_zig(expected);
                let _ = writeln!(
                    out,
                    "    try testing.expect(std.mem.startsWith(u8, {field_expr}, {zig_val}));"
                );
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let zig_val = json_to_zig(expected);
                let _ = writeln!(
                    out,
                    "    try testing.expect(std.mem.endsWith(u8, {field_expr}, {zig_val}));"
                );
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    try testing.expect({field_expr}.len >= {n});");
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    try testing.expect({field_expr}.len <= {n});");
                }
            }
        }
        "count_min" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    try testing.expect({field_expr}.len >= {n});");
                }
            }
        }
        "count_equals" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    // When there is no field (field_expr == result_var), the result
                    // is `[]u8` JSON (e.g. batch functions). Parse the JSON array
                    // and count its elements; `.len` would give byte count, not item count.
                    let has_field = assertion.field.as_deref().is_some_and(|f| !f.is_empty());
                    if has_field {
                        let _ = writeln!(out, "    try testing.expectEqual(@as(usize, {n}), {field_expr}.len);");
                    } else {
                        let _ = writeln!(out, "    {{");
                        let _ = writeln!(
                            out,
                            "        var _cparse = try std.json.parseFromSlice(std.json.Value, std.heap.c_allocator, {field_expr}, .{{}});"
                        );
                        let _ = writeln!(out, "        defer _cparse.deinit();");
                        let _ = writeln!(
                            out,
                            "        try testing.expectEqual(@as(usize, {n}), _cparse.value.array.items.len);"
                        );
                        let _ = writeln!(out, "    }}");
                    }
                }
            }
        }
        "is_true" => {
            let _ = writeln!(out, "    try testing.expect({field_expr});");
        }
        "is_false" => {
            let _ = writeln!(out, "    try testing.expect(!{field_expr});");
        }
        "not_error" => {
            // Already handled by the call succeeding.
        }
        "error" => {
            // Handled at the test function level.
        }
        "greater_than" => {
            if let Some(val) = &assertion.value {
                // Skip comparisons like `len > -1` when the value is negative: they are always-true
                // tautologies for unsigned types and create invalid Zig code (@as(usize, -1)).
                let is_negative = matches!(val, serde_json::Value::Number(n) if n.as_i64().is_some_and(|i| i < 0));
                if !is_negative {
                    let zig_val = json_to_zig(val);
                    let _ = writeln!(out, "    try testing.expect({field_expr} > {zig_val});");
                }
            }
        }
        "less_than" => {
            if let Some(val) = &assertion.value {
                let zig_val = json_to_zig(val);
                let _ = writeln!(out, "    try testing.expect({field_expr} < {zig_val});");
            }
        }
        "greater_than_or_equal" => {
            if let Some(val) = &assertion.value {
                // Skip comparisons like `len >= -1` when the value is negative: they are always-true
                // tautologies for unsigned types and create invalid Zig code (@as(usize, -1)).
                let is_negative = matches!(val, serde_json::Value::Number(n) if n.as_i64().is_some_and(|i| i < 0));
                if !is_negative {
                    let zig_val = json_to_zig(val);
                    let _ = writeln!(out, "    try testing.expect({field_expr} >= {zig_val});");
                }
            }
        }
        "less_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let zig_val = json_to_zig(val);
                let _ = writeln!(out, "    try testing.expect({field_expr} <= {zig_val});");
            }
        }
        "contains_any" => {
            // At least ONE of the values must be found in the field (OR logic).
            if let Some(values) = &assertion.values {
                let string_values: Vec<String> = values
                    .iter()
                    .filter_map(|v| {
                        if let serde_json::Value::String(s) = v {
                            Some(format!(
                                "std.mem.indexOf(u8, {field_expr}, \"{}\") != null",
                                escape_zig(s)
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !string_values.is_empty() {
                    let condition = string_values.join(" or\n        ");
                    let _ = writeln!(out, "    try testing.expect(\n        {condition}\n    );");
                }
            }
        }
        "matches_regex" => {
            let _ = writeln!(out, "    // regex match not yet implemented for Zig");
        }
        "method_result" => {
            let _ = writeln!(out, "    // method_result assertions not yet implemented for Zig");
        }
        other => {
            panic!("Zig e2e generator: unsupported assertion type: {other}");
        }
    }
}

/// Convert a `serde_json::Value` to a Zig literal string.
pub(super) fn json_to_zig(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_zig(s)),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_zig).collect();
            format!("&.{{{}}}", items.join(", "))
        }
        serde_json::Value::Object(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            format!("\"{}\"", escape_zig(&json_str))
        }
    }
}
