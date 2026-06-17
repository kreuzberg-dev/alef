//! Dart-specific e2e generator tests.

use super::stubs::emit_test_backend;
use crate::core::config::TraitBridgeConfig;
use crate::core::ir::{MethodDef, PrimitiveType, TypeRef};
use crate::e2e::fixture::Fixture;

fn make_trait_bridge(trait_name: &str) -> TraitBridgeConfig {
    TraitBridgeConfig {
        trait_name: trait_name.to_string(),
        super_trait: Some("Plugin".to_string()),
        register_fn: Some(format!("register_{}", trait_name.to_lowercase())),
        ..Default::default()
    }
}

fn make_method(name: &str, required: bool) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        params: vec![],
        return_type: TypeRef::Primitive(PrimitiveType::Bool),
        is_async: true,
        is_static: false,
        error_type: None,
        doc: String::new(),
        receiver: Some(crate::core::ir::ReceiverKind::Ref),
        sanitized: false,
        trait_source: None,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        has_default_impl: !required,
        binding_excluded: false,
        binding_exclusion_reason: None,
        version: Default::default(),
    }
}

fn make_fixture(id: &str) -> Fixture {
    Fixture {
        id: id.to_string(),
        category: None,
        description: "test".to_string(),
        tags: vec![],
        skip: None,
        env: None,
        setup: Vec::new(),
        call: None,
        input: serde_json::Value::Null,
        mock_response: None,
        source: String::new(),
        http: None,
        assertions: vec![],
        visitor: None,
        args: vec![],
        assertion_recipes: vec![],
    }
}

/// Verify that no sample_core-domain names leak into the generated output when
/// the trait bridge is configured for a synthetic `TestTrait` in `testlib`.
#[test]
fn dart_stub_contains_no_sample_crate_domain_names() {
    let bridge = make_trait_bridge("TestTrait");
    let required_method = make_method("doWork", true);
    let methods = [&required_method];
    let fixture = make_fixture("my_test_fixture");

    let emission = emit_test_backend(&bridge, &methods, &fixture, &[]);

    let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

    assert!(
        !output.contains("SampleCrate"),
        "must not contain literal 'SampleCrate', got:\n{output}"
    );
    assert!(
        !output.contains("sample_crate::"),
        "must not contain 'sample_crate::', got:\n{output}"
    );
    assert!(
        !output.contains("SampleCrateBridge"),
        "must not contain 'SampleCrateBridge', got:\n{output}"
    );
    assert!(
        output.contains("TestStubMyTestFixture"),
        "class name must be derived from fixture id, got:\n{output}"
    );
    assert!(
        output.contains("extends TestTrait"),
        "class must extend the configured trait class, got:\n{output}"
    );
    assert!(
        output.contains("doWork"),
        "required method must be emitted, got:\n{output}"
    );
}

fn make_param(name: &str, ty: TypeRef) -> crate::core::ir::ParamDef {
    crate::core::ir::ParamDef {
        name: name.to_string(),
        ty,
        optional: false,
        default: None,
        sanitized: false,
        typed_default: None,
        is_ref: false,
        is_mut: false,
        newtype_wrapper: None,
        original_type: None,
        map_is_ahash: false,
        map_key_is_cow: false,
        vec_inner_is_ref: false,
        map_is_btree: false,
        core_wrapper: crate::core::ir::CoreWrapper::None,
    }
}

fn make_method_with_params(name: &str, required: bool) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        params: vec![
            make_param("content", TypeRef::Bytes),
            make_param("mime_type", TypeRef::String),
        ],
        return_type: TypeRef::Named("SampleResult".to_string()),
        is_async: true,
        is_static: false,
        error_type: Some("anyhow::Error".to_string()),
        doc: String::new(),
        receiver: Some(crate::core::ir::ReceiverKind::Ref),
        sanitized: false,
        trait_source: None,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        has_default_impl: !required,
        binding_excluded: false,
        binding_exclusion_reason: None,
        version: Default::default(),
    }
}

/// Verify params use concrete Dart types (not `dynamic`) and no @override annotation.
#[test]
fn dart_stub_uses_typed_params_not_dynamic() {
    let bridge = make_trait_bridge("TestTrait");
    let required_method = make_method_with_params("extract", true);
    let methods = [&required_method];
    let fixture = make_fixture("my_test_fixture");

    let emission = emit_test_backend(&bridge, &methods, &fixture, &[]);
    let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

    assert!(
        !output.contains("dynamic content"),
        "param must not use `dynamic`, got:\n{output}"
    );
    assert!(
        output.contains("Uint8List content"),
        "bytes param must map to Uint8List, got:\n{output}"
    );
    assert!(
        output.contains("String mimeType"),
        "string param must map to String, got:\n{output}"
    );
    assert!(
        output.contains("Future<SampleResult>"),
        "return type must be concrete not dynamic, got:\n{output}"
    );
    assert!(
        !output.contains("@override"),
        "local class members must not use @override annotation, got:\n{output}"
    );
}

/// Verify that `fixture.input["name"]` is used as the plugin name when present.
#[test]
fn dart_stub_uses_fixture_input_name_for_plugin_name() {
    let bridge = make_trait_bridge("TestTrait");
    let required_method = make_method("doWork", true);
    let methods = [&required_method];
    let mut fixture = make_fixture("my_fixture_id");
    fixture.input = serde_json::json!({ "name": "my-backend-name" });

    let emission = emit_test_backend(&bridge, &methods, &fixture, &[]);
    let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

    assert!(
        output.contains("'my-backend-name'"),
        "plugin name must come from fixture.input.name, got:\n{output}"
    );
    assert!(
        !output.contains("my_fixture_id"),
        "fixture id must not appear as plugin name when input.name is set, got:\n{output}"
    );
}

/// Verify that _setEnv helper forces overwrite=1 and checks return code.
/// Regression test for the bug where setenv(..., 0) silently no-ops when the
/// env var is already set, causing SAMPLE_ALLOW_PRIVATE_NETWORK to be
/// invisible to Rust FFI dylib in dart e2e tests.
#[test]
fn dart_emit_setenv_forces_overwrite_and_checks_return_code() {
    use crate::e2e::config::E2eConfig;
    use std::collections::HashMap;

    // Create a minimal E2eConfig with an env var to trigger _setEnv emission.
    let mut env = HashMap::new();
    env.insert("SAMPLE_ALLOW_PRIVATE_NETWORK".to_string(), "true".to_string());

    let e2e_config = E2eConfig {
        env,
        ..Default::default()
    };

    // Build a minimal test file just to check the _setEnv helper.
    // We'll use a dummy fixture and configuration.
    let fixture = make_fixture("test_fixture");
    let _bridge = make_trait_bridge("TestTrait");
    let config = crate::core::config::ResolvedCrateConfig::default();
    let type_defs = [];
    let enums = [];
    let adapters = [];
    let dart_first_class_map = crate::e2e::field_access::DartFirstClassMap::default();

    let output = super::test_file::render_test_file(
        "smoke",
        &[&fixture],
        &e2e_config,
        "dart",
        "samplecli",
        "RustLib",
        "RustLibBridge",
        &dart_first_class_map,
        &adapters,
        &config,
        &type_defs,
        &enums,
    );

    // Verify that the generated setenv call uses overwrite=1 (third argument).
    assert!(
        output.contains("setenv(keyPtr, valuePtr, 1)"),
        "setenv must use overwrite=1, got:\n{output}"
    );

    // Verify that the old buggy pattern is NOT in the output.
    assert!(
        !output.contains("setenv(keyPtr, valuePtr, 0)"),
        "setenv must NOT use overwrite=0, got:\n{output}"
    );

    // Verify that return code is captured and checked.
    assert!(
        output.contains("final result = setenv(keyPtr, valuePtr, 1)"),
        "setenv result must be captured, got:\n{output}"
    );

    assert!(
        output.contains("if (result != 0)"),
        "return code must be checked with 'if (result != 0)', got:\n{output}"
    );

    assert!(
        output.contains("throw StateError"),
        "must throw StateError on non-zero return code, got:\n{output}"
    );
}
