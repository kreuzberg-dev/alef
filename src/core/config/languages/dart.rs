use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dart bridging style: FRB (default) or raw `dart:ffi`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DartStyle {
    /// flutter_rust_bridge — emits a Rust crate plus Dart wrappers using
    /// FRB-generated bridge symbols. Default.
    #[default]
    Frb,
    /// Raw `dart:ffi` over the cbindgen C ABI — emits Dart-only source that
    /// loads the shared library at runtime. Cheaper to ship; loses FRB's
    /// async ergonomics and freezed-style data classes.
    Ffi,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DartConfig {
    /// Dart pub.dev package name (e.g. `"my_package"`). Used as the `name` in
    /// `pubspec.yaml`. Defaults to a snake_case derivation of the crate name.
    #[serde(default)]
    pub pubspec_name: Option<String>,
    /// Dart library name (the `library` declaration). Defaults to the pubspec name.
    #[serde(default)]
    pub lib_name: Option<String>,
    /// Dart package name override (e.g. for pub.dev scoped packages).
    #[serde(default)]
    pub package_name: Option<String>,
    /// Bridging style. `"frb"` (default) uses flutter_rust_bridge; `"ffi"` emits
    /// raw `dart:ffi` source over the cbindgen C library.
    #[serde(default)]
    pub style: DartStyle,
    /// flutter_rust_bridge version to pin in generated pubspec.yaml.
    /// Defaults to `template_versions::cargo::FLUTTER_RUST_BRIDGE` when unset.
    #[serde(default)]
    pub frb_version: Option<String>,
    /// Cargo features to enable on the binding crate.
    #[serde(default)]
    pub features: Option<Vec<String>>,
    /// Additional Cargo dependencies for the generated Dart Rust bridge crate.
    #[serde(default)]
    #[schemars(with = "HashMap<String, serde_json::Value>")]
    pub extra_dependencies: HashMap<String, toml::Value>,
    /// Override the serde rename_all strategy for JSON field names (e.g. "camelCase", "snake_case").
    #[serde(default)]
    pub serde_rename_all: Option<String>,
    /// Per-field name remapping. Key is `TypeName.field_name`, value is the
    /// desired binding field name. Applied after automatic keyword escaping.
    #[serde(default)]
    pub rename_fields: HashMap<String, String>,
    /// Functions to exclude from Dart binding generation.
    #[serde(default)]
    pub exclude_functions: Vec<String>,
    /// Types to exclude from Dart binding generation.
    #[serde(default)]
    pub exclude_types: Vec<String>,
    /// Prefix wrapper for default tool invocations.
    #[serde(default)]
    pub run_wrapper: Option<String>,
    /// Extra paths to append to default lint commands.
    #[serde(default)]
    pub extra_lint_paths: Vec<String>,
    /// Override the core Cargo dependency name and path for the Dart binding crate.
    /// When set, the binding `Cargo.toml` depends on this crate (resolved as
    /// `../../../crates/<override>`) instead of the umbrella `[crate.name]`.
    /// Defaults to unset.
    #[serde(default)]
    pub core_crate_override: Option<String>,
    /// Keys to subtract from the merged `extra_dependencies` set for this
    /// language only.
    #[serde(default)]
    pub exclude_extra_dependencies: Vec<String>,
    /// Method names whose Rust bridge body should be emitted as `unimplemented!()`.
    ///
    /// Use this when a function's FFI signature (e.g. nested tuples containing
    /// `Vec<u8>`) cannot be represented across the FRB bridge at all. Consumers must
    /// list the method names explicitly — this field has no built-in defaults so the
    /// knob is library-agnostic.
    ///
    /// Example (`alef.toml`):
    /// ```toml
    /// [crates.dart]
    /// stub_methods = ["batch_extract_bytes", "batch_extract_bytes_sync"]
    /// ```
    #[serde(default)]
    pub stub_methods: Vec<String>,
    /// Per-target Cargo dependency overrides for the binding crate.
    ///
    /// When set, the emitted `Cargo.toml` wraps the base core dependency in a
    /// `[target.'cfg(not(<cfg>))'.dependencies]` section and adds a matching
    /// `[target.'cfg(<cfg>)'.dependencies]` block using `override_features`
    /// (and `default_features = false` when `override_default_features = false`).
    /// Required when the binding has to swap the feature set on a specific
    /// target triple, e.g. Android x86_64 dropping ORT-dependent features.
    ///
    /// Example (`alef.toml`):
    /// ```toml
    /// [[crates.dart.target_dep_overrides]]
    /// cfg = "all(target_os = \"android\", target_arch = \"x86_64\")"
    /// features = ["android-target"]
    /// default_features = false
    /// ```
    #[serde(default)]
    pub target_dep_overrides: Vec<DartTargetDepOverride>,
    /// Skip the `flutter_rust_bridge_codegen generate` post-build step.
    ///
    /// When `true`, alef omits the `RunCommand` that invokes
    /// `flutter_rust_bridge_codegen` during `alef all` / `alef generate`.
    /// File post-processors (sealed-variant rewriting, loader injection, etc.)
    /// are still executed; only the upstream codegen invocation is suppressed.
    ///
    /// Use this when `flutter_rust_bridge` is not installed on the host —
    /// e.g. in CI environments that regen binding scaffolding only, or on
    /// developer machines that have not installed the Flutter SDK.
    ///
    /// Equivalent CLI override: pass `--skip-frb` to `alef all` or
    /// `alef generate`, or set `ALEF_SKIP_COMMANDS=flutter_rust_bridge_codegen`.
    ///
    /// Default: `false` (FRB codegen runs as usual).
    #[serde(default)]
    pub skip_frb: bool,
    /// Feature names that should be declared as opt-in flags in the wrapper's
    /// `[features]` table but excluded from the `default = [...]` array.
    ///
    /// The named features are still emitted as forwarding entries
    /// (`<name> = ["<core>/<name>"]`) so `cargo build -p <crate>-dart --features <name>`
    /// continues to work on desktop targets. They are simply not auto-enabled
    /// by `cargo build` against default features.
    ///
    /// Use this to keep native cross-compile targets (iOS, Android NDK) green
    /// when a feature pulls in a system library (e.g. `libheif-sys` via `heic`)
    /// whose `build.rs` cannot satisfy `pkg-config` under cross-compilation.
    /// The target-conditional `[target.'cfg(...)'.dependencies]` block alone
    /// is insufficient because cargo unions feature sets across dep instances.
    #[serde(default)]
    pub excluded_default_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DartTargetDepOverride {
    /// Cargo `cfg(...)` predicate (without the `cfg(...)` wrapper). Example:
    /// `all(target_os = "android", target_arch = "x86_64")`.
    pub cfg: String,
    /// Features to enable on the core dependency for this target.
    #[serde(default)]
    pub features: Vec<String>,
    /// When false (default), emit `default-features = false` for this target.
    /// When true, allow the core dep's default features through.
    #[serde(default)]
    pub default_features: bool,
}
