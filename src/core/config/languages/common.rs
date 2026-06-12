use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct StubsConfig {
    pub output: PathBuf,
    /// When true, emit Rust `///` doc comments as stub-level docstrings.
    /// Default: false — ruff PYI021 flags docstrings in stub files.
    #[serde(default)]
    pub emit_docstrings: bool,
}
