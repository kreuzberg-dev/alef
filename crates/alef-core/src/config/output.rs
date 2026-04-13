use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExcludeConfig {
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
    /// Exclude specific methods: "TypeName.method_name"
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncludeConfig {
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputConfig {
    pub python: Option<PathBuf>,
    pub node: Option<PathBuf>,
    pub ruby: Option<PathBuf>,
    pub php: Option<PathBuf>,
    pub elixir: Option<PathBuf>,
    pub wasm: Option<PathBuf>,
    pub ffi: Option<PathBuf>,
    pub go: Option<PathBuf>,
    pub java: Option<PathBuf>,
    pub csharp: Option<PathBuf>,
    pub r: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaffoldConfig {
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadmeConfig {
    pub template_dir: Option<PathBuf>,
    pub snippets_dir: Option<PathBuf>,
    /// Deprecated: path to an external YAML config file. Prefer inline fields below.
    pub config: Option<PathBuf>,
    pub output_pattern: Option<String>,
    /// Discord invite URL used in README templates.
    pub discord_url: Option<String>,
    /// Banner image URL used in README templates.
    pub banner_url: Option<String>,
    /// Per-language README configuration, keyed by language code
    /// (e.g. "python", "typescript", "ruby"). Values are flexible JSON objects
    /// that map directly to minijinja template context variables.
    #[serde(default)]
    pub languages: HashMap<String, JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub format: Option<String>,
    pub check: Option<String>,
    pub typecheck: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestConfig {
    /// Command to run unit/integration tests for this language.
    pub command: Option<String>,
    /// Command to run e2e tests for this language.
    pub e2e: Option<String>,
}

/// A single text replacement rule for version sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextReplacement {
    /// Glob pattern for files to process.
    pub path: String,
    /// Regex pattern to search for (may contain `{version}` placeholder).
    pub search: String,
    /// Replacement string (may contain `{version}` placeholder).
    pub replace: String,
}

/// Configuration for the `sync-versions` command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncConfig {
    /// Extra file paths to update version in (glob patterns).
    #[serde(default)]
    pub extra_paths: Vec<String>,
    /// Arbitrary text replacements applied during version sync.
    #[serde(default)]
    pub text_replacements: Vec<TextReplacement>,
}
