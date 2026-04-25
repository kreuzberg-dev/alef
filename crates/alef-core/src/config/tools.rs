//! Global tooling configuration.
//!
//! `[tools]` is a top-level section in `alef.toml` that selects per-language
//! package managers and dev-tool sets used by the default pipeline commands
//! (lint, test, build, setup, update, clean). Each field has a sensible default
//! so the section is fully optional; users only override what they need.

use serde::{Deserialize, Serialize};

/// Default Rust dev tools installed by `alef setup rust`.
/// Mirrors the polyrepo's `task setup` so binding generators get a consistent
/// developer environment out of the box.
pub const DEFAULT_RUST_DEV_TOOLS: &[&str] = &[
    "cargo-edit",
    "cargo-sort",
    "cargo-machete",
    "cargo-deny",
    "cargo-llvm-cov",
];

const DEFAULT_PYTHON_PM: &str = "uv";
const DEFAULT_NODE_PM: &str = "pnpm";

/// Top-level `[tools]` config. Selects which package manager / tool variants
/// the default per-language pipeline commands target.
///
/// All fields are optional; getters return the documented default when unset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    /// Python package manager. One of: `"uv"`, `"pip"`, `"poetry"`. Default: `"uv"`.
    #[serde(default)]
    pub python_package_manager: Option<String>,

    /// Node package manager. One of: `"pnpm"`, `"npm"`, `"yarn"`. Default: `"pnpm"`.
    #[serde(default)]
    pub node_package_manager: Option<String>,

    /// Rust dev tools installed by the Rust `setup` default.
    /// Default: see [`DEFAULT_RUST_DEV_TOOLS`].
    #[serde(default)]
    pub rust_dev_tools: Option<Vec<String>>,
}

/// Build a POSIX precondition that checks whether `tool` is on `PATH`.
///
/// The resulting command exits 0 when the tool is available and non-zero
/// otherwise. Used by per-language defaults so a missing tool causes a
/// graceful warn-and-skip rather than a hard failure.
pub fn require_tool(tool: &str) -> String {
    format!("command -v {tool} >/dev/null 2>&1")
}

/// Build a POSIX precondition requiring multiple tools to be on `PATH`.
///
/// Joins individual `command -v` checks with `&&` so the precondition only
/// passes when every listed tool is present.
pub fn require_tools(tools: &[&str]) -> String {
    tools.iter().map(|t| require_tool(t)).collect::<Vec<_>>().join(" && ")
}

impl ToolsConfig {
    /// Resolved Python package manager (defaults to `uv` when unset).
    pub fn python_pm(&self) -> &str {
        self.python_package_manager.as_deref().unwrap_or(DEFAULT_PYTHON_PM)
    }

    /// Resolved Node package manager (defaults to `pnpm` when unset).
    pub fn node_pm(&self) -> &str {
        self.node_package_manager.as_deref().unwrap_or(DEFAULT_NODE_PM)
    }

    /// Resolved Rust dev tools (defaults to [`DEFAULT_RUST_DEV_TOOLS`] when unset).
    pub fn rust_tools(&self) -> Vec<&str> {
        match self.rust_dev_tools.as_deref() {
            Some(list) => list.iter().map(String::as_str).collect(),
            None => DEFAULT_RUST_DEV_TOOLS.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_values() {
        let cfg = ToolsConfig::default();
        assert_eq!(cfg.python_pm(), "uv");
        assert_eq!(cfg.node_pm(), "pnpm");
        assert_eq!(
            cfg.rust_tools(),
            vec![
                "cargo-edit",
                "cargo-sort",
                "cargo-machete",
                "cargo-deny",
                "cargo-llvm-cov"
            ]
        );
    }

    #[test]
    fn getters_return_user_value_when_set() {
        let cfg = ToolsConfig {
            python_package_manager: Some("pip".to_string()),
            node_package_manager: Some("yarn".to_string()),
            rust_dev_tools: Some(vec!["cargo-foo".to_string(), "cargo-bar".to_string()]),
        };
        assert_eq!(cfg.python_pm(), "pip");
        assert_eq!(cfg.node_pm(), "yarn");
        assert_eq!(cfg.rust_tools(), vec!["cargo-foo", "cargo-bar"]);
    }

    #[test]
    fn empty_rust_dev_tools_is_respected() {
        // Users may explicitly opt out of installing any cargo tools.
        let cfg = ToolsConfig {
            rust_dev_tools: Some(vec![]),
            ..Default::default()
        };
        assert!(cfg.rust_tools().is_empty());
    }

    #[test]
    fn deserializes_from_toml() {
        let toml_str = r#"
            python_package_manager = "poetry"
            node_package_manager = "npm"
            rust_dev_tools = ["cargo-edit"]
        "#;
        let cfg: ToolsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.python_pm(), "poetry");
        assert_eq!(cfg.node_pm(), "npm");
        assert_eq!(cfg.rust_tools(), vec!["cargo-edit"]);
    }

    #[test]
    fn require_tool_emits_command_v() {
        assert_eq!(require_tool("ruff"), "command -v ruff >/dev/null 2>&1");
    }

    #[test]
    fn require_tools_joins_with_and() {
        assert_eq!(
            require_tools(&["go", "gofmt"]),
            "command -v go >/dev/null 2>&1 && command -v gofmt >/dev/null 2>&1"
        );
    }

    #[test]
    fn empty_toml_uses_defaults() {
        let cfg: ToolsConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.python_pm(), "uv");
        assert_eq!(cfg.node_pm(), "pnpm");
    }
}
