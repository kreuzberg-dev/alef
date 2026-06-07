use super::*;
use std::collections::{HashMap, HashSet};

fn make_resolver() -> FieldResolver {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), "metadata.document.title".to_string());
    fields.insert("tags".to_string(), "metadata.tags[name]".to_string());
    fields.insert("og".to_string(), "metadata.document.open_graph".to_string());
    fields.insert("twitter".to_string(), "metadata.document.twitter_card".to_string());
    fields.insert("canonical".to_string(), "metadata.document.canonical_url".to_string());
    fields.insert("og_tag".to_string(), "metadata.open_graph_tags[og_title]".to_string());
    let mut optional = HashSet::new();
    optional.insert("metadata.document.title".to_string());
    FieldResolver::new(&fields, &optional, &HashSet::new(), &HashSet::new(), &HashSet::new())
}

fn make_resolver_with_doc_optional() -> FieldResolver {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), "metadata.document.title".to_string());
    fields.insert("tags".to_string(), "metadata.tags[name]".to_string());
    let mut optional = HashSet::new();
    optional.insert("document".to_string());
    optional.insert("metadata.document.title".to_string());
    optional.insert("metadata.document".to_string());
    FieldResolver::new(&fields, &optional, &HashSet::new(), &HashSet::new(), &HashSet::new())
}

#[test]
fn test_resolve_alias() {
    let r = make_resolver();
    assert_eq!(r.resolve("title"), "metadata.document.title");
}

#[test]
fn test_resolve_passthrough() {
    let r = make_resolver();
    assert_eq!(r.resolve("content"), "content");
}

#[test]
fn test_is_optional() {
    let r = make_resolver();
    assert!(r.is_optional("metadata.document.title"));
    assert!(!r.is_optional("content"));
}

#[test]
fn is_optional_strips_namespace_prefix() {
    let fields = HashMap::new();
    let mut optional = HashSet::new();
    optional.insert("action_results.data".to_string());
    let result_fields: HashSet<String> = ["action_results".to_string()].into_iter().collect();
    let r = FieldResolver::new(&fields, &optional, &result_fields, &HashSet::new(), &HashSet::new());
    // `interaction.` is a virtual namespace prefix — strip and re-check.
    assert!(r.is_optional("interaction.action_results[0].data"));
    // Still finds it without the prefix.
    assert!(r.is_optional("action_results[0].data"));
}

#[test]
fn test_accessor_rust_struct() {
    let r = make_resolver();
    assert_eq!(r.accessor("title", "rust", "result"), "result.metadata.document.title");
}

#[test]
fn test_accessor_rust_map() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("tags", "rust", "result"),
        "result.metadata.tags.get(\"name\").map(|s| s.as_str())"
    );
}

#[test]
fn test_accessor_python() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "python", "result"),
        "result.metadata.document.title"
    );
}

#[test]
fn test_accessor_go() {
    let r = make_resolver();
    assert_eq!(r.accessor("title", "go", "result"), "result.Metadata.Document.Title");
}

#[test]
fn test_accessor_go_initialism_fields() {
    let mut fields = std::collections::HashMap::new();
    fields.insert("content".to_string(), "html".to_string());
    fields.insert("link_url".to_string(), "links.url".to_string());
    let r = FieldResolver::new(
        &fields,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
    );
    assert_eq!(r.accessor("content", "go", "result"), "result.HTML");
    assert_eq!(r.accessor("link_url", "go", "result"), "result.Links.URL");
    assert_eq!(r.accessor("html", "go", "result"), "result.HTML");
    assert_eq!(r.accessor("url", "go", "result"), "result.URL");
    assert_eq!(r.accessor("id", "go", "result"), "result.ID");
    assert_eq!(r.accessor("user_id", "go", "result"), "result.UserID");
    assert_eq!(r.accessor("request_url", "go", "result"), "result.RequestURL");
    assert_eq!(r.accessor("links", "go", "result"), "result.Links");
}

#[test]
fn test_accessor_typescript() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "typescript", "result"),
        "result.metadata.document.title"
    );
}

#[test]
fn test_accessor_typescript_snake_to_camel() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("og", "typescript", "result"),
        "result.metadata.document.openGraph"
    );
    assert_eq!(
        r.accessor("twitter", "typescript", "result"),
        "result.metadata.document.twitterCard"
    );
    assert_eq!(
        r.accessor("canonical", "typescript", "result"),
        "result.metadata.document.canonicalUrl"
    );
}

#[test]
fn test_accessor_typescript_map_snake_to_camel() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("og_tag", "typescript", "result"),
        "result.metadata.openGraphTags[\"og_title\"]"
    );
}

#[test]
fn test_accessor_typescript_numeric_index_is_unquoted() {
    // Digit-only map-access keys (e.g. JSON pointer segments like `results.0`)
    // must emit numeric bracket access (`[0]`) not string-keyed access
    // (`["0"]`), which would return undefined on arrays.
    let mut fields = HashMap::new();
    fields.insert("first_score".to_string(), "results[0].relevance_score".to_string());
    let r = FieldResolver::new(
        &fields,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
    );
    assert_eq!(
        r.accessor("first_score", "typescript", "result"),
        "result.results[0].relevanceScore"
    );
}

#[test]
fn test_accessor_node_alias() {
    let r = make_resolver();
    assert_eq!(r.accessor("og", "node", "result"), "result.metadata.document.openGraph");
}

#[test]
fn test_accessor_wasm_camel_case() {
    let r = make_resolver();
    assert_eq!(r.accessor("og", "wasm", "result"), "result.metadata.document.openGraph");
    assert_eq!(
        r.accessor("twitter", "wasm", "result"),
        "result.metadata.document.twitterCard"
    );
    assert_eq!(
        r.accessor("canonical", "wasm", "result"),
        "result.metadata.document.canonicalUrl"
    );
}

#[test]
fn test_accessor_wasm_map_access() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("og_tag", "wasm", "result"),
        "result.metadata.openGraphTags.get(\"og_title\")"
    );
}

#[test]
fn test_accessor_java() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "java", "result"),
        "result.metadata().document().title()"
    );
}

#[test]
fn test_accessor_kotlin_uses_kotlin_collection_idioms() {
    let mut fields = HashMap::new();
    fields.insert("first_node_name".to_string(), "nodes[0].name".to_string());
    fields.insert("node_count".to_string(), "nodes.length".to_string());
    let mut arrays = HashSet::new();
    arrays.insert("nodes".to_string());
    let r = FieldResolver::new(&fields, &HashSet::new(), &HashSet::new(), &arrays, &HashSet::new());
    assert_eq!(
        r.accessor("first_node_name", "kotlin", "result"),
        "result.nodes().first().name()"
    );
    assert_eq!(r.accessor("node_count", "kotlin", "result"), "result.nodes().size");
}

#[test]
fn test_accessor_kotlin_uses_safe_calls_for_optional_prefixes() {
    let r = make_resolver_with_doc_optional();
    assert_eq!(
        r.accessor("title", "kotlin", "result"),
        "result.metadata().document()?.title()"
    );
}

#[test]
fn test_accessor_kotlin_uses_safe_calls_for_optional_arrays_and_maps() {
    let mut fields = HashMap::new();
    fields.insert("first_node_name".to_string(), "nodes[0].name".to_string());
    fields.insert("tag".to_string(), "tags[name]".to_string());
    let mut optional = HashSet::new();
    optional.insert("nodes".to_string());
    optional.insert("tags".to_string());
    let mut arrays = HashSet::new();
    arrays.insert("nodes".to_string());
    let r = FieldResolver::new(&fields, &optional, &HashSet::new(), &arrays, &HashSet::new());
    assert_eq!(
        r.accessor("first_node_name", "kotlin", "result"),
        "result.nodes()?.first()?.name()"
    );
    assert_eq!(r.accessor("tag", "kotlin", "result"), "result.tags()?.get(\"name\")");
}

/// Regression: optional-field keys with explicit `[0]` indices (e.g.
/// `"choices[0].message.tool_calls"`) were not matched by
/// `render_kotlin_with_optionals` because `path_so_far` omitted the `[0]`
/// suffix after traversing an ArrayField segment. Fix: append `"[0]"` to
/// `path_so_far` after each ArrayField, mirroring the Rust renderer.
#[test]
fn test_accessor_kotlin_optional_field_after_indexed_array() {
    // "choices[0].message.tool_calls" is optional; the path is accessed as
    // choices[0].message.tool_calls[0].function.name.
    let mut fields = HashMap::new();
    fields.insert(
        "tool_call_name".to_string(),
        "choices[0].message.tool_calls[0].function.name".to_string(),
    );
    let mut optional = HashSet::new();
    optional.insert("choices[0].message.tool_calls".to_string());
    let mut arrays = HashSet::new();
    arrays.insert("choices".to_string());
    arrays.insert("choices[0].message.tool_calls".to_string());
    let r = FieldResolver::new(&fields, &optional, &HashSet::new(), &arrays, &HashSet::new());
    let expr = r.accessor("tool_call_name", "kotlin", "result");
    // toolCalls() is optional so it must use `?.` before `.first()`.
    assert!(
        expr.contains("toolCalls()?.first()"),
        "expected toolCalls()?.first() for optional list, got: {expr}"
    );
}

#[test]
fn test_accessor_csharp() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "csharp", "result"),
        "result.Metadata.Document.Title"
    );
}

#[test]
fn test_accessor_php() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "php", "$result"),
        "$result->metadata->document->title"
    );
}

#[test]
fn test_accessor_r() {
    let r = make_resolver();
    assert_eq!(r.accessor("title", "r", "result"), "result$metadata$document$title");
}

#[test]
fn test_accessor_c() {
    let r = make_resolver();
    assert_eq!(
        r.accessor("title", "c", "result"),
        "result_title(result_document(result_metadata(result)))"
    );
}

#[test]
fn test_rust_unwrap_binding() {
    let r = make_resolver();
    let (binding, var) = r.rust_unwrap_binding("title", "result").unwrap();
    // Binding is prefixed with `_` to suppress `-D unused_variables` when no
    // assertion references it; the variable remains accessible under that name.
    assert_eq!(var, "_metadata_document_title");
    assert!(binding.starts_with("let _metadata_document_title ="));
    // Optional scalar fields are unwrapped via Display (`to_string()`) so enum
    // types like `FinishReason` render their serde-style string form.
    assert!(binding.contains("as_ref().map(|v| v.to_string()).unwrap_or_default()"));
}

#[test]
fn test_rust_unwrap_binding_non_optional() {
    let r = make_resolver();
    assert!(r.rust_unwrap_binding("content", "result").is_none());
}

#[test]
fn test_rust_unwrap_binding_collapses_double_underscore() {
    // When an alias resolves to a path with `[]` (e.g. `json_ld.name` →
    // `json_ld[].name`), the naive replace previously yielded `json_ld__name`,
    // which trips Rust's non_snake_case lint under -D warnings. The local
    // binding name must collapse consecutive underscores into one.
    let mut aliases = HashMap::new();
    aliases.insert("json_ld.name".to_string(), "json_ld[].name".to_string());
    let mut optional = HashSet::new();
    optional.insert("json_ld[].name".to_string());
    let mut array = HashSet::new();
    array.insert("json_ld".to_string());
    let result_fields = HashSet::new();
    let method_calls = HashSet::new();
    let r = FieldResolver::new(&aliases, &optional, &result_fields, &array, &method_calls);
    let (_binding, var) = r.rust_unwrap_binding("json_ld.name", "result").unwrap();
    assert_eq!(var, "_json_ld_name");
}

#[test]
fn test_direct_field_no_alias() {
    let r = make_resolver();
    assert_eq!(r.accessor("content", "rust", "result"), "result.content");
    assert_eq!(r.accessor("content", "go", "result"), "result.Content");
}

#[test]
fn test_accessor_rust_with_optionals() {
    let r = make_resolver_with_doc_optional();
    assert_eq!(
        r.accessor("title", "rust", "result"),
        "result.metadata.document.as_ref().unwrap().title"
    );
}

#[test]
fn test_accessor_csharp_with_optionals() {
    let r = make_resolver_with_doc_optional();
    assert_eq!(
        r.accessor("title", "csharp", "result"),
        "result.Metadata.Document!.Title"
    );
}

#[test]
fn test_accessor_rust_non_optional_field() {
    let r = make_resolver();
    assert_eq!(r.accessor("content", "rust", "result"), "result.content");
}

#[test]
fn test_accessor_csharp_non_optional_field() {
    let r = make_resolver();
    assert_eq!(r.accessor("content", "csharp", "result"), "result.Content");
}

#[test]
fn test_accessor_rust_method_call() {
    // "metadata.format.excel" is in method_calls — should emit `excel()` instead of `excel`
    let mut fields = HashMap::new();
    fields.insert(
        "excel_sheet_count".to_string(),
        "metadata.format.excel.sheet_count".to_string(),
    );
    let mut optional = HashSet::new();
    optional.insert("metadata.format".to_string());
    optional.insert("metadata.format.excel".to_string());
    let mut method_calls = HashSet::new();
    method_calls.insert("metadata.format.excel".to_string());
    let r = FieldResolver::new(&fields, &optional, &HashSet::new(), &HashSet::new(), &method_calls);
    assert_eq!(
        r.accessor("excel_sheet_count", "rust", "result"),
        "result.metadata.format.as_ref().unwrap().excel().as_ref().unwrap().sheet_count"
    );
}

// ---------------------------------------------------------------------------
// PHP getter-method tests (ext-php-rs 0.15.x `#[php(getter)]` vs `#[php(prop)]`)
// ---------------------------------------------------------------------------

fn make_php_getter_resolver() -> FieldResolver {
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    getters.insert(
        "Root".to_string(),
        ["metadata".to_string(), "links".to_string()].into_iter().collect(),
    );
    let map = PhpGetterMap {
        getters,
        field_types: HashMap::new(),
        root_type: Some("Root".to_string()),
        all_fields: HashMap::new(),
    };
    FieldResolver::new_with_php_getters(
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map,
    )
}

#[test]
fn render_php_uses_getter_method_for_non_scalar_field() {
    let r = make_php_getter_resolver();
    assert_eq!(r.accessor("metadata", "php", "$result"), "$result->getMetadata()");
}

#[test]
fn render_php_uses_property_for_scalar_field() {
    let r = make_php_getter_resolver();
    assert_eq!(r.accessor("status_code", "php", "$result"), "$result->statusCode");
}

#[test]
fn render_php_nested_non_scalar_uses_getter_then_property() {
    let mut fields = HashMap::new();
    fields.insert("title".to_string(), "metadata.title".to_string());
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    getters.insert("Root".to_string(), ["metadata".to_string()].into_iter().collect());
    // No entry for Metadata.title → scalar by default.
    getters.insert("Metadata".to_string(), HashSet::new());
    let mut field_types: HashMap<String, HashMap<String, String>> = HashMap::new();
    field_types.insert(
        "Root".to_string(),
        [("metadata".to_string(), "Metadata".to_string())].into_iter().collect(),
    );
    let map = PhpGetterMap {
        getters,
        field_types,
        root_type: Some("Root".to_string()),
        all_fields: HashMap::new(),
    };
    let r = FieldResolver::new_with_php_getters(
        &fields,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map,
    );
    // `metadata` → `->getMetadata()`, then `title` (scalar on returned object) → `->title`
    assert_eq!(r.accessor("title", "php", "$result"), "$result->getMetadata()->title");
}

#[test]
fn render_php_array_field_uses_getter_when_non_scalar() {
    let mut fields = HashMap::new();
    fields.insert("first_link".to_string(), "links[0]".to_string());
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    getters.insert("Root".to_string(), ["links".to_string()].into_iter().collect());
    let map = PhpGetterMap {
        getters,
        field_types: HashMap::new(),
        root_type: Some("Root".to_string()),
        all_fields: HashMap::new(),
    };
    let r = FieldResolver::new_with_php_getters(
        &fields,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map,
    );
    assert_eq!(r.accessor("first_link", "php", "$result"), "$result->getLinks()[0]");
}

#[test]
fn render_php_falls_back_to_property_when_getter_fields_empty() {
    // With empty php_getter_map the resolver uses the plain `render_php` path,
    // which emits `->camelCase` for every field regardless of scalar-ness.
    let r = FieldResolver::new(
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
    );
    assert_eq!(r.accessor("status_code", "php", "$result"), "$result->statusCode");
    assert_eq!(r.accessor("metadata", "php", "$result"), "$result->metadata");
}

// Regression: bare-name HashSet classification produced false getters when two
// types shared a field name with different scalarness (`content`
// collision between CrawlConfig.content: ContentConfig and MarkdownResult.content: String).
#[test]
fn render_php_with_getters_distinguishes_same_field_name_on_different_types() {
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    // A.content is non-scalar.
    getters.insert("A".to_string(), ["content".to_string()].into_iter().collect());
    // B.content is scalar — explicit empty set.
    getters.insert("B".to_string(), HashSet::new());
    // Both A and B declare a "content" field — needed so the per-type
    // classification is consulted (not fallback bare-name union).
    let mut all_fields: HashMap<String, HashSet<String>> = HashMap::new();
    all_fields.insert("A".to_string(), ["content".to_string()].into_iter().collect());
    all_fields.insert("B".to_string(), ["content".to_string()].into_iter().collect());
    let map_a = PhpGetterMap {
        getters: getters.clone(),
        field_types: HashMap::new(),
        root_type: Some("A".to_string()),
        all_fields: all_fields.clone(),
    };
    let map_b = PhpGetterMap {
        getters,
        field_types: HashMap::new(),
        root_type: Some("B".to_string()),
        all_fields,
    };
    let r_a = FieldResolver::new_with_php_getters(
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map_a,
    );
    let r_b = FieldResolver::new_with_php_getters(
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map_b,
    );
    assert_eq!(r_a.accessor("content", "php", "$a"), "$a->getContent()");
    assert_eq!(r_b.accessor("content", "php", "$b"), "$b->content");
}

// Regression: the chain renderer must advance current_type through the IR's
// nested-type graph so a scalar field on a nested type is not falsely
// classified as needing a getter because some other type uses the same name.
#[test]
fn render_php_with_getters_chains_through_correct_type() {
    let mut fields = HashMap::new();
    fields.insert("nested_content".to_string(), "inner.content".to_string());
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    // Outer.inner is non-scalar (struct B).
    getters.insert("Outer".to_string(), ["inner".to_string()].into_iter().collect());
    // B.content is scalar.
    getters.insert("B".to_string(), HashSet::new());
    // Decoy: another type with non-scalar `content` field — used to verify
    // the legacy bare-name union would have produced the wrong answer.
    getters.insert("Decoy".to_string(), ["content".to_string()].into_iter().collect());
    let mut field_types: HashMap<String, HashMap<String, String>> = HashMap::new();
    field_types.insert(
        "Outer".to_string(),
        [("inner".to_string(), "B".to_string())].into_iter().collect(),
    );
    let mut all_fields: HashMap<String, HashSet<String>> = HashMap::new();
    all_fields.insert("Outer".to_string(), ["inner".to_string()].into_iter().collect());
    all_fields.insert("B".to_string(), ["content".to_string()].into_iter().collect());
    all_fields.insert("Decoy".to_string(), ["content".to_string()].into_iter().collect());
    let map = PhpGetterMap {
        getters,
        field_types,
        root_type: Some("Outer".to_string()),
        all_fields,
    };
    let r = FieldResolver::new_with_php_getters(
        &fields,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map,
    );
    assert_eq!(
        r.accessor("nested_content", "php", "$result"),
        "$result->getInner()->content"
    );
}

// ---------------------------------------------------------------------------
// Namespace-prefix stripping tests
// ---------------------------------------------------------------------------

fn make_resolver_with_result_fields(result_fields: &[&str]) -> FieldResolver {
    let rf: HashSet<String> = result_fields.iter().map(|s| s.to_string()).collect();
    FieldResolver::new(&HashMap::new(), &HashSet::new(), &rf, &HashSet::new(), &HashSet::new())
}

/// `browser.browser_used` — `browser` is a virtual namespace prefix, actual
/// field is `browser_used` which IS in result_fields.
#[test]
fn is_valid_for_result_accepts_virtual_namespace_prefix() {
    let r = make_resolver_with_result_fields(&["browser_used", "js_render_hint", "status_code"]);
    assert!(
        r.is_valid_for_result("browser.browser_used"),
        "browser.browser_used should be valid via namespace-prefix stripping"
    );
    assert!(
        r.is_valid_for_result("browser.js_render_hint"),
        "browser.js_render_hint should be valid via namespace-prefix stripping"
    );
}

/// `interaction.action_results[0].action_type` — `interaction` is a virtual
/// namespace prefix, `action_results` IS in result_fields.
#[test]
fn is_valid_for_result_accepts_namespace_prefix_before_array_field() {
    let r = make_resolver_with_result_fields(&["action_results", "final_html", "final_url"]);
    assert!(
        r.is_valid_for_result("interaction.action_results[0].action_type"),
        "interaction. prefix should be stripped so action_results is recognised"
    );
}

/// Fields that genuinely don't exist should still be rejected.
#[test]
fn is_valid_for_result_rejects_unknown_field_even_after_namespace_strip() {
    let r = make_resolver_with_result_fields(&["pages", "final_url"]);
    assert!(
        !r.is_valid_for_result("browser.browser_used"),
        "browser_used is not in result_fields so should be rejected"
    );
    assert!(
        !r.is_valid_for_result("ns.unknown_field"),
        "unknown_field is not in result_fields so should be rejected"
    );
}

/// Accessor for `browser.browser_used` should produce the stripped path
/// (i.e. `result.browser_used` for Python, not `result.browser.browser_used`).
#[test]
fn accessor_strips_namespace_prefix_for_python() {
    let r = make_resolver_with_result_fields(&["browser_used", "js_render_hint"]);
    assert_eq!(
        r.accessor("browser.browser_used", "python", "result"),
        "result.browser_used"
    );
    assert_eq!(
        r.accessor("browser.js_render_hint", "python", "result"),
        "result.js_render_hint"
    );
}

/// Accessor for `browser.browser_used` should produce PascalCase path for C#.
#[test]
fn accessor_strips_namespace_prefix_for_csharp() {
    let r = make_resolver_with_result_fields(&["browser_used"]);
    assert_eq!(
        r.accessor("browser.browser_used", "csharp", "result"),
        "result.BrowserUsed"
    );
}

/// Accessor for `interaction.action_results[0].action_type` — strips `interaction.`
/// prefix and resolves the remaining path.
#[test]
fn accessor_strips_namespace_prefix_for_indexed_array_field() {
    let r = make_resolver_with_result_fields(&["action_results", "final_html", "final_url"]);
    // Python: result.action_results[0].action_type
    assert_eq!(
        r.accessor("interaction.action_results[0].action_type", "python", "result"),
        "result.action_results[0].action_type"
    );
    // TypeScript: result.actionResults[0].actionType
    assert_eq!(
        r.accessor("interaction.action_results[0].action_type", "typescript", "result"),
        "result.actionResults[0].actionType"
    );
}

/// When `result_fields` is empty, namespace stripping is disabled and every
/// path is accepted (the permissive default).
#[test]
fn is_valid_for_result_is_permissive_when_result_fields_empty() {
    let r = make_resolver_with_result_fields(&[]);
    assert!(r.is_valid_for_result("browser.browser_used"));
    assert!(r.is_valid_for_result("anything.at.all"));
}

/// A real two-segment path like `metadata.title` where `metadata` IS a
/// known result field must NOT be stripped — the full path resolves correctly.
#[test]
fn accessor_does_not_strip_real_first_segment() {
    let r = make_resolver_with_result_fields(&["metadata", "status_code"]);
    // `metadata` is a real result field; should not be stripped.
    assert_eq!(
        r.accessor("metadata.title", "python", "result"),
        "result.metadata.title"
    );
}

/// When `result_fields` is empty, `namespace_stripped_path` must return
/// `None` so dotted field paths like `metrics.total_lines` survive intact
/// for backends that navigate the path segment-by-segment (e.g. C). Prior
/// behaviour stripped unconditionally, collapsing every dotted path to its
/// leaf — the C codegen then emitted `<root>_<leaf>` accessors against the
/// wrong parent type (e.g. `sample_pack_process_result_total_lines(result)`
/// instead of `sample_pack_file_metrics_total_lines(metrics)`).
#[test]
fn namespace_stripped_path_returns_none_when_result_fields_empty() {
    let r = make_resolver_with_result_fields(&[]);
    assert_eq!(r.namespace_stripped_path("metrics.total_lines"), None);
    assert_eq!(r.namespace_stripped_path("anything.deeply.nested.path"), None);
}

// ---------------------------------------------------------------------------
// Rust + PHP accessor regression: result_fields and per-type getter lookups
// must override the global "method_calls" / "any-type" leakage.
// ---------------------------------------------------------------------------

/// Rust accessor: when a field is in both `method_calls` (workspace-global
/// from `[crates.e2e.fields_method_calls]`) AND `result_fields` (the
/// fixture's root-type field list), it must render as field access
/// (`result.content`), not a method call (`result.content()`).
///
/// Regression: a fixture DTO's `DocumentResult.content: String` is a struct
/// field, but other types in the workspace declare `content` as a method,
/// so the global `method_calls` set carries it. Without consulting
/// `result_fields`, the Rust e2e renderer emitted `result.content()` and
/// produced E0599 against `pub content: String`.
#[test]
fn render_rust_with_result_fields_overrides_method_calls() {
    let result_fields: HashSet<String> = ["content".to_string(), "mime_type".to_string()].into_iter().collect();
    let method_calls: HashSet<String> = [
        "content".to_string(),
        "mime_type".to_string(),
        "other_accessor".to_string(),
    ]
    .into_iter()
    .collect();
    let r = FieldResolver::new(
        &HashMap::new(),
        &HashSet::new(),
        &result_fields,
        &HashSet::new(),
        &method_calls,
    );
    assert_eq!(r.accessor("content", "rust", "result"), "result.content");
    assert_eq!(r.accessor("mime_type", "rust", "result"), "result.mime_type");
    // A path that's in method_calls but NOT in result_fields still renders
    // as a method call — the override is targeted at result-root fields.
    assert_eq!(
        r.accessor("other_accessor", "rust", "result"),
        "result.other_accessor()"
    );
}

/// PHP `needs_getter`: when the owner type declares the field but has no
/// entry in `getters` (i.e. all its fields are scalar), the answer must be
/// `false` — without falling back to the global bare-name union.
///
/// Regression: a fixture DTO `DocumentResult` declares `content: String`
/// (scalar) and has no entry in `getters`. Some other type in the workspace
/// declares `content` as non-scalar (e.g. a chunk struct). The legacy
/// fallback would flip `$result->content` to `$result->getContent()` based
/// on the bare-name union — producing a "method does not exist" error
/// against the actual ProcessingResult class. The fix: when owner is known
/// and declares the field, trust the per-type getters map exclusively.
#[test]
fn render_php_needs_getter_returns_false_when_owner_has_no_getter_entry() {
    let getters: HashMap<String, HashSet<String>> = {
        let mut m = HashMap::new();
        // Chunk.content is non-scalar (some Vec<Span>); only Chunk has a
        // getters entry.
        m.insert("Chunk".to_string(), ["content".to_string()].into_iter().collect());
        m
    };
    let all_fields: HashMap<String, HashSet<String>> = {
        let mut m = HashMap::new();
        m.insert(
            "ProcessingResult".to_string(),
            ["content".to_string()].into_iter().collect(),
        );
        m.insert("Chunk".to_string(), ["content".to_string()].into_iter().collect());
        m
    };
    let map = PhpGetterMap {
        getters,
        field_types: HashMap::new(),
        root_type: Some("ProcessingResult".to_string()),
        all_fields,
    };
    // ProcessingResult declares `content` but has no getters entry — must be
    // treated as scalar, NOT flipped to getter syntax via bare-name union.
    assert!(!map.needs_getter(Some("ProcessingResult"), "content"));
    // Chunk DOES need getter syntax (entry exists).
    assert!(map.needs_getter(Some("Chunk"), "content"));
    // Unknown owner still uses the bare-name fallback (legacy behaviour).
    assert!(map.needs_getter(None, "content"));

    // Confirm end-to-end accessor rendering matches.
    let r = FieldResolver::new_with_php_getters(
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        map,
    );
    assert_eq!(r.accessor("content", "php", "$result"), "$result->content");
}
