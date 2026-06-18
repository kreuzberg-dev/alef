use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FfiConfig {
    pub prefix: Option<String>,
    #[serde(default = "default_error_style")]
    pub error_style: String,
    pub header_name: Option<String>,
    /// Native library name for Go cgo/Java Panama/C# P/Invoke (e.g., "sample_pack_ffi").
    /// Defaults to `{prefix}_ffi`.
    #[serde(default)]
    pub lib_name: Option<String>,
    /// If true, generate visitor/callback FFI support.
    #[serde(default)]
    pub visitor_callbacks: bool,
    #[serde(default)]
    pub features: Option<Vec<String>>,
    /// Core-crate features that must be *declared* on the generated FFI crate so
    /// that `#[cfg(feature = "X")]` gates in the FFI source compile cleanly under
    /// `RUSTFLAGS="-D warnings"`, but must NOT be enabled by default.
    ///
    /// Use this for mutually-exclusive alternatives to a default feature — e.g. a
    /// `wasm-http` HTTP backend that the FFI source references in
    /// `#[cfg(any(feature = "native-http", feature = "wasm-http"))]` but which must
    /// never be active alongside the default `native-http`. Each entry `X` emits a
    /// `X = ["<core-crate>/X"]` line in `[features]` without adding `X` to `default`.
    ///
    /// Defaults to empty — crates that don't reference non-default core features in
    /// their FFI `#[cfg]` gates can ignore this knob entirely.
    #[serde(default)]
    pub extra_features: Vec<String>,
    /// Override the serde rename_all strategy for JSON field names (e.g. "camelCase", "snake_case").
    /// When set, this takes priority over the IR type-level serde_rename_all.
    #[serde(default)]
    pub serde_rename_all: Option<String>,
    /// Functions to exclude from FFI binding generation.
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    /// Types to exclude from FFI binding generation.
    #[serde(default)]
    pub exclude_types: Vec<String>,
    /// Per-field name remapping for this language. Key is `TypeName.field_name`, value is the
    /// desired binding field name. Applied after automatic keyword escaping.
    #[serde(default)]
    pub rename_fields: HashMap<String, String>,
    /// Rust expression used to construct an error value of this crate's
    /// `error_type` from a runtime `String` message inside generated FFI
    /// trait-bridge plugin shims (`plugin_impl_initialize`, `plugin_impl_shutdown`).
    ///
    /// The expression has access to a local variable `msg: String` containing
    /// the underlying error message and is interpolated verbatim. Example
    /// values:
    ///
    /// ```toml
    /// # downstream whose error type has a struct variant with two fields:
    /// plugin_error_constructor = """
    /// sample_core::SampleCrateError::Plugin { message: msg, plugin_name: String::new() }
    /// """
    ///
    /// # downstream whose error type implements `From<String>`:
    /// plugin_error_constructor = "MyError::from(msg)"
    /// ```
    ///
    /// Defaults to `None`. When unset, the plugin shim still emits — backends
    /// fall back to a `format!("{}: {}", prefix, msg)`-style construction via
    /// the configured `error_constructor`. Downstreams that don't expose
    /// trait-bridged plugins can ignore this knob entirely.
    #[serde(default)]
    pub plugin_error_constructor: Option<String>,
    /// Per-target overrides for the core-crate dependency emitted into the
    /// generated FFI Cargo.toml. Used when some `cfg(...)` target requires a
    /// reduced feature set (e.g. the `x86_64-linux-android` emulator cannot
    /// link ONNX Runtime, so sample_core ships an `android-target` feature
    /// flag that drops every ORT-dependent extractor).
    ///
    /// When this list is non-empty the scaffold emits
    /// `[target.'cfg(not(<any-cfg>))'.dependencies]` for the default branch
    /// plus one `[target.'cfg(<cfg>)'.dependencies]` block per override,
    /// instead of the unconditional `[dependencies]` block.
    #[serde(default)]
    pub target_dep_overrides: Vec<FfiTargetDepOverride>,
}

/// A per-target replacement for the core-crate feature set emitted into the
/// generated FFI Cargo.toml. See [`FfiConfig::target_dep_overrides`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FfiTargetDepOverride {
    /// Cargo cfg expression, without the surrounding `cfg(...)`.
    /// Example: `all(target_os = "android", target_arch = "x86_64")`.
    pub cfg: String,
    /// Replacement feature set used for the core-crate dependency when this
    /// target matches. An empty list means "no features".
    #[serde(default)]
    pub features: Vec<String>,
}

fn default_error_style() -> String {
    "last_error".to_string()
}
