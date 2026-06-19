//! Shared host-native capsule (Language-passthrough) config for the C-ABI family backends.
//!
//! Every C-ABI binding (Go, Java, C#, Swift, Dart, Zig, Kotlin Android) links the same C
//! symbol emitted by the FFI backend, which returns the host runtime's raw grammar pointer
//! (`const TSLanguage *`) for capsule types instead of an opaque alef handle. Each binding
//! then wraps that raw pointer in its own ecosystem's native `Language` type.
//!
//! This struct captures the per-backend host construction: the host type name to annotate the
//! return as, the package/module to depend on, its version, and (optionally) an override of the
//! construction expression. The `{ptr}` placeholder in `construct_expr` is replaced with the raw
//! pointer expression at the FFI boundary in the target language.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Host-native capsule config for a single type in one C-ABI family backend.
///
/// TOML form (Go example):
/// ```toml
/// [crates.go.capsule_types.Language]
/// host_type = "*tree_sitter.Language"
/// package = "github.com/tree-sitter/go-tree-sitter"
/// package_version = "v0.25.0"
/// construct_expr = "tree_sitter.NewLanguage(unsafe.Pointer({ptr}))"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct HostCapsuleTypeConfig {
    /// The host ecosystem's `Language` type, used as the return-type annotation in the
    /// generated binding (e.g. `"*tree_sitter.Language"` for Go,
    /// `"SwiftTreeSitter.Language"` for Swift, `"TreeSitter.Language"` for C#).
    pub host_type: String,
    /// The host package/module identifier to depend on (e.g.
    /// `"github.com/tree-sitter/go-tree-sitter"`, `"SwiftTreeSitter"`,
    /// `"TreeSitter.DotNet"`, `"io.github.tree-sitter:jtreesitter"`). Injected into the
    /// backend's package manifest by the scaffold layer. Empty string disables injection.
    #[serde(default)]
    pub package: String,
    /// The version constraint for `package` (e.g. `"v0.25.0"`, `"0.8.0"`, `"1.0.0"`).
    /// Format is backend-specific and passed through verbatim to the manifest.
    #[serde(default)]
    pub package_version: String,
    /// The construction expression that wraps the raw FFI pointer in the host `Language`.
    /// The `{ptr}` placeholder is substituted with the raw-pointer expression produced at
    /// the FFI boundary. When empty, the backend uses its built-in default for the ecosystem.
    #[serde(default)]
    pub construct_expr: String,
}

impl HostCapsuleTypeConfig {
    /// Returns the construction expression with `{ptr}` substituted by `ptr_expr`,
    /// falling back to `default_expr` (also `{ptr}`-templated) when `construct_expr` is empty.
    pub fn construct(&self, ptr_expr: &str, default_expr: &str) -> String {
        let template = if self.construct_expr.is_empty() {
            default_expr
        } else {
            self.construct_expr.as_str()
        };
        template.replace("{ptr}", ptr_expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_substitutes_ptr_placeholder() {
        let cfg = HostCapsuleTypeConfig {
            host_type: "*tree_sitter.Language".to_string(),
            package: "github.com/tree-sitter/go-tree-sitter".to_string(),
            package_version: "v0.25.0".to_string(),
            construct_expr: "tree_sitter.NewLanguage(unsafe.Pointer({ptr}))".to_string(),
        };
        assert_eq!(
            cfg.construct("ptr", "DEFAULT"),
            "tree_sitter.NewLanguage(unsafe.Pointer(ptr))"
        );
    }

    #[test]
    fn construct_falls_back_to_default_when_empty() {
        let cfg = HostCapsuleTypeConfig {
            host_type: "*tree_sitter.Language".to_string(),
            package: String::new(),
            package_version: String::new(),
            construct_expr: String::new(),
        };
        assert_eq!(
            cfg.construct("ptr", "tree_sitter.NewLanguage(unsafe.Pointer({ptr}))"),
            "tree_sitter.NewLanguage(unsafe.Pointer(ptr))"
        );
    }
}
