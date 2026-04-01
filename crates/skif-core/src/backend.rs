use crate::config::{Language, SkifConfig};
use crate::ir::ApiSurface;
use std::path::PathBuf;

/// A generated file to write to disk.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    /// Path relative to the output root.
    pub path: PathBuf,
    /// File content.
    pub content: String,
    /// Whether to prepend a "DO NOT EDIT" header.
    pub generated_header: bool,
}

/// Capabilities supported by a backend.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub supports_async: bool,
    pub supports_classes: bool,
    pub supports_enums: bool,
    pub supports_option: bool,
    pub supports_result: bool,
    pub supports_callbacks: bool,
    pub supports_streaming: bool,
}

/// Trait that all language backends implement.
pub trait Backend: Send + Sync {
    /// Backend identifier (e.g., "pyo3", "napi", "ffi").
    fn name(&self) -> &str;

    /// Target language.
    fn language(&self) -> Language;

    /// What this backend supports.
    fn capabilities(&self) -> Capabilities;

    /// Generate binding source code.
    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>>;

    /// Generate type stubs (.pyi, .rbs, .d.ts). Optional — default returns empty.
    fn generate_type_stubs(&self, _api: &ApiSurface, _config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        Ok(vec![])
    }

    /// Generate package scaffolding. Optional — default returns empty.
    fn generate_scaffold(&self, _api: &ApiSurface, _config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        Ok(vec![])
    }
}
