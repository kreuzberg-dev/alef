use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::extras::Language;

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
    /// If true, generate visitor/callback FFI support:
    /// a `#[repr(C)]` callbacks struct, an opaque `Visitor` handle that implements
    /// the core visitor trait by calling the C function pointers, and
    /// `{prefix}_visitor_create` / `{prefix}_visitor_free` /
    /// `{prefix}_convert_with_visitor` exports.
    #[serde(default)]
    pub visitor_callbacks: bool,
}

fn default_error_style() -> String {
    "last_error".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoConfig {
    pub module: Option<String>,
    /// Override the Go package name (default: derived from module path)
    pub package_name: Option<String>,
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

/// Custom modules that alef should declare (mod X;) but not generate.
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
