//! E2e test code generation trait and language dispatch.

pub mod c;
pub mod csharp;
pub mod elixir;
pub mod go;
pub mod java;
pub mod php;
pub mod python;
pub mod r;
pub mod ruby;
pub mod rust;
pub mod typescript;
pub mod wasm;

use crate::config::E2eConfig;
use crate::fixture::FixtureGroup;
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;

/// Convert a JSON value's object keys from camelCase to snake_case recursively.
///
/// Used when serializing fixture options for FFI-based languages (Rust, C, Java)
/// where the receiving Rust type uses default serde (snake_case) without `rename_all`.
pub(crate) fn normalize_json_keys_to_snake_case(value: &serde_json::Value) -> serde_json::Value {
    use heck::ToSnakeCase;
    match value {
        serde_json::Value::Object(obj) => {
            let new_obj: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(k, v)| (k.to_snake_case(), normalize_json_keys_to_snake_case(v)))
                .collect();
            serde_json::Value::Object(new_obj)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_json_keys_to_snake_case).collect())
        }
        other => other.clone(),
    }
}

/// Trait for per-language e2e test code generation.
pub trait E2eCodegen: Send + Sync {
    /// Generate all e2e test project files for this language.
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>>;

    /// Language name for display and directory naming.
    fn language_name(&self) -> &'static str;
}

/// Get all available e2e code generators.
pub fn all_generators() -> Vec<Box<dyn E2eCodegen>> {
    vec![
        Box::new(rust::RustE2eCodegen),
        Box::new(python::PythonE2eCodegen),
        Box::new(typescript::TypeScriptCodegen),
        Box::new(go::GoCodegen),
        Box::new(java::JavaCodegen),
        Box::new(csharp::CSharpCodegen),
        Box::new(php::PhpCodegen),
        Box::new(ruby::RubyCodegen),
        Box::new(elixir::ElixirCodegen),
        Box::new(r::RCodegen),
        Box::new(wasm::WasmCodegen),
        Box::new(c::CCodegen),
    ]
}

/// Get e2e code generators for specific language names.
pub fn generators_for(languages: &[String]) -> Vec<Box<dyn E2eCodegen>> {
    all_generators()
        .into_iter()
        .filter(|g| languages.iter().any(|l| l == g.language_name()))
        .collect()
}
