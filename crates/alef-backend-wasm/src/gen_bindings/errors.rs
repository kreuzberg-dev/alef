//! WASM error type generation.
//!
//! WASM error conversion is handled by `alef_codegen::error_gen::gen_wasm_error_converter`.
//! This module is a thin re-export shim so the gen_bindings structure is consistent
//! across all backends.

/// Generate a WASM error converter for a single error type.
///
/// Delegates to `alef_codegen::error_gen::gen_wasm_error_converter`.
pub(super) fn gen_error_converter(error: &alef_core::ir::ErrorDef, core_import: &str) -> String {
    alef_codegen::error_gen::gen_wasm_error_converter(error, core_import)
}
