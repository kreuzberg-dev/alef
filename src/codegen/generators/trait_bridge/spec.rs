use crate::core::config::TraitBridgeConfig;
use crate::core::ir::{MethodDef, TypeDef, TypeRef};
use heck::ToSnakeCase;
use std::collections::{HashMap, HashSet};

use super::bridge_wrapper_name;

pub struct TraitBridgeSpec<'a> {
    /// The trait definition from the IR.
    pub trait_def: &'a TypeDef,
    /// Bridge configuration from `alef.toml`.
    pub bridge_config: &'a TraitBridgeConfig,
    /// Core crate import path (e.g., `"sample_core"`).
    pub core_import: &'a str,
    /// Language-specific prefix for the wrapper type (e.g., `"Python"`, `"Js"`, `"Wasm"`).
    pub wrapper_prefix: &'a str,
    /// Map of type name → fully-qualified Rust path for qualifying `Named` types.
    pub type_paths: HashMap<String, String>,
    /// Set of core type names that carry a lifetime parameter (e.g. `NodeContext<'a>`).
    /// When non-empty, method signatures emit `TypeName<'_>` for these types so the
    /// generated `impl Trait for Wrapper` matches the trait definition exactly.
    pub lifetime_type_names: HashSet<String>,
    /// The crate's error type name (e.g., `"SampleCrateError"`). Defaults to `"Error"`.
    pub error_type: String,
    /// Error constructor pattern. `{msg}` is replaced with the message expression.
    pub error_constructor: String,
}

impl<'a> TraitBridgeSpec<'a> {
    /// Fully qualified error type path (e.g., `"sample_core::SampleCrateError"`).
    ///
    /// If `error_type` already looks fully-qualified (contains `::`) or is a generic
    /// type expression (contains `<`), it is returned as-is without prefixing
    /// `core_import`. This lets backends specify rich error types like
    /// `"Box<dyn std::error::Error + Send + Sync>"` directly.
    pub fn error_path(&self) -> String {
        if self.error_type.contains("::") || self.error_type.contains('<') {
            self.error_type.clone()
        } else {
            format!("{}::{}", self.core_import, self.error_type)
        }
    }

    /// Generate an error construction expression from a message expression.
    pub fn make_error(&self, msg_expr: &str) -> String {
        self.error_constructor.replace("{msg}", msg_expr)
    }

    /// Wrapper struct name: `{prefix}{TraitName}Bridge` (e.g., `PythonOcrBackendBridge`).
    pub fn wrapper_name(&self) -> String {
        bridge_wrapper_name(self.wrapper_prefix, self.bridge_config)
    }

    /// Snake-case version of the trait name (e.g., `"ocr_backend"`).
    pub fn trait_snake(&self) -> String {
        self.trait_def.name.to_snake_case()
    }

    /// Full Rust path to the trait (e.g., `sample_core::OcrBackend`).
    pub fn trait_path(&self) -> String {
        self.trait_def.rust_path.replace('-', "_")
    }

    /// Methods that are required (no default impl) — must be provided by the foreign object.
    pub fn required_methods(&self) -> Vec<&'a MethodDef> {
        self.trait_def.methods.iter().filter(|m| !m.has_default_impl).collect()
    }

    /// Methods that have a default impl — optional on the foreign object.
    pub fn optional_methods(&self) -> Vec<&'a MethodDef> {
        self.trait_def.methods.iter().filter(|m| m.has_default_impl).collect()
    }
}

/// Return visitor callback methods configured for visitor-style bridges.
///
/// A visitor callback is an own trait method whose return type is the configured
/// visitor result type and whose parameters include the configured context type.
pub fn visitor_callback_methods<'a>(trait_def: &'a TypeDef, bridge_config: &TraitBridgeConfig) -> Vec<&'a MethodDef> {
    trait_def
        .methods
        .iter()
        .filter(|method| is_visitor_callback_method(method, bridge_config))
        .collect()
}

fn is_visitor_callback_method(method: &MethodDef, bridge_config: &TraitBridgeConfig) -> bool {
    if method.trait_source.is_some() {
        return false;
    }

    let Some(result_type) = bridge_config.result_type.as_deref() else {
        return false;
    };
    let Some(context_type) = bridge_config.context_type.as_deref() else {
        return false;
    };

    type_ref_matches_name(&method.return_type, result_type)
        && method
            .params
            .iter()
            .any(|param| type_ref_matches_name(&param.ty, context_type))
}

fn type_ref_matches_name(ty: &TypeRef, name: &str) -> bool {
    match ty {
        TypeRef::Named(type_name) => type_name == name,
        TypeRef::Optional(inner) => type_ref_matches_name(inner, name),
        _ => false,
    }
}
