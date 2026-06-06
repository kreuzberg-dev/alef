/// Emit a Kotlin test backend stub.
///
/// Kotlin (JVM) is not currently in e2e.languages, so trait_bridge e2e tests
/// are not generated for it. Return unimplemented gracefully.
pub fn emit_test_backend(
    _trait_bridge: &crate::core::config::TraitBridgeConfig,
    _methods: &[&crate::core::ir::MethodDef],
    _fixture: &crate::e2e::fixture::Fixture,
) -> crate::e2e::codegen::TestBackendEmission {
    crate::e2e::codegen::TestBackendEmission::unimplemented("Kotlin (JVM) trait_bridge e2e tests not yet implemented")
}
