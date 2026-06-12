//! NAPI-RS-specific trait bridge code generation.
//!
//! Generates Rust wrapper structs that implement Rust traits by delegating
//! to JavaScript objects via NAPI-RS.

mod bridge;
mod bridge_functions;
mod bridge_generator;
mod options_field_bridge;
mod typescript_bridge;
mod visitor_bridge;

use crate::core::config::TraitBridgeConfig;

pub use bridge::gen_trait_bridge;
pub use bridge_functions::gen_bridge_function;
pub use bridge_generator::NapiBridgeGenerator;
pub use options_field_bridge::gen_options_field_bridge_function;
pub use typescript_bridge::gen_typescript_trait_bridge_files;

/// Find the first parameter index and bridge config where the parameter's named type
/// matches a trait bridge's `type_alias`.
///
/// Returns `None` when no bridge applies.
pub use crate::codegen::generators::trait_bridge::find_bridge_param;

/// Find a bridge config that uses options_field binding and a parameter of the options_type.
/// This complements find_bridge_param which only handles FunctionParam bindings.
pub fn find_options_field_binding<'a>(
    func: &crate::core::ir::FunctionDef,
    bridges: &'a [TraitBridgeConfig],
) -> Option<(usize, &'a TraitBridgeConfig)> {
    for bridge in bridges {
        if bridge.bind_via != crate::core::config::BridgeBinding::OptionsField {
            continue;
        }
        if let Some(options_type) = &bridge.options_type {
            for (idx, param) in func.params.iter().enumerate() {
                let matches = match &param.ty {
                    crate::core::ir::TypeRef::Named(n) => n == options_type,
                    crate::core::ir::TypeRef::Optional(inner) => {
                        if let crate::core::ir::TypeRef::Named(n) = inner.as_ref() {
                            n == options_type
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if matches {
                    return Some((idx, bridge));
                }
            }
        }
    }
    None
}

#[cfg(test)]
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
        assert!(output.code.contains("displayName"));
    }
}
