use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Configuration for the JNI Rust shim crate emitter (`alef-backend-jni`).
///
/// Most identifiers are derived from the paired `[crates.kotlin_android]`
/// section (package, features, etc.).  Set `crate_dir` when the JNI crate
/// directory should differ from the default `<config.name>-jni/` — for
/// example when `config.name` carries a language-specific suffix (e.g.
/// `"sample-markdown-rs"`) but you want the JNI crate to live at
/// `crates/sample-markdown-jni/` to match every other binding crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct JniConfig {
    /// Override the JNI crate directory name.
    ///
    /// When set, the JNI crate is placed at `crates/<crate_dir>-jni/` and the
    /// `[package] name` in the generated `Cargo.toml` is `<crate_dir>-jni`.
    /// When unset, both derive from `config.name` (the default, which matches
    /// the behavior used by `alef-backend-jni::gen_shims::jni_output_path`).
    #[serde(default)]
    pub crate_dir: Option<String>,
}
