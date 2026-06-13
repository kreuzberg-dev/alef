use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SwiftConfig {
    /// Swift module name (e.g. `"MyLibrary"`). Defaults to PascalCase of the crate name.
    #[serde(default)]
    pub module_name: Option<String>,
    /// Swift package name. Defaults to the module name.
    #[serde(default)]
    pub package_name: Option<String>,
    /// swift-bridge version. Defaults to `template_versions::cargo::SWIFT_BRIDGE` when unset.
    #[serde(default)]
    pub swift_bridge_version: Option<String>,
    /// Minimum macOS deployment target. Defaults to `template_versions::toolchain::SWIFT_MIN_MACOS` when unset.
    #[serde(default)]
    pub min_macos_version: Option<String>,
    /// Minimum iOS deployment target. Defaults to `template_versions::toolchain::SWIFT_MIN_IOS` when unset.
    #[serde(default)]
    pub min_ios_version: Option<String>,
    /// Cargo features to enable on the binding crate.
    #[serde(default)]
    pub features: Option<Vec<String>>,
    /// Override the serde rename_all strategy for JSON field names (e.g. "camelCase", "snake_case").
    #[serde(default)]
    pub serde_rename_all: Option<String>,
    /// Per-field name remapping. Key is `TypeName.field_name`, value is the
    /// desired binding field name. Applied after automatic keyword escaping.
    #[serde(default)]
    pub rename_fields: HashMap<String, String>,
    /// Functions to exclude from Swift binding generation.
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    /// Types to exclude from Swift binding generation.
    #[serde(default)]
    pub exclude_types: Vec<String>,
    /// Fields to exclude from Swift binding generation.
    /// Format: `"TypeName.field_name"`.
    #[serde(default)]
    pub exclude_fields: Vec<String>,
    /// Prefix wrapper for default tool invocations.
    #[serde(default)]
    pub run_wrapper: Option<String>,
    /// Extra paths to append to default lint commands.
    #[serde(default)]
    pub extra_lint_paths: Vec<String>,
    /// Override the core Cargo dependency name and path for the Swift binding crate.
    /// When set, the binding `Cargo.toml` depends on this crate (resolved as
    /// `../../../crates/<override>`) instead of the umbrella `[crate.name]`.
    /// Defaults to unset.
    #[serde(default)]
    pub core_crate_override: Option<String>,
    /// Extra Cargo dependencies merged into the generated Swift Rust bridge crate.
    #[serde(default)]
    #[schemars(with = "HashMap<String, serde_json::Value>")]
    pub extra_dependencies: HashMap<String, toml::Value>,
    /// Keys to subtract from the merged `extra_dependencies` set for this
    /// language only.
    #[serde(default)]
    pub exclude_extra_dependencies: Vec<String>,
    /// Override the auto-generated `create_<type>(api_key, base_url)` constructor
    /// body for opaque client types that expose methods. When set, the swift backend
    /// emits this snippet verbatim as the function body (no implicit `Ok(...)`).
    ///
    /// Use this when the source crate's constructor signature differs from the
    /// default `Type::new(api_key, base_url)` shape — e.g. some clients use
    /// `DefaultClient::new(ClientConfig, Option<&str>)` and needs to build a
    /// `ClientConfig` from the bridge inputs first.
    ///
    /// The snippet is parameterised by `{type_name}` (the wrapper newtype name)
    /// and runs in a function body with `api_key: String` and `base_url: Option<String>`
    /// already in scope. It must return `Result<{type_name}, String>`.
    #[serde(default)]
    pub client_constructor_body: HashMap<String, String>,
    /// Per-target overrides for the core Cargo dependency. Each entry replaces
    /// the default `[dependencies]` entry with a `[target.'cfg(...)'.dependencies]`
    /// block scoped to the cfg predicate. When non-empty, the default entry is
    /// gated on `cfg(not(any(<override cfgs>)))` so exactly one branch matches
    /// per build target.
    ///
    /// Mirrors `DartTargetDepOverride`. Needed because `libheif-sys` (pulled in
    /// via the `heic` feature) cannot cross-compile to iOS or Android NDK
    /// targets and is not on the Windows runner image's library path.
    #[serde(default)]
    pub target_dep_overrides: Vec<SwiftTargetDepOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SwiftTargetDepOverride {
    /// Cargo `cfg(...)` predicate (without the `cfg(...)` wrapper). Example:
    /// `target_os = "ios"`.
    pub cfg: String,
    /// Features to enable on the core dependency for this target.
    #[serde(default)]
    pub features: Vec<String>,
    /// When false (default), emit `default-features = false` for this target.
    /// When true, allow the core dep's default features through.
    #[serde(default)]
    pub default_features: bool,
}
