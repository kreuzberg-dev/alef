use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Root configuration from skif.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkifConfig {
    #[serde(rename = "crate")]
    pub crate_config: CrateConfig,
    pub languages: Vec<Language>,
    #[serde(default)]
    pub exclude: ExcludeConfig,
    #[serde(default)]
    pub include: IncludeConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub python: Option<PythonConfig>,
    #[serde(default)]
    pub node: Option<NodeConfig>,
    #[serde(default)]
    pub ruby: Option<RubyConfig>,
    #[serde(default)]
    pub php: Option<PhpConfig>,
    #[serde(default)]
    pub elixir: Option<ElixirConfig>,
    #[serde(default)]
    pub wasm: Option<WasmConfig>,
    #[serde(default)]
    pub ffi: Option<FfiConfig>,
    #[serde(default)]
    pub go: Option<GoConfig>,
    #[serde(default)]
    pub java: Option<JavaConfig>,
    #[serde(default)]
    pub csharp: Option<CSharpConfig>,
    #[serde(default)]
    pub r: Option<RConfig>,
    #[serde(default)]
    pub scaffold: Option<ScaffoldConfig>,
    #[serde(default)]
    pub readme: Option<ReadmeConfig>,
    #[serde(default)]
    pub lint: Option<HashMap<String, LintConfig>>,
    #[serde(default)]
    pub custom_files: Option<HashMap<String, Vec<PathBuf>>>,
    #[serde(default)]
    pub adapters: Vec<AdapterConfig>,
    #[serde(default)]
    pub custom_modules: CustomModulesConfig,
    #[serde(default)]
    pub custom_registrations: CustomRegistrationsConfig,
    /// Declare opaque types from external crates that skif can't extract.
    /// Map of type name → Rust path (e.g., "Tree" = "tree_sitter_language_pack::Tree").
    /// These get opaque wrapper structs in all backends.
    #[serde(default)]
    pub opaque_types: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateConfig {
    pub name: String,
    pub sources: Vec<PathBuf>,
    #[serde(default = "default_version_from")]
    pub version_from: String,
    #[serde(default)]
    pub core_import: Option<String>,
    /// Optional workspace root path for resolving `pub use` re-exports from sibling crates.
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
    /// When true, skip adding `use {core_import};` to generated bindings.
    #[serde(default)]
    pub skip_core_import: bool,
    /// Maps extracted rust_path prefixes to actual import paths in binding crates.
    /// Example: { "spikard" = "spikard_http" } rewrites "spikard::ServerConfig" to "spikard_http::ServerConfig"
    #[serde(default)]
    pub path_mappings: HashMap<String, String>,
}

fn default_version_from() -> String {
    "Cargo.toml".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    Node,
    Ruby,
    Php,
    Elixir,
    Wasm,
    Ffi,
    Go,
    Java,
    Csharp,
    R,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Python => write!(f, "python"),
            Self::Node => write!(f, "node"),
            Self::Ruby => write!(f, "ruby"),
            Self::Php => write!(f, "php"),
            Self::Elixir => write!(f, "elixir"),
            Self::Wasm => write!(f, "wasm"),
            Self::Ffi => write!(f, "ffi"),
            Self::Go => write!(f, "go"),
            Self::Java => write!(f, "java"),
            Self::Csharp => write!(f, "csharp"),
            Self::R => write!(f, "r"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExcludeConfig {
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
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
pub struct PythonConfig {
    pub module_name: Option<String>,
    pub async_runtime: Option<String>,
    pub stubs: Option<StubsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubsConfig {
    pub output: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub package_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubyConfig {
    pub gem_name: Option<String>,
    pub stubs: Option<StubsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpConfig {
    pub extension_name: Option<String>,
    /// Feature gate for ext-php-rs (default: "extension-module").
    /// All generated code is wrapped in `#[cfg(feature = "...")]`.
    #[serde(default)]
    pub feature_gate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElixirConfig {
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    #[serde(default)]
    pub exclude_types: Vec<String>,
    #[serde(default)]
    pub type_overrides: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiConfig {
    pub prefix: Option<String>,
    #[serde(default = "default_error_style")]
    pub error_style: String,
    pub header_name: Option<String>,
    /// Native library name for Go cgo/Java Panama/C# P/Invoke (e.g., "ts_pack_ffi").
    /// Defaults to `{prefix}_ffi`.
    #[serde(default)]
    pub lib_name: Option<String>,
}

fn default_error_style() -> String {
    "last_error".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoConfig {
    pub module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaConfig {
    pub package: Option<String>,
    #[serde(default = "default_java_ffi_style")]
    pub ffi_style: String,
}

fn default_java_ffi_style() -> String {
    "panama".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CSharpConfig {
    pub namespace: Option<String>,
    pub target_framework: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RConfig {
    pub package_name: Option<String>,
}

/// A parameter in an adapter function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub optional: bool,
}

/// The kind of adapter pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterPattern {
    SyncFunction,
    AsyncMethod,
    CallbackBridge,
    Streaming,
    ServerLifecycle,
}

/// Configuration for a single adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub name: String,
    pub pattern: AdapterPattern,
    /// Full Rust path to the core function/method (e.g., "html_to_markdown_rs::convert")
    pub core_path: String,
    /// Parameters
    #[serde(default)]
    pub params: Vec<AdapterParam>,
    /// Return type name
    pub returns: Option<String>,
    /// Error type name
    pub error_type: Option<String>,
    /// For async_method/streaming: the owning type name
    pub owner_type: Option<String>,
    /// For streaming: the item type
    pub item_type: Option<String>,
    /// For Python: release GIL during call
    #[serde(default)]
    pub gil_release: bool,
    /// For callback_bridge: the Rust trait to implement (e.g., "SpikardHandler")
    #[serde(default)]
    pub trait_name: Option<String>,
    /// For callback_bridge: the trait method name (e.g., "handle")
    #[serde(default)]
    pub trait_method: Option<String>,
    /// For callback_bridge: whether to detect async callbacks at construction time
    #[serde(default)]
    pub detect_async: bool,
}

/// Custom modules that skif should declare (mod X;) but not generate.
/// These are hand-written modules imported by the generated lib.rs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomModulesConfig {
    #[serde(default)]
    pub python: Vec<String>,
    #[serde(default)]
    pub node: Vec<String>,
    #[serde(default)]
    pub ruby: Vec<String>,
    #[serde(default)]
    pub php: Vec<String>,
    #[serde(default)]
    pub elixir: Vec<String>,
    #[serde(default)]
    pub wasm: Vec<String>,
    #[serde(default)]
    pub ffi: Vec<String>,
    #[serde(default)]
    pub go: Vec<String>,
    #[serde(default)]
    pub java: Vec<String>,
    #[serde(default)]
    pub csharp: Vec<String>,
    #[serde(default)]
    pub r: Vec<String>,
}

impl CustomModulesConfig {
    pub fn for_language(&self, lang: Language) -> &[String] {
        match lang {
            Language::Python => &self.python,
            Language::Node => &self.node,
            Language::Ruby => &self.ruby,
            Language::Php => &self.php,
            Language::Elixir => &self.elixir,
            Language::Wasm => &self.wasm,
            Language::Ffi => &self.ffi,
            Language::Go => &self.go,
            Language::Java => &self.java,
            Language::Csharp => &self.csharp,
            Language::R => &self.r,
        }
    }
}

/// Custom classes/functions from hand-written modules to register in module init.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomRegistration {
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
    #[serde(default)]
    pub init_calls: Vec<String>,
}

/// Per-language custom registrations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomRegistrationsConfig {
    #[serde(default)]
    pub python: Option<CustomRegistration>,
    #[serde(default)]
    pub node: Option<CustomRegistration>,
    #[serde(default)]
    pub ruby: Option<CustomRegistration>,
    #[serde(default)]
    pub php: Option<CustomRegistration>,
    #[serde(default)]
    pub elixir: Option<CustomRegistration>,
    #[serde(default)]
    pub wasm: Option<CustomRegistration>,
}

impl CustomRegistrationsConfig {
    pub fn for_language(&self, lang: Language) -> Option<&CustomRegistration> {
        match lang {
            Language::Python => self.python.as_ref(),
            Language::Node => self.node.as_ref(),
            Language::Ruby => self.ruby.as_ref(),
            Language::Php => self.php.as_ref(),
            Language::Elixir => self.elixir.as_ref(),
            Language::Wasm => self.wasm.as_ref(),
            _ => None,
        }
    }
}

/// Helper function to resolve output directory path from config.
/// Replaces {name} placeholder with the crate name.
pub fn resolve_output_dir(config_path: Option<&PathBuf>, crate_name: &str, default: &str) -> String {
    config_path
        .map(|p| p.to_string_lossy().replace("{name}", crate_name))
        .unwrap_or_else(|| default.replace("{name}", crate_name))
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
    pub config: Option<PathBuf>,
    pub output_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub format: Option<String>,
    pub check: Option<String>,
    pub typecheck: Option<String>,
}

// ---------------------------------------------------------------------------
// Shared config resolution helpers
// ---------------------------------------------------------------------------

impl SkifConfig {
    /// Get the core crate import path (e.g., "liter_llm"). Used by codegen to call into the core crate.
    pub fn core_import(&self) -> String {
        self.crate_config
            .core_import
            .clone()
            .unwrap_or_else(|| self.crate_config.name.replace('-', "_"))
    }

    /// Get the FFI prefix (e.g., "kreuzberg"). Used by FFI, Go, Java, C# backends.
    pub fn ffi_prefix(&self) -> String {
        self.ffi
            .as_ref()
            .and_then(|f| f.prefix.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.replace('-', "_"))
    }

    /// Get the FFI native library name (for Go cgo, Java Panama, C# P/Invoke).
    pub fn ffi_lib_name(&self) -> String {
        self.ffi
            .as_ref()
            .and_then(|f| f.lib_name.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("{}_ffi", self.ffi_prefix()))
    }

    /// Get the FFI header name.
    pub fn ffi_header_name(&self) -> String {
        self.ffi
            .as_ref()
            .and_then(|f| f.header_name.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("{}.h", self.ffi_prefix()))
    }

    /// Get the Python module name.
    pub fn python_module_name(&self) -> String {
        self.python
            .as_ref()
            .and_then(|p| p.module_name.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("_{}", self.crate_config.name.replace('-', "_")))
    }

    /// Get the Node package name.
    pub fn node_package_name(&self) -> String {
        self.node
            .as_ref()
            .and_then(|n| n.package_name.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.clone())
    }

    /// Get the Ruby gem name.
    pub fn ruby_gem_name(&self) -> String {
        self.ruby
            .as_ref()
            .and_then(|r| r.gem_name.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.replace('-', "_"))
    }

    /// Get the PHP extension name.
    pub fn php_extension_name(&self) -> String {
        self.php
            .as_ref()
            .and_then(|p| p.extension_name.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.replace('-', "_"))
    }

    /// Get the Elixir app name.
    pub fn elixir_app_name(&self) -> String {
        self.elixir
            .as_ref()
            .and_then(|e| e.app_name.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.replace('-', "_"))
    }

    /// Get the Go module path.
    pub fn go_module(&self) -> String {
        self.go
            .as_ref()
            .and_then(|g| g.module.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("github.com/kreuzberg-dev/{}", self.crate_config.name))
    }

    /// Get the Java package name.
    pub fn java_package(&self) -> String {
        self.java
            .as_ref()
            .and_then(|j| j.package.as_ref())
            .cloned()
            .unwrap_or_else(|| "dev.kreuzberg".to_string())
    }

    /// Get the C# namespace.
    pub fn csharp_namespace(&self) -> String {
        self.csharp
            .as_ref()
            .and_then(|c| c.namespace.as_ref())
            .cloned()
            .unwrap_or_else(|| {
                use heck::ToPascalCase;
                self.crate_config.name.to_pascal_case()
            })
    }

    /// Get the R package name.
    pub fn r_package_name(&self) -> String {
        self.r
            .as_ref()
            .and_then(|r| r.package_name.as_ref())
            .cloned()
            .unwrap_or_else(|| self.crate_config.name.clone())
    }

    /// Rewrite a rust_path using path_mappings.
    /// Matches the longest prefix first.
    pub fn rewrite_path(&self, rust_path: &str) -> String {
        // Sort mappings by key length descending (longest prefix first)
        let mut mappings: Vec<_> = self.crate_config.path_mappings.iter().collect();
        mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        for (from, to) in &mappings {
            if rust_path.starts_with(from.as_str()) {
                return format!("{}{}", to, &rust_path[from.len()..]);
            }
        }
        rust_path.to_string()
    }
}

/// Detect whether `serde` and `serde_json` are available in a binding crate's Cargo.toml.
///
/// `output_dir` is the generated source directory (e.g., `crates/spikard-py/src/`).
/// The function walks up to find the crate's Cargo.toml and checks its `[dependencies]`
/// for both `serde` and `serde_json`.
pub fn detect_serde_available(output_dir: &str) -> bool {
    let src_path = std::path::Path::new(output_dir);
    // Walk up from the output dir to find Cargo.toml (usually output_dir is `crates/foo/src/`)
    let mut dir = src_path;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            return cargo_toml_has_serde(&cargo_toml);
        }
        match dir.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => dir = parent,
            _ => break,
        }
    }
    false
}

/// Check if a Cargo.toml has both `serde` (with derive feature) and `serde_json` in its dependencies.
///
/// The `serde::Serialize` derive macro requires `serde` as a direct dependency with the `derive`
/// feature enabled. Having only `serde_json` is not sufficient since it only pulls in `serde`
/// transitively without the derive proc-macro.
fn cargo_toml_has_serde(path: &std::path::Path) -> bool {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let has_serde_json = content.contains("serde_json");
    // Check for `serde` as a direct dependency (not just serde_json).
    // Must match "serde" as a TOML key, not as a substring of "serde_json".
    // Valid patterns: `serde = `, `serde.`, `[dependencies.serde]`
    let has_serde_dep = content.lines().any(|line| {
        let trimmed = line.trim();
        // Match `serde = ...` or `serde.workspace = true` etc., but not `serde_json`
        trimmed.starts_with("serde ")
            || trimmed.starts_with("serde=")
            || trimmed.starts_with("serde.")
            || trimmed == "[dependencies.serde]"
    });

    has_serde_json && has_serde_dep
}
