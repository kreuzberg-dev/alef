/// Emit a Kotlin Android test backend stub class for a trait bridge.
///
/// Generates a class implementing `I{TraitName}`. Required methods are overridden
/// with Kotlin-idiomatic defaults. Suspend (async) methods use `suspend fun`.
/// The `name()` function is emitted when a Plugin super-trait is configured.
/// Registration uses `{TraitName}Bridge.register(stub)` (the static object pattern).
pub fn emit_test_backend(
    trait_bridge: &crate::core::config::TraitBridgeConfig,
    methods: &[&crate::core::ir::MethodDef],
    fixture: &crate::e2e::fixture::Fixture,
) -> crate::e2e::codegen::TestBackendEmission {
    use crate::backends::kotlin::type_map::KotlinMapper;
    use crate::codegen::defaults::language_defaults;
    use crate::codegen::type_mapper::TypeMapper as _;
    use heck::{ToLowerCamelCase, ToUpperCamelCase};
    use std::fmt::Write as _;

    let pascal_id = fixture.id.to_upper_camel_case();
    let class_name = format!("TestStub{pascal_id}");
    // Kotlin Android uses I{TraitName} as the interface.
    let interface_name = format!("I{}", trait_bridge.trait_name);
    // Use the canonical naming helper so both production and e2e emit the same bridge object name.
    let bridge_object = crate::backends::kotlin_android::naming::bridge_object_name(&trait_bridge.trait_name);

    // Prefer the fixture's input "name" field (e.g. "test-extractor") over the
    // fixture id, which is an internal snake_case identifier, not a backend name.
    let plugin_name = fixture
        .input
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&fixture.id)
        .to_string();

    let defaults = language_defaults("kotlin_android");
    let mapper = KotlinMapper;

    // Collect all type imports needed by method parameters and return types.
    // Exclude Kotlin built-in types and the interface itself (which is always imported).
    let mut type_imports = std::collections::HashSet::new();
    type_imports.insert(interface_name.clone());

    const KOTLIN_BUILTINS: &[&str] = &[
        "String",
        "Int",
        "Long",
        "Short",
        "Byte",
        "Boolean",
        "Char",
        "Float",
        "Double",
        "Unit",
        "Any",
        "Nothing",
        "List",
        "Map",
        "Set",
        "ByteArray",
    ];

    for method in methods {
        // Collect parameter types.
        for param in &method.params {
            if let crate::core::ir::TypeRef::Named(name) = &param.ty {
                if !KOTLIN_BUILTINS.contains(&name.as_str()) {
                    type_imports.insert(name.clone());
                }
            }
        }
        // Collect return type.
        if let crate::core::ir::TypeRef::Named(name) = &method.return_type {
            if !KOTLIN_BUILTINS.contains(&name.as_str()) {
                type_imports.insert(name.clone());
            }
        }
    }

    let mut setup = String::new();
    let _ = writeln!(setup, "class {class_name} : {interface_name} {{");

    // Plugin super-trait `name()` function.
    let mut emitted_methods = std::collections::HashSet::new();
    if trait_bridge.super_trait.is_some() {
        let _ = writeln!(setup, "    override fun name(): String = \"{plugin_name}\"");
        emitted_methods.insert("name".to_string());
    }

    // Emit all methods to ensure test stubs are concrete and non-abstract.
    // Even methods marked with has_default_impl=true must be overridden in test stubs
    // to ensure the stub class is not abstract and can be instantiated. The Kotlin
    // interface may declare abstract methods without defaults that the Rust metadata
    // incorrectly marks as having defaults.
    for method in methods {
        // Skip if already emitted (e.g., super-trait name method).
        if emitted_methods.contains(&method.name) {
            continue;
        }
        let method_name = method.name.to_lower_camel_case();

        // Build parameter list with concrete Kotlin types.
        let params: Vec<String> = method
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name.to_lower_camel_case(), mapper.map_type(&p.ty)))
            .collect();
        let params_str = params.join(", ");

        let return_type = mapper.map_type(&method.return_type);

        // For Unit return types, use block syntax {} instead of assignment.
        // For other types, use expression syntax = default_val.
        let is_unit = matches!(&method.return_type, crate::core::ir::TypeRef::Unit);

        if is_unit {
            if method.is_async {
                let _ = writeln!(
                    setup,
                    "    override suspend fun {method_name}({params_str}): {return_type} {{}}"
                );
            } else {
                let _ = writeln!(
                    setup,
                    "    override fun {method_name}({params_str}): {return_type} {{}}"
                );
            }
        } else {
            // Try to extract default from fixture.input.backend first.
            let default_val = super::enum_fixtures::extract_kotlin_android_fixture_default(&method.name, fixture)
                .unwrap_or_else(|| {
                    // Fall back to language defaults.
                    if let crate::core::ir::TypeRef::Named(name) = &method.return_type {
                        match name.as_str() {
                            "ProcessingStage" => "ProcessingStage.EARLY".to_string(),
                            "OcrBackendType" => "OcrBackendType.TESSERACT".to_string(),
                            "OutputFormat" => "OutputFormat.TEXT".to_string(),
                            "ChunkingStrategy" => "ChunkingStrategy.NAIVE".to_string(),
                            "EmbeddingModelType" => "EmbeddingModelType.UNKNOWN".to_string(),
                            _ => defaults.emit_default(&method.return_type),
                        }
                    } else {
                        defaults.emit_default(&method.return_type)
                    }
                });

            if method.is_async {
                let _ = writeln!(
                    setup,
                    "    override suspend fun {method_name}({params_str}): {return_type} = {default_val}"
                );
            } else {
                let _ = writeln!(
                    setup,
                    "    override fun {method_name}({params_str}): {return_type} = {default_val}"
                );
            }
        }
        emitted_methods.insert(method.name.clone());
    }

    let _ = writeln!(setup, "}}");

    // Registration: `{TraitName}Bridge.register(stub)` — static object pattern.
    let arg_expr = format!("{class_name}()");
    // Emit a registration comment in the setup block so the caller can see the bridge object.
    let _ = writeln!(setup, "// register via: {bridge_object}.register({class_name}())");

    let mut sorted_imports: Vec<String> = type_imports.into_iter().collect();
    sorted_imports.sort();

    crate::e2e::codegen::TestBackendEmission {
        setup_block: setup,
        arg_expr,
        type_imports: sorted_imports,
        teardown_block: String::new(),
    }
}

#[cfg(test)]
mod test_backend_tests {
    use super::emit_test_backend;
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
            is_async: false,
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
    fn kotlin_android_stub_contains_no_sample_crate_domain_names() {
        let bridge = make_trait_bridge("TestTrait");
        let required_method = make_method("process_item", true);
        let methods = [&required_method];
        let fixture = make_fixture("my_test_fixture");

        let emission = emit_test_backend(&bridge, &methods, &fixture);

        let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

        assert!(
            !output.contains("SampleCrate"),
            "must not contain literal 'SampleCrate', got:\n{output}"
        );
        assert!(
            !output.contains("sample_crate::"),
            "must not contain 'sample_crate::', got:\n{output}"
        );
        // The bridge object is "TestTraitBridge" not "SampleCrateBridge"
        assert!(
            !output.contains("SampleCrateBridge"),
            "must not contain 'SampleCrateBridge', got:\n{output}"
        );
        assert!(
            output.contains("TestStubMyTestFixture"),
            "class name must be derived from fixture id, got:\n{output}"
        );
        assert!(
            output.contains("ITestTrait"),
            "class must implement interface derived from trait name, got:\n{output}"
        );
        assert!(
            output.contains("TestTraitBridge"),
            "setup block must reference the bridge object derived from trait name, got:\n{output}"
        );
        assert!(
            output.contains("processItem"),
            "required method must be emitted in camelCase, got:\n{output}"
        );
    }

    fn make_param(name: &str, ty: crate::core::ir::TypeRef) -> crate::core::ir::ParamDef {
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
            return_type: TypeRef::Named("ProcessingResult".to_string()),
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

    /// Verify params use concrete Kotlin types (not `Any`) and return type is concrete.
    #[test]
    fn kotlin_android_stub_uses_typed_params_not_any() {
        let bridge = make_trait_bridge("TestTrait");
        let required_method = make_method_with_params("extractBytes", true);
        let methods = [&required_method];
        let fixture = make_fixture("my_test_fixture");

        let emission = emit_test_backend(&bridge, &methods, &fixture);
        let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

        assert!(
            !output.contains(": Any"),
            "param type must not be `Any`, got:\n{output}"
        );
        assert!(
            output.contains("content: ByteArray"),
            "bytes param must map to ByteArray in Kotlin, got:\n{output}"
        );
        assert!(
            output.contains("mimeType: String"),
            "string param must map to String in Kotlin, got:\n{output}"
        );
        assert!(
            output.contains("): ProcessingResult"),
            "return type must be concrete not Any, got:\n{output}"
        );
    }

    /// Verify that `fixture.input["name"]` is used as the plugin name when present.
    #[test]
    fn kotlin_android_stub_uses_fixture_input_name_for_plugin_name() {
        let bridge = make_trait_bridge("TestTrait");
        let required_method = make_method("process_item", true);
        let methods = [&required_method];
        let mut fixture = make_fixture("my_fixture_id");
        fixture.input = serde_json::json!({ "name": "my-backend-name" });

        let emission = emit_test_backend(&bridge, &methods, &fixture);
        let output = format!("{}\n{}", emission.setup_block, emission.arg_expr);

        assert!(
            output.contains("\"my-backend-name\""),
            "plugin name must come from fixture.input.name, got:\n{output}"
        );
    }
}
