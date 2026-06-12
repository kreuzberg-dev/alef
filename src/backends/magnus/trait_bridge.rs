//! Ruby (Magnus) specific trait bridge code generation.
//!
//! Generates Rust wrapper structs that implement Rust traits by delegating
//! to Ruby objects via Magnus `respond_to` checks and `funcall`.

mod bridge_functions;
mod bridge_generator;
mod options_field;
mod visitor_bridge;

pub use crate::codegen::generators::trait_bridge::find_bridge_param;
pub use bridge_functions::gen_bridge_function;
pub use bridge_generator::gen_trait_bridge;
pub use options_field::{find_options_field_binding, gen_options_field_bridge_function};

#[cfg(test)]
mod tests {
    #[test]
    fn visitor_bridge_uses_configured_context_and_result_metadata() {
        let (api, trait_type, bridge) = crate::codegen::visitor_context::test_support::neutral_visitor_fixture();
        let code = super::gen_trait_bridge(
            &trait_type,
            &bridge,
            "sample_core",
            "SampleError",
            "SampleError::Message { message: {msg} }",
            &api,
        )
        .expect("visitor bridge should generate");

        crate::codegen::visitor_context::test_support::assert_neutral_visitor_output(&code);
        assert!(code.contains("\"display_name\""));
    }
}
