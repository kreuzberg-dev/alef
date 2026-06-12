use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GleamConfig {
    pub app_name: Option<String>,
    /// Erlang atom name for `@external(erlang, "<nif>", ...)` lookups (e.g., "my_app_nif").
    /// Defaults to the app_name.
    #[serde(default)]
    pub nif_module: Option<String>,
    #[serde(default)]
    pub features: Option<Vec<String>>,
    /// Override the serde rename_all strategy for JSON field names (e.g. "camelCase", "snake_case").
    /// When set, this takes priority over the IR type-level serde_rename_all.
    #[serde(default)]
    pub serde_rename_all: Option<String>,
    /// Per-field name remapping for this language. Key is `TypeName.field_name`, value is the
    /// desired binding field name. Applied after automatic keyword escaping.
    #[serde(default)]
    pub rename_fields: HashMap<String, String>,
    /// Functions to exclude from Gleam binding generation.
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    /// Types to exclude from Gleam binding generation.
    #[serde(default)]
    pub exclude_types: Vec<String>,
    /// Prefix wrapper for default tool invocations.
    #[serde(default)]
    pub run_wrapper: Option<String>,
    /// Extra paths to append to default lint commands.
    #[serde(default)]
    pub extra_lint_paths: Vec<String>,
    /// Per-`element_type` Gleam record-constructor recipes used by the e2e
    /// generator when emitting `json_object` arg literals. Each entry maps a
    /// fixture-side `element_type` string (e.g. `"BatchFileItem"`) to a
    /// structured constructor description that the codegen interpolates per
    /// JSON-array item. Without an entry the codegen falls back to the
    /// `json_object_wrapper` (or a plain `json_to_gleam`).
    ///
    /// Example:
    ///
    /// ```toml
    /// [[crates.gleam.element_constructors]]
    /// element_type = "BatchFileItem"
    /// constructor = "sample_core.BatchFileItem"
    /// [[crates.gleam.element_constructors.fields]]
    /// gleam_field = "path"
    /// kind = "file_path"
    /// json_field = "path"
    /// [[crates.gleam.element_constructors.fields]]
    /// gleam_field = "config"
    /// kind = "literal"
    /// value = "option.None"
    /// ```
    #[serde(default)]
    pub element_constructors: Vec<GleamElementConstructor>,
    /// Optional Gleam expression template used to wrap `json_object` arg
    /// values when no `element_type` recipe matches. The placeholder
    /// `{json}` is replaced with a Gleam string literal containing the JSON
    /// form of the arg value, allowing the downstream's Gleam binding to do
    /// its own parsing.
    ///
    /// Example:
    ///
    /// ```toml
    /// [crates.gleam]
    /// json_object_wrapper = "sample_core.config_from_json_string({json})"
    /// ```
    ///
    /// When `None`, the codegen emits `{json}` verbatim (a plain Gleam
    /// string), matching the iter15 default.
    #[serde(default)]
    pub json_object_wrapper: Option<String>,
}

/// One per-`element_type` Gleam record-constructor recipe. Keyed by the
/// fixture-side `element_type` string and consumed by the e2e Gleam codegen
/// when building `json_object` arg literals.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GleamElementConstructor {
    /// Fixture-side `element_type` value this recipe applies to (e.g.
    /// `"BatchFileItem"`).
    pub element_type: String,
    /// Fully-qualified Gleam constructor identifier (e.g.
    /// `"sample_core.BatchFileItem"`). Emitted verbatim before the `(...)` field
    /// list.
    pub constructor: String,
    /// Ordered list of fields to emit inside the constructor's `(...)` block,
    /// in argument-position order. Each field describes how its value is
    /// derived from the per-item JSON object.
    pub fields: Vec<GleamElementField>,
}

/// One field inside a [`GleamElementConstructor`]'s argument list.
///
/// `kind` selects the source/encoding strategy:
/// * `"file_path"` — read `json_field` from the JSON object as a string,
///   prefix with the configured `test_documents_dir` when the value does not
///   start with `/`, and emit as a Gleam string literal.
/// * `"byte_array"` — read `json_field` from the JSON object as a JSON
///   `Array(Number)` and emit as a Gleam BitArray literal `<<n1, n2, …>>`.
/// * `"string"` — read `json_field` as a string, emit as a Gleam string
///   literal; falls back to `default` (or empty) if missing.
/// * `"literal"` — emit `value` verbatim (no JSON lookup). Use for
///   constant fields like `config: option.None`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GleamElementField {
    /// Gleam record field name (e.g. `"path"`, `"config"`).
    pub gleam_field: String,
    /// Source/encoding strategy. See struct doc.
    pub kind: String,
    /// JSON object key to read, when `kind` is one of the JSON-driven
    /// strategies. Required for `"file_path"`, `"byte_array"`, `"string"`;
    /// ignored for `"literal"`.
    #[serde(default)]
    pub json_field: Option<String>,
    /// Default Gleam expression when `json_field` is missing/null. Only
    /// honoured by the `"string"` strategy today.
    #[serde(default)]
    pub default: Option<String>,
    /// Verbatim Gleam expression to emit when `kind = "literal"`.
    #[serde(default)]
    pub value: Option<String>,
}
