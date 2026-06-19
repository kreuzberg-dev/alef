use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ZigConfig {
    /// Map of Rust type name -> host-native capsule (Language-passthrough) config.
    /// When set, functions returning the type construct the host runtime's native
    /// `Language` (e.g. `*const tree_sitter.Language`) from the raw C grammar pointer
    /// instead of an opaque handle. See [`crate::core::config::HostCapsuleTypeConfig`].
    #[serde(default)]
    pub capsule_types: HashMap<String, crate::core::config::HostCapsuleTypeConfig>,
    pub module_name: Option<String>,
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
    /// Functions to exclude from Zig binding generation.
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    /// Types to exclude from Zig binding generation.
    #[serde(default)]
    pub exclude_types: Vec<String>,
    /// Prefix wrapper for default tool invocations.
    #[serde(default)]
    pub run_wrapper: Option<String>,
    /// Extra paths to append to default lint commands.
    #[serde(default)]
    pub extra_lint_paths: Vec<String>,
    /// List of static-compiled languages supported by the Zig binding.
    /// When set, e2e fixtures whose `input.language` (or `input.config.language`)
    /// falls outside this set are omitted from the generated test file entirely.
    /// This bridges the gap between the full language pack and Zig's
    /// static-compiled grammar set (Zig does not currently dynamically load
    /// grammars at runtime).
    /// Defaults to empty (all languages assumed supported).
    #[serde(default)]
    pub languages: Vec<String>,
}
