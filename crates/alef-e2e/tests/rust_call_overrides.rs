//! Verifies the Rust e2e codegen honours `wrap_options_in_some` and `extra_args`
//! overrides from `[e2e.call.overrides.rust]`.
//!
//! These are needed for fallible signatures whose options slot is owned `Option<T>`
//! (rather than borrowed `&T`) and which take additional trailing positional args
//! the fixture cannot supply (e.g. `convert(html, options, visitor) -> Result<…>`).
//!
//! Without them the generator emits `&options` against an `Option<T>` slot, omits
//! the trailing arg, and produces uncompilable output (E0061, E0308, E0609).

use alef_core::config::AlefConfig;
use alef_e2e::codegen::E2eCodegen;
use alef_e2e::codegen::rust::RustE2eCodegen;
use alef_e2e::fixture::{Assertion, Fixture, FixtureGroup};

fn build_config(extra_call_override: &str) -> AlefConfig {
    let toml_src = format!(
        r#"
languages = ["rust"]

[crate]
name = "html-to-markdown-rs"
sources = ["src/lib.rs"]

[e2e]
fixtures = "fixtures"
output = "e2e"

[e2e.call]
function = "convert"
module = "html_to_markdown_rs"
args = [
  {{ name = "html", field = "html", type = "string" }},
  {{ name = "options", field = "options", type = "json_object", optional = true }},
]

[e2e.call.overrides.rust]
crate_name = "html_to_markdown_rs"
function = "convert"
{extra_call_override}
"#
    );
    toml::from_str(&toml_src).expect("config parses")
}

fn build_fixture() -> FixtureGroup {
    FixtureGroup {
        category: "smoke".to_string(),
        fixtures: vec![Fixture {
            id: "smoke_basic".to_string(),
            category: Some("smoke".to_string()),
            description: "basic conversion".to_string(),
            tags: Vec::new(),
            skip: None,
            call: None,
            input: serde_json::json!({
                "html": "<p>hi</p>",
                "options": { "headingStyle": "atx" },
            }),
            mock_response: None,
            visitor: None,
            assertions: vec![Assertion {
                assertion_type: "not_empty".to_string(),
                field: Some("content".to_string()),
                value: None,
                values: None,
                method: None,
                check: None,
                args: None,
            }],
            source: "test.json".to_string(),
            http: None,
        }],
    }
}

fn render_rust_test(config: &AlefConfig) -> String {
    let groups = vec![build_fixture()];
    let files = RustE2eCodegen
        .generate(&groups, &config.e2e.clone().unwrap(), config)
        .expect("generation succeeds");
    let test_file = files
        .iter()
        .find(|f| f.path.to_string_lossy().contains("smoke_test.rs"))
        .expect("smoke_test.rs is emitted");
    test_file.content.clone()
}

#[test]
fn default_options_pass_by_reference() {
    // Without wrap_options_in_some, json_object args render as `&options`.
    let config = build_config("");
    let rendered = render_rust_test(&config);
    assert!(
        rendered.contains("convert(&html, &options)"),
        "default rust override should pass json_object args by reference. Rendered:\n{rendered}"
    );
}

#[test]
fn wrap_options_in_some_emits_some_clone() {
    // With wrap_options_in_some = true, the json_object expression is wrapped
    // in `Some(...).clone()` so it matches owned `Option<T>` parameter slots.
    let config = build_config("wrap_options_in_some = true");
    let rendered = render_rust_test(&config);
    assert!(
        rendered.contains("Some(options.clone())"),
        "wrap_options_in_some should emit `Some(options.clone())`. Rendered:\n{rendered}"
    );
    assert!(
        !rendered.contains("convert(&html, &options"),
        "wrap_options_in_some must not emit the default `&options` form. Rendered:\n{rendered}"
    );
}

#[test]
fn extra_args_are_appended_after_configured_args() {
    // extra_args = ["None"] must be emitted verbatim after html and options,
    // matching e.g. `convert(html, options, visitor) -> Result<…>`.
    let config = build_config(r#"extra_args = ["None"]"#);
    let rendered = render_rust_test(&config);
    assert!(
        rendered.contains(", None)"),
        "extra_args entry `None` must be appended as a trailing positional arg. Rendered:\n{rendered}"
    );
}

#[test]
fn wrap_options_in_some_combined_with_extra_args_and_returns_result() {
    // The full html-to-markdown shape: owned options slot, trailing visitor slot,
    // and a fallible return that triggers `.expect("should succeed")`.
    let config = build_config(
        r#"
wrap_options_in_some = true
extra_args = ["None"]
returns_result = true
"#,
    );
    let rendered = render_rust_test(&config);
    assert!(
        rendered.contains("convert(&html, Some(options.clone()), None)"),
        "combined overrides should emit the full 3-arg call shape. Rendered:\n{rendered}"
    );
    assert!(
        rendered.contains(".expect(\"should succeed\")"),
        "returns_result = true must emit the `.expect(...)` unwrap. Rendered:\n{rendered}"
    );
}
