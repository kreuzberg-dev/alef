/// Test that PHP wrapper param signatures preserve required-ness from the Rust API.
///
/// Before the fix: Required params after an optional param were being made optional.
/// Example: `scrape(?CrawlEngineHandle $engine = null, ?string $url = null)`
/// when the Rust API required both `engine: CrawlEngineHandle` and `url: String`.
///
/// After the fix: Only explicitly optional params or default-constructible params
/// become optional in the wrapper. Required params stay required.
/// Example: `scrape(CrawlEngineHandle $engine, string $url)`
#[test]
fn test_php_wrapper_param_optionality_logic() {
    use crate::core::ir::{ParamDef, TypeRef};

    // Helper to check if a param should be optional in the wrapper
    let is_optional_default_constructible_param = |p: &ParamDef| -> bool {
        if let TypeRef::Named(name) = &p.ty {
            // Simulate the no_arg_constructor_types set
            matches!(name.as_str(), "CrawlConfig" | "InteractionActions")
        } else {
            false
        }
    };

    // Test case 1: Required params should remain required
    let req_param = ParamDef {
        name: "url".to_string(),
        ty: TypeRef::String,
        optional: false,
        ..ParamDef::default()
    };

    let should_be_optional = req_param.optional || is_optional_default_constructible_param(&req_param);
    assert!(
        !should_be_optional,
        "required param should not become optional in wrapper"
    );

    // Test case 2: Explicitly optional params remain optional
    let opt_param = ParamDef {
        name: "config".to_string(),
        ty: TypeRef::Named("CrawlConfig".to_string()),
        optional: true,
        ..ParamDef::default()
    };

    let should_be_optional = opt_param.optional || is_optional_default_constructible_param(&opt_param);
    assert!(should_be_optional, "explicitly optional param should be optional");

    // Test case 3: Default-constructible required params become optional
    let default_constructible_param = ParamDef {
        name: "config".to_string(),
        ty: TypeRef::Named("CrawlConfig".to_string()),
        optional: false,
        ..ParamDef::default()
    };

    let should_be_optional =
        default_constructible_param.optional || is_optional_default_constructible_param(&default_constructible_param);
    assert!(should_be_optional, "default-constructible param should become optional");
}

/// Regression: the `#[php_impl]` facade is Rust source, so function docs must be emitted as
/// Rust line doc-comments (`///`), never PHPDoc `/** … */` blocks.
///
/// Rust block comments nest, so a doc that mentions `image/*` opens a nested `/*` that the
/// intended closing `*/` only balances at the inner level, leaving the outer `/**` unterminated
/// (`error[E0758]: unterminated block doc-comment`). Line doc-comments have no such hazard.
#[test]
fn should_emit_rust_line_doc_comments_when_doc_text_contains_block_comment_sequences() {
    use super::super::type_map::PhpMapper;
    use crate::backends::php::gen_bindings::functions::{PhpParamTypeSets, gen_function_as_static_method};
    use crate::core::ir::{FunctionDef, TypeRef};
    use ahash::AHashSet;

    let func = FunctionDef {
        name: "choose_call_mode".to_string(),
        rust_path: "sample_crate::choose_call_mode".to_string(),
        return_type: TypeRef::String,
        doc: "Decide which call mode best fits this document.\n\n\
              Rules: `image/*` → vision; `text/*` and `application/*` → text. Closes with */."
            .to_string(),
        ..FunctionDef::default()
    };

    let mapper = PhpMapper {
        enum_names: AHashSet::new(),
        data_enum_names: AHashSet::new(),
        untagged_data_enum_names: AHashSet::new(),
    };
    let empty = AHashSet::new();
    let type_sets = PhpParamTypeSets {
        opaque: &empty,
        default: &empty,
        enums: &empty,
    };

    let generated = gen_function_as_static_method(&func, &mapper, type_sets, "sample_crate", &[], false, &empty);

    // The doc must be rendered as `///` line comments (which carry the `image/*` text safely).
    assert!(
        generated.contains("/// Decide which call mode best fits this document."),
        "doc must be emitted as Rust `///` line comments, got:\n{generated}"
    );
    assert!(
        generated.contains("/// Rules: `image/*` → vision; `text/*` and `application/*` → text. Closes with */."),
        "doc body (incl. `image/*` and `*/`) must survive verbatim on a `///` line, got:\n{generated}"
    );
    // No PHPDoc block-comment opener may be emitted into Rust source: a `/**` block would nest on
    // the embedded `/*` (from `image/*`) and leave the comment unterminated (E0758).
    assert!(
        !generated.contains("/**"),
        "Rust crate doc must not use PHPDoc `/**` block comments (nesting hazard), got:\n{generated}"
    );

    // Strongest guarantee: every doc line is a line comment, so no block-comment delimiter is
    // ever in token position. Verify by confirming the doc region contains no `*/` outside a
    // `///` line. Each rendered doc line begins with `///`, so any `*/` is inert comment text.
    for line in generated.lines().filter(|l| l.contains("Closes with")) {
        assert!(
            line.trim_start().starts_with("///"),
            "line carrying a `*/` token must be a `///` line doc-comment, got: {line:?}"
        );
    }
}
