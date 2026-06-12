use crate::core::config::TraitBridgeConfig;

pub fn collect_bridge_register_fns(configs: &[TraitBridgeConfig]) -> Vec<String> {
    configs.iter().filter_map(|c| c.register_fn.clone()).collect()
}

/// Collect unregistration function names for api.py pass-through wrappers.
///
/// Only bridges that define an `unregister_fn` are included.
pub fn collect_bridge_unregister_fns(configs: &[TraitBridgeConfig]) -> Vec<String> {
    configs.iter().filter_map(|c| c.unregister_fn.clone()).collect()
}

/// Collect clear function names for api.py pass-through wrappers.
///
/// Only bridges that define a `clear_fn` are included.
pub fn collect_bridge_clear_fns(configs: &[TraitBridgeConfig]) -> Vec<String> {
    configs.iter().filter_map(|c| c.clear_fn.clone()).collect()
}

/// Imports needed by trait bridge generated code.
pub fn trait_bridge_imports(configs: &[TraitBridgeConfig]) -> Vec<&'static str> {
    if configs.is_empty() {
        return vec![];
    }
    vec![
        "use async_trait::async_trait;",
        "use pyo3::prelude::*;",
        "use std::sync::Arc;",
    ]
}
