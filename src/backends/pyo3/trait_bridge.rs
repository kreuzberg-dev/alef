//! PyO3-specific trait bridge code generation.
//!
//! Generates Rust wrapper structs that implement Rust traits by delegating
//! to Python objects via PyO3.

mod bridge_methods;
mod generator;
mod options_field;
mod registry;
mod visitor_bridge;

pub use crate::codegen::generators::trait_bridge::find_bridge_param;
pub use bridge_methods::gen_bridge_function;
pub use generator::Pyo3BridgeGenerator;
pub use options_field::gen_bridge_field_function;
pub use registry::{
    collect_bridge_clear_fns, collect_bridge_register_fns, collect_bridge_unregister_fns, trait_bridge_imports,
};

use crate::codegen::generators::trait_bridge::{BridgeOutput, TraitBridgeSpec, gen_bridge_all};
use crate::core::config::TraitBridgeConfig;
use crate::core::ir::{ApiSurface, TypeDef};
use std::collections::{HashMap, HashSet};
use visitor_bridge::gen_visitor_bridge;

pub fn gen_trait_bridge(
    trait_type: &TypeDef,
    bridge_cfg: &TraitBridgeConfig,
    core_import: &str,
    error_type: &str,
    error_constructor: &str,
    api: &ApiSurface,
) -> anyhow::Result<BridgeOutput> {
    // Build type name → rust_path lookup for qualifying Named types in signatures
    let type_paths: HashMap<String, String> = api
        .types
        .iter()
        .map(|t| (t.name.clone(), t.rust_path.replace('-', "_")))
        .chain(
            api.enums
                .iter()
                .map(|e| (e.name.clone(), e.rust_path.replace('-', "_"))),
        )
        // Include excluded types so trait methods referencing them (for example, `&HiddenDoc`)
        // are qualified with the full Rust path rather than emitting the bare type name.
        .chain(
            api.excluded_type_paths
                .iter()
                .map(|(name, path)| (name.clone(), path.replace('-', "_"))),
        )
        .collect();

    // Determine bridge pattern: visitor-style (all methods have defaults, no registry) vs
    // plugin-style (cached fields, registry, super-trait).
    let is_visitor_bridge = bridge_cfg.type_alias.is_some()
        && bridge_cfg.register_fn.is_none()
        && bridge_cfg.super_trait.is_none()
        && trait_type.methods.iter().all(|m| m.has_default_impl);

    if is_visitor_bridge {
        let trait_path = trait_type.rust_path.replace('-', "_");
        let struct_name = crate::codegen::generators::trait_bridge::bridge_wrapper_name("Py", bridge_cfg);
        let code = gen_visitor_bridge(
            trait_type,
            bridge_cfg,
            &struct_name,
            &trait_path,
            core_import,
            &type_paths,
            api,
        )?;
        Ok(BridgeOutput { imports: vec![], code })
    } else {
        // Use the IR-driven TraitBridgeGenerator infrastructure
        let generator = Pyo3BridgeGenerator {
            core_import: core_import.to_string(),
            type_paths: type_paths.clone(),
            error_type: error_type.to_string(),
        };
        let lifetime_type_names: HashSet<String> = api
            .types
            .iter()
            .filter(|t| t.has_lifetime_params)
            .map(|t| t.name.clone())
            .collect();
        let spec = TraitBridgeSpec {
            trait_def: trait_type,
            bridge_config: bridge_cfg,
            core_import,
            wrapper_prefix: "Py",
            type_paths,
            lifetime_type_names,
            error_type: error_type.to_string(),
            error_constructor: error_constructor.to_string(),
        };
        Ok(gen_bridge_all(&spec, &generator))
    }
}

/// Generate a visitor-style bridge: thin wrapper over `Py<PyAny>` where every trait method
/// tries to call the corresponding Python method, falling back to the default if absent.
///
/// This pattern is used for traits where:
/// - All methods have default implementations
/// - No registration function is needed (per-call construction via `type_alias`)

mod tests {
    #[test]
    fn visitor_bridge_uses_configured_context_and_result_metadata() {
        let (api, trait_type, bridge) = crate::codegen::visitor_context::test_support::neutral_visitor_fixture();
        let output = super::gen_trait_bridge(
            &trait_type,
            &bridge,
            "sample_core",
            "SampleError",
            "SampleError::Message { message: {msg} }",
            &api,
        )
        .expect("visitor bridge should generate");

        crate::codegen::visitor_context::test_support::assert_neutral_visitor_output(&output.code);
        assert!(output.code.contains("\"display_name\""));
    }
}
