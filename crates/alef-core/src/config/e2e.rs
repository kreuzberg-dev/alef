//! E2E test generation configuration types.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Root e2e configuration from `[e2e]` section of alef.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct E2eConfig {
    /// Directory containing fixture JSON files (default: "fixtures").
    #[serde(default = "default_fixtures_dir")]
    pub fixtures: String,
    /// Output directory for generated e2e test projects (default: "e2e").
    #[serde(default = "default_output_dir")]
    pub output: String,
    /// Languages to generate e2e tests for. Defaults to top-level `languages` list.
    #[serde(default)]
    pub languages: Vec<String>,
    /// Function call configuration.
    pub call: CallConfig,
    /// Per-language package reference overrides.
    #[serde(default)]
    pub packages: HashMap<String, PackageRef>,
    /// Per-language formatter commands.
    #[serde(default)]
    pub format: HashMap<String, String>,
    /// Field path aliases: maps fixture field paths to actual API struct paths.
    /// E.g., "metadata.title" -> "metadata.document.title"
    /// Supports struct access (foo.bar), map access (foo[key]), direct fields.
    #[serde(default)]
    pub fields: HashMap<String, String>,
    /// Fields that are Optional/nullable in the return type.
    /// Rust generators use .as_deref().unwrap_or("") for strings, .is_some() for structs.
    #[serde(default)]
    pub fields_optional: HashSet<String>,
    /// Fields that are arrays/Vecs on the result type.
    /// When a fixture path like `json_ld.name` traverses an array field, the
    /// accessor adds `[0]` (or language equivalent) to index into the first element.
    #[serde(default)]
    pub fields_array: HashSet<String>,
    /// Known top-level fields on the result type.
    ///
    /// When non-empty, assertions whose resolved field path starts with a
    /// segment that is NOT in this set are emitted as comments (skipped)
    /// instead of executable assertions.  This prevents broken assertions
    /// when fixtures reference fields from a different operation (e.g.,
    /// `batch.completed_count` on a `ScrapeResult`).
    #[serde(default)]
    pub result_fields: HashSet<String>,
    /// C FFI accessor type chain: maps `"{parent_snake_type}.{field}"` to the
    /// PascalCase return type name (without prefix).
    ///
    /// Used by the C e2e generator to emit chained FFI accessor calls for
    /// nested field paths. The root type is always `conversion_result`.
    ///
    /// Example:
    /// ```toml
    /// [e2e.fields_c_types]
    /// "conversion_result.metadata" = "HtmlMetadata"
    /// "html_metadata.document" = "DocumentMetadata"
    /// ```
    #[serde(default)]
    pub fields_c_types: HashMap<String, String>,
}

fn default_fixtures_dir() -> String {
    "fixtures".to_string()
}

fn default_output_dir() -> String {
    "e2e".to_string()
}

/// Configuration for the function call in each test.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallConfig {
    /// The function name (alef applies language naming conventions).
    #[serde(default)]
    pub function: String,
    /// The module/package where the function lives.
    #[serde(default)]
    pub module: String,
    /// Variable name for the return value (default: "result").
    #[serde(default = "default_result_var")]
    pub result_var: String,
    /// Whether the function is async.
    #[serde(default)]
    pub r#async: bool,
    /// How fixture `input` fields map to function arguments.
    #[serde(default)]
    pub args: Vec<ArgMapping>,
    /// Per-language overrides for module/function/etc.
    #[serde(default)]
    pub overrides: HashMap<String, CallOverride>,
}

fn default_result_var() -> String {
    "result".to_string()
}

/// Maps a fixture input field to a function argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgMapping {
    /// Argument name in the function signature.
    pub name: String,
    /// JSON field path in the fixture's `input` object.
    pub field: String,
    /// Type hint for code generation.
    #[serde(rename = "type", default = "default_arg_type")]
    pub arg_type: String,
    /// Whether this argument is optional.
    #[serde(default)]
    pub optional: bool,
}

fn default_arg_type() -> String {
    "string".to_string()
}

/// Per-language override for function call configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallOverride {
    /// Override the module/import path.
    #[serde(default)]
    pub module: Option<String>,
    /// Override the function name.
    #[serde(default)]
    pub function: Option<String>,
    /// Override the crate name (Rust only).
    #[serde(default)]
    pub crate_name: Option<String>,
    /// Override the class name (Java/C# only).
    #[serde(default)]
    pub class: Option<String>,
    /// Import alias (Go only, e.g., `htmd`).
    #[serde(default)]
    pub alias: Option<String>,
    /// C header file name (C only).
    #[serde(default)]
    pub header: Option<String>,
    /// FFI symbol prefix (C only).
    #[serde(default)]
    pub prefix: Option<String>,
    /// For json_object args: the constructor to use instead of raw dict/object.
    /// E.g., "ConversionOptions" — generates `ConversionOptions(**options)` in Python,
    /// `new ConversionOptions(options)` in TypeScript.
    #[serde(default)]
    pub options_type: Option<String>,
    /// How to pass json_object args: "kwargs" (default), "dict", or "json".
    ///
    /// - `"kwargs"`: construct `OptionsType(key=val, ...)` (requires `options_type`).
    /// - `"dict"`: pass as a plain dict/object literal `{"key": "val"}`.
    /// - `"json"`: pass via `json.loads('...')` / `JSON.parse('...')`.
    #[serde(default)]
    pub options_via: Option<String>,
    /// Maps fixture option field names to their enum type names.
    /// E.g., `{"headingStyle": "HeadingStyle", "codeBlockStyle": "CodeBlockStyle"}`.
    /// The generator imports these types and maps string values to enum constants.
    #[serde(default)]
    pub enum_fields: HashMap<String, String>,
    /// Module to import enum types from (if different from the main module).
    /// E.g., "html_to_markdown._html_to_markdown" for PyO3 native enums.
    #[serde(default)]
    pub enum_module: Option<String>,
    /// When `true`, the function returns a simple type (e.g., `String`) rather
    /// than a struct.  Generators that would normally emit `result.content`
    /// (or equivalent field access) will use the result variable directly.
    #[serde(default)]
    pub result_is_simple: bool,
}

/// Per-language package reference configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageRef {
    /// Package/crate/gem/module name.
    #[serde(default)]
    pub name: Option<String>,
    /// Relative path from e2e/{lang}/ to the package.
    #[serde(default)]
    pub path: Option<String>,
    /// Go module path.
    #[serde(default)]
    pub module: Option<String>,
    /// Package version (e.g., for go.mod require directives).
    #[serde(default)]
    pub version: Option<String>,
}
