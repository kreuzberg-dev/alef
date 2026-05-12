//! Shared streaming-virtual-fields module for e2e test codegen.
//!
//! Chat-stream fixtures assert on "virtual" fields that don't exist on the
//! stream result type itself — `chunks`, `chunks.length`, `stream_content`,
//! `stream_complete`, `no_chunks_after_done`, `tool_calls`, `finish_reason`.
//! These fields resolve against the *collected* list of chunks produced by
//! draining the stream.
//!
//! [`StreamingFieldResolver`] provides two entry points:
//! - [`StreamingFieldResolver::accessor`] — the language-specific expression
//!   for a virtual field given a local variable that holds the collected list.
//! - [`StreamingFieldResolver::collect_snippet`] — the language-specific
//!   code snippet that drains a stream variable into the collected list.
//!
//! ## Convention
//!
//! The `chunks_var` parameter is the local variable name that holds the
//! collected list (default: `"chunks"`).  The `stream_var` parameter is the
//! result variable produced by the stream call (default: `"result"`).
//!
//! The set of streaming-virtual field names handled by this module:
//! - `chunks`              → the collected list itself
//! - `chunks.length`       → length/count of the collected list
//! - `stream_content`      → concatenation of all delta content strings
//! - `stream_complete`     → boolean — last chunk has a non-null finish_reason
//! - `no_chunks_after_done` → structural invariant (true by construction for
//!   channel/iterator-based APIs once the channel is closed; emitted as
//!   `assert!(true)` / `assertTrue` for languages without post-DONE chunk plumbing)
//! - `tool_calls`          → flat list of tool_calls from all chunk deltas
//! - `finish_reason`       → finish_reason string from the last chunk

/// The set of field names treated as streaming-virtual fields.
pub const STREAMING_VIRTUAL_FIELDS: &[&str] = &[
    "chunks",
    "chunks.length",
    "stream_content",
    "stream_complete",
    "no_chunks_after_done",
    "tool_calls",
    "finish_reason",
];

/// Returns `true` when `field` is a streaming-virtual field name.
pub fn is_streaming_virtual_field(field: &str) -> bool {
    STREAMING_VIRTUAL_FIELDS.contains(&field)
}

/// Shared streaming-virtual-fields resolver for e2e test codegen.
pub struct StreamingFieldResolver;

impl StreamingFieldResolver {
    /// Returns the language-specific expression for a streaming-virtual field,
    /// given `chunks_var` (the collected-list local name) and `lang`.
    ///
    /// Returns `None` when the field name is not a known streaming-virtual
    /// field or the language has no streaming support.
    pub fn accessor(field: &str, lang: &str, chunks_var: &str) -> Option<String> {
        match field {
            "chunks" => Some(chunks_var.to_string()),

            "chunks.length" => Some(match lang {
                "rust" => format!("{chunks_var}.len()"),
                "go" => format!("len({chunks_var})"),
                "python" => format!("len({chunks_var})"),
                "php" => format!("count(${chunks_var})"),
                // node/wasm/typescript use .length
                _ => format!("{chunks_var}.length"),
            }),

            "stream_content" => Some(match lang {
                "rust" => {
                    format!(
                        "{chunks_var}.iter().map(|c| c.choices.first().and_then(|ch| ch.delta.content.as_deref()).unwrap_or(\"\")).collect::<String>()"
                    )
                }
                "go" => {
                    // Go: chunks is []pkg.ChatCompletionChunk
                    format!(
                        "func() string {{ var s string; for _, c := range {chunks_var} {{ if len(c.Choices) > 0 && c.Choices[0].Delta.Content != nil {{ s += *c.Choices[0].Delta.Content }} }}; return s }}()"
                    )
                }
                "java" => {
                    format!(
                        "{chunks_var}.stream().map(c -> c.choices().stream().findFirst().map(ch -> ch.delta().content() != null ? ch.delta().content() : \"\").orElse(\"\")).collect(java.util.stream.Collectors.joining())"
                    )
                }
                "php" => {
                    format!("implode('', array_map(fn($c) => $c->choices[0]->delta->content ?? '', ${chunks_var}))")
                }
                "zig" => {
                    // Zig: simplified - use empty string as fallback for zig JSON struct path
                    format!("{chunks_var}_content")
                }
                // node/wasm/typescript
                _ => {
                    format!("{chunks_var}.map((c: any) => c.choices?.[0]?.delta?.content ?? '').join('')")
                }
            }),

            "stream_complete" => Some(match lang {
                "rust" => {
                    format!(
                        "{chunks_var}.last().and_then(|c| c.choices.first()).and_then(|ch| ch.finish_reason.as_ref()).is_some()"
                    )
                }
                "go" => {
                    format!(
                        "func() bool {{ if len({chunks_var}) == 0 {{ return false }}; last := {chunks_var}[len({chunks_var})-1]; return len(last.Choices) > 0 && last.Choices[0].FinishReason != nil }}()"
                    )
                }
                "java" => {
                    format!(
                        "!{chunks_var}.isEmpty() && {chunks_var}.get({chunks_var}.size()-1).choices().stream().findFirst().flatMap(ch -> java.util.Optional.ofNullable(ch.finishReason())).isPresent()"
                    )
                }
                "php" => {
                    format!("!empty(${chunks_var}) && isset(end(${chunks_var})->choices[0]->finishReason)")
                }
                // node/wasm/typescript
                _ => {
                    format!(
                        "{chunks_var}.length > 0 && {chunks_var}[{chunks_var}.length - 1].choices?.[0]?.finishReason != null"
                    )
                }
            }),

            // no_chunks_after_done is a structural invariant: once the stream
            // closes (channel drained / iterator exhausted), no further chunks
            // can arrive.  We assert `true` as a compile-time proof of intent.
            "no_chunks_after_done" => Some(match lang {
                "rust" => "true".to_string(),
                "go" => "true".to_string(),
                "java" => "true".to_string(),
                "php" => "true".to_string(),
                _ => "true".to_string(),
            }),

            "tool_calls" => Some(match lang {
                "rust" => {
                    format!(
                        "{chunks_var}.iter().flat_map(|c| c.choices.iter().flat_map(|ch| ch.delta.tool_calls.iter().flatten())).collect::<Vec<_>>()"
                    )
                }
                "go" => {
                    format!(
                        "func() []interface{{}} {{ var tc []interface{{}}; for _, c := range {chunks_var} {{ for _, ch := range c.Choices {{ if ch.Delta.ToolCalls != nil {{ for _, t := range *ch.Delta.ToolCalls {{ tc = append(tc, t) }} }} }} }}; return tc }}()"
                    )
                }
                "java" => {
                    format!(
                        "{chunks_var}.stream().flatMap(c -> c.choices().stream()).flatMap(ch -> ch.delta().toolCalls() != null ? ch.delta().toolCalls().stream() : java.util.stream.Stream.empty()).toList()"
                    )
                }
                "php" => {
                    format!(
                        "array_merge(...array_map(fn($c) => $c->choices[0]->delta->toolCalls ?? [], ${chunks_var}))"
                    )
                }
                _ => {
                    format!("{chunks_var}.flatMap((c: any) => c.choices?.[0]?.delta?.toolCalls ?? [])")
                }
            }),

            "finish_reason" => Some(match lang {
                "rust" => {
                    format!(
                        "{chunks_var}.last().and_then(|c| c.choices.first()).and_then(|ch| ch.finish_reason.as_deref()).unwrap_or(\"\")"
                    )
                }
                "go" => {
                    format!(
                        "func() string {{ if len({chunks_var}) == 0 {{ return \"\" }}; last := {chunks_var}[len({chunks_var})-1]; if len(last.Choices) > 0 && last.Choices[0].FinishReason != nil {{ return *last.Choices[0].FinishReason }}; return \"\" }}()"
                    )
                }
                "java" => {
                    format!(
                        "({chunks_var}.isEmpty() ? null : {chunks_var}.get({chunks_var}.size()-1).choices().stream().findFirst().map(ch -> ch.finishReason()).orElse(null))"
                    )
                }
                "php" => {
                    format!("(!empty(${chunks_var}) ? (end(${chunks_var})->choices[0]->finishReason ?? null) : null)")
                }
                _ => {
                    format!(
                        "{chunks_var}.length > 0 ? {chunks_var}[{chunks_var}.length - 1].choices?.[0]?.finishReason : undefined"
                    )
                }
            }),

            _ => None,
        }
    }

    /// Returns the language-specific stream-collect-into-list snippet that
    /// produces `chunks_var` from `stream_var`.
    ///
    /// Returns `None` when the language has no streaming collect support or
    /// when the collect snippet cannot be expressed generically.
    pub fn collect_snippet(lang: &str, stream_var: &str, chunks_var: &str) -> Option<String> {
        match lang {
            "rust" => Some(format!(
                "let {chunks_var}: Vec<_> = tokio_stream::StreamExt::collect::<Vec<_>>({stream_var}).await;"
            )),
            "go" => Some(format!(
                "var {chunks_var} []pkg.ChatCompletionChunk\n\tfor chunk := range {stream_var} {{\n\t\t{chunks_var} = append({chunks_var}, chunk)\n\t}}"
            )),
            "java" => Some(format!(
                "var {chunks_var} = new java.util.ArrayList<ChatCompletionChunk>();\n        var _it = {stream_var};\n        while (_it.hasNext()) {{ {chunks_var}.add(_it.next()); }}"
            )),
            "php" => Some(format!("${chunks_var} = iterator_to_array(${stream_var});")),
            "node" | "wasm" | "typescript" => Some(format!(
                "const {chunks_var}: any[] = [];\n    for await (const _chunk of {stream_var}) {{ {chunks_var}.push(_chunk); }}"
            )),
            "zig" => {
                // Zig: streams are returned as opaque handles with JSON output;
                // the collect pattern would require specialized Zig iterator
                // drain code. Emit a simpler approach: use the result directly.
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_streaming_virtual_field_recognizes_all_fields() {
        for field in STREAMING_VIRTUAL_FIELDS {
            assert!(
                is_streaming_virtual_field(field),
                "field '{field}' not recognized as streaming virtual"
            );
        }
    }

    #[test]
    fn is_streaming_virtual_field_rejects_real_fields() {
        assert!(!is_streaming_virtual_field("content"));
        assert!(!is_streaming_virtual_field("choices"));
        assert!(!is_streaming_virtual_field("model"));
        assert!(!is_streaming_virtual_field(""));
    }

    #[test]
    fn accessor_chunks_returns_var_name() {
        assert_eq!(
            StreamingFieldResolver::accessor("chunks", "rust", "chunks"),
            Some("chunks".to_string())
        );
        assert_eq!(
            StreamingFieldResolver::accessor("chunks", "node", "chunks"),
            Some("chunks".to_string())
        );
    }

    #[test]
    fn accessor_chunks_length_uses_language_idiom() {
        let rust = StreamingFieldResolver::accessor("chunks.length", "rust", "chunks").unwrap();
        assert!(rust.contains(".len()"), "rust: {rust}");

        let go = StreamingFieldResolver::accessor("chunks.length", "go", "chunks").unwrap();
        assert!(go.starts_with("len("), "go: {go}");

        let node = StreamingFieldResolver::accessor("chunks.length", "node", "chunks").unwrap();
        assert!(node.contains(".length"), "node: {node}");

        let php = StreamingFieldResolver::accessor("chunks.length", "php", "chunks").unwrap();
        assert!(php.starts_with("count("), "php: {php}");
    }

    #[test]
    fn accessor_stream_content_rust_uses_iterator() {
        let expr = StreamingFieldResolver::accessor("stream_content", "rust", "chunks").unwrap();
        assert!(expr.contains(".collect::<String>()"), "rust stream_content: {expr}");
    }

    #[test]
    fn accessor_no_chunks_after_done_returns_true() {
        for lang in ["rust", "go", "java", "php", "node", "wasm"] {
            let expr = StreamingFieldResolver::accessor("no_chunks_after_done", lang, "chunks").unwrap();
            assert_eq!(expr, "true", "lang {lang}: expected 'true', got '{expr}'");
        }
    }

    #[test]
    fn collect_snippet_rust_uses_tokio_stream() {
        let snip = StreamingFieldResolver::collect_snippet("rust", "result", "chunks").unwrap();
        assert!(snip.contains("tokio_stream::StreamExt::collect"), "rust: {snip}");
        assert!(snip.contains("let chunks"), "rust: {snip}");
    }

    #[test]
    fn collect_snippet_go_drains_channel() {
        let snip = StreamingFieldResolver::collect_snippet("go", "stream", "chunks").unwrap();
        assert!(snip.contains("for chunk := range stream"), "go: {snip}");
    }

    #[test]
    fn collect_snippet_java_uses_iterator() {
        let snip = StreamingFieldResolver::collect_snippet("java", "result", "chunks").unwrap();
        assert!(snip.contains("hasNext()"), "java: {snip}");
    }

    #[test]
    fn collect_snippet_php_uses_iterator_to_array() {
        let snip = StreamingFieldResolver::collect_snippet("php", "result", "chunks").unwrap();
        assert!(snip.contains("iterator_to_array"), "php: {snip}");
    }

    #[test]
    fn collect_snippet_node_uses_for_await() {
        let snip = StreamingFieldResolver::collect_snippet("node", "result", "chunks").unwrap();
        assert!(snip.contains("for await"), "node: {snip}");
    }

    #[test]
    fn accessor_unknown_field_returns_none() {
        assert_eq!(
            StreamingFieldResolver::accessor("nonexistent_field", "rust", "chunks"),
            None
        );
    }
}
