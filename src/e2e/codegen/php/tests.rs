#![allow(clippy::module_name_repetitions)]

#[cfg(test)]
mod trait_bridge_tests {
    use crate::core::config::TraitBridgeConfig;
    use crate::core::ir::{MethodDef, ParamDef, TypeRef};
    use crate::e2e::fixture::Fixture;

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

    fn make_param(name: &str, ty: TypeRef) -> ParamDef {
        ParamDef {
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

    fn make_method(name: &str, params: Vec<(&str, TypeRef)>, ret: TypeRef, is_async: bool) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            params: params.into_iter().map(|(n, ty)| make_param(n, ty)).collect(),
            return_type: ret,
            is_async,
            is_static: false,
            error_type: None,
            doc: String::new(),
            receiver: Some(crate::core::ir::ReceiverKind::Ref),
            sanitized: false,
            trait_source: None,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
            has_default_impl: false,
            binding_excluded: false,
            binding_exclusion_reason: None,
            version: Default::default(),
        }
    }

    /// Genericity test: a synthetic TestTrait with one sync method and Plugin super-trait
    /// must not reference any sample_core-domain names in setup_block or arg_expr.
    #[test]
    fn test_backend_emission_is_generic() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "TestTrait".to_string(),
            super_trait: Some("SomeSuperTrait".to_string()),
            register_fn: Some("register_test_trait".to_string()),
            ..TraitBridgeConfig::default()
        };

        let do_thing = make_method(
            "do_thing",
            vec![("x", TypeRef::Primitive(crate::core::ir::PrimitiveType::I32))],
            TypeRef::String,
            false,
        );

        let fixture = make_fixture("my_test_fixture");
        let methods = vec![&do_thing];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        // setup_block must not reference any sample_core-domain trait or method names.
        assert!(
            !emission.setup_block.contains("OcrBackend"),
            "setup_block must not hardcode domain trait names, got:\n{}",
            emission.setup_block
        );
        assert!(
            !emission.setup_block.contains("process_image"),
            "setup_block must not hardcode domain method names, got:\n{}",
            emission.setup_block
        );
        // Must emit the method name verbatim from MethodDef (PHP snake_case).
        assert!(
            emission.setup_block.contains("do_thing"),
            "setup_block must contain the PHP snake_case method name 'do_thing', got:\n{}",
            emission.setup_block
        );
        // Must emit Plugin name method when super_trait is set.
        assert!(
            emission.setup_block.contains("name"),
            "setup_block must emit 'name' for super_trait, got:\n{}",
            emission.setup_block
        );
        // arg_expr is the anonymous class variable.
        assert_eq!(
            emission.arg_expr, "$stub",
            "arg_expr must be '$stub', got: {}",
            emission.arg_expr
        );
    }

    /// Named return types must emit `'{}'` (JSON-safe empty-object string), not
    /// a constructor call that would reference an undefined class.
    #[test]
    fn test_backend_named_return_emits_json_string() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "DocumentExtractor".to_string(),
            super_trait: Some("Plugin".to_string()),
            register_fn: Some("register_document_extractor".to_string()),
            ..TraitBridgeConfig::default()
        };

        let extract_bytes = make_method(
            "extract_bytes",
            vec![("content", TypeRef::Bytes), ("mime_type", TypeRef::String)],
            TypeRef::Named("InternalRecord".to_string()),
            false,
        );

        let fixture = make_fixture("register_document_extractor_trait_bridge");
        let methods = vec![&extract_bytes];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        assert!(
            emission.setup_block.contains("'{}'"),
            "Named return type must emit '{{}}' not a constructor call, got:\n{}",
            emission.setup_block
        );
        assert!(
            !emission.setup_block.contains("new InternalRecord"),
            "setup_block must not reference undefined type InternalRecord, got:\n{}",
            emission.setup_block
        );
    }

    /// Backend name is extracted from fixture.input, not fixture.id.
    #[test]
    fn test_backend_name_from_input() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "DocumentExtractor".to_string(),
            super_trait: Some("Plugin".to_string()),
            register_fn: Some("register_document_extractor".to_string()),
            ..TraitBridgeConfig::default()
        };

        let extract_bytes = make_method(
            "extract_bytes",
            vec![("content", TypeRef::Bytes)],
            TypeRef::Named("InternalRecord".to_string()),
            false,
        );

        let mut fixture = make_fixture("register_document_extractor_trait_bridge");
        fixture.input = serde_json::json!({
            "extractor": { "type": "test", "name": "test-extractor" }
        });

        let methods = vec![&extract_bytes];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        assert!(
            emission.setup_block.contains("test-extractor"),
            "setup_block must use input-derived name 'test-extractor', got:\n{}",
            emission.setup_block
        );
        assert!(
            !emission
                .setup_block
                .contains("register_document_extractor_trait_bridge"),
            "setup_block must not use fixture id as name, got:\n{}",
            emission.setup_block
        );
    }

    /// Snapshot: verify exact setup_block shape for a DocumentExtractor-like bridge.
    #[test]
    fn test_backend_snapshot() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "DocumentExtractor".to_string(),
            super_trait: Some("Plugin".to_string()),
            register_fn: Some("register_document_extractor".to_string()),
            ..TraitBridgeConfig::default()
        };

        let extract_bytes = make_method(
            "extract_bytes",
            vec![
                ("content", TypeRef::Bytes),
                ("mime_type", TypeRef::String),
                ("config", TypeRef::Named("ParseConfig".to_string())),
            ],
            TypeRef::Named("InternalRecord".to_string()),
            false,
        );

        let mut fixture = make_fixture("register_document_extractor_trait_bridge");
        fixture.input = serde_json::json!({
            "extractor": { "type": "test", "name": "test-extractor" }
        });

        let methods = vec![&extract_bytes];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        let expected_setup = concat!(
            "$stub = new class implements DocumentExtractor {\n",
            "    public function name(): string { return 'test-extractor'; }\n",
            "    public function extract_bytes($content, $mime_type, $config): mixed { return '{}'; }\n",
            "};\n",
        );
        assert_eq!(emission.setup_block, expected_setup, "setup_block snapshot mismatch");
        assert_eq!(emission.arg_expr, "$stub");
    }

    /// Verify that test stubs include both direct trait methods and super-trait methods.
    #[test]
    fn test_backend_includes_super_trait_methods() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "DocumentExtractor".to_string(),
            super_trait: Some("crate::plugin::Plugin".to_string()),
            register_fn: Some("register_document_extractor".to_string()),
            ..TraitBridgeConfig::default()
        };

        // Simulate a direct trait method.
        let extract_bytes = make_method(
            "extract_bytes",
            vec![("content", TypeRef::Bytes), ("mime_type", TypeRef::String)],
            TypeRef::Named("ProcessingResult".to_string()),
            false,
        );

        // Simulate super-trait methods (Plugin trait).
        let name_method = make_method("name", vec![], TypeRef::String, false);
        let priority_method = make_method(
            "priority",
            vec![],
            TypeRef::Primitive(crate::core::ir::PrimitiveType::U32),
            false,
        );

        let mut fixture = make_fixture("test_super_trait_methods");
        fixture.input = serde_json::json!({
            "extractor": { "type": "test", "name": "my-extractor" }
        });

        // Pass both direct trait methods and super-trait methods.
        let methods = vec![&extract_bytes, &name_method, &priority_method];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        // Verify the setup_block includes all required methods.
        assert!(
            emission.setup_block.contains("public function name()"),
            "setup_block must include name() from super-trait (Plugin)"
        );
        assert!(
            emission.setup_block.contains("public function priority()"),
            "setup_block must include priority() from super-trait (Plugin)"
        );
        assert!(
            emission.setup_block.contains("public function extract_bytes("),
            "setup_block must include extract_bytes() from direct trait (DocumentExtractor)"
        );
        assert!(
            emission.setup_block.contains("my-extractor"),
            "setup_block must use input-derived name"
        );
    }

    /// Verify that name() is not duplicated when super_trait is set and name is in methods list.
    /// This test prevents the PHP fatal "Cannot redeclare ...::name()" bug.
    #[test]
    fn test_backend_no_duplicate_name_with_super_trait() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "OcrBackend".to_string(),
            super_trait: Some("Plugin".to_string()),
            register_fn: Some("register_ocr_backend".to_string()),
            ..TraitBridgeConfig::default()
        };

        // Direct method.
        let process_image = make_method(
            "process_image",
            vec![("image", TypeRef::Bytes)],
            TypeRef::Named("OcrResult".to_string()),
            false,
        );

        // Super-trait methods: name() and priority().
        let name_method = make_method("name", vec![], TypeRef::String, false);
        let priority_method = make_method(
            "priority",
            vec![],
            TypeRef::Primitive(crate::core::ir::PrimitiveType::U32),
            false,
        );

        let mut fixture = make_fixture("test_no_duplicate_name");
        fixture.input = serde_json::json!({
            "ocr_backend": { "type": "test", "name": "test-ocr" }
        });

        let methods = vec![&process_image, &name_method, &priority_method];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        // Count occurrences of "function name()" to ensure exactly one.
        let name_count = emission.setup_block.matches("function name()").count();
        assert_eq!(
            name_count, 1,
            "name() must appear exactly once in setup_block, found {} occurrences:\n{}",
            name_count, emission.setup_block
        );

        // Verify all methods are present (name deduplicated, not missing).
        assert!(
            emission
                .setup_block
                .contains("public function name(): string { return 'test-ocr'; }"),
            "name() must be hardcoded with backend name, got:\n{}",
            emission.setup_block
        );
        assert!(
            emission.setup_block.contains("public function priority()"),
            "priority() must be present, got:\n{}",
            emission.setup_block
        );
        assert!(
            emission.setup_block.contains("public function process_image("),
            "process_image() must be present, got:\n{}",
            emission.setup_block
        );
    }

    /// Verify that test stubs emit methods with default implementations.
    /// PHP interfaces require ALL abstract methods to be implemented, even if the
    /// Rust trait has default implementations.
    #[test]
    fn test_backend_includes_default_impl_methods() {
        let trait_bridge = TraitBridgeConfig {
            trait_name: "DocumentExtractor".to_string(),
            super_trait: Some("Plugin".to_string()),
            register_fn: Some("register_document_extractor".to_string()),
            ..TraitBridgeConfig::default()
        };

        // Direct trait method (no default impl).
        let extract_bytes = make_method(
            "extract_bytes",
            vec![
                ("content", TypeRef::Bytes),
                ("mime_type", TypeRef::String),
                ("config", TypeRef::Named("ParseConfig".to_string())),
            ],
            TypeRef::Named("ParseResult".to_string()),
            false,
        );

        // Method with default impl in Rust trait.
        let mut as_sync_extractor = make_method("as_sync_extractor", vec![], TypeRef::String, false);
        as_sync_extractor.has_default_impl = true;

        // Super-trait method with default impl.
        let mut priority = make_method(
            "priority",
            vec![],
            TypeRef::Primitive(crate::core::ir::PrimitiveType::U32),
            false,
        );
        priority.has_default_impl = true;

        let mut fixture = make_fixture("test_default_impl_methods");
        fixture.input = serde_json::json!({
            "extractor": { "type": "test", "name": "test-default-impl" }
        });

        let methods = vec![&extract_bytes, &as_sync_extractor, &priority];
        let emission = super::super::stubs::emit_test_backend(&trait_bridge, &methods, &fixture);

        // PHP requires implementations of ALL abstract methods, including those with
        // default implementations in the Rust trait.
        assert!(
            emission.setup_block.contains("public function extract_bytes("),
            "extract_bytes() must be emitted, got:\n{}",
            emission.setup_block
        );
        assert!(
            emission.setup_block.contains("public function as_sync_extractor()"),
            "as_sync_extractor() with default impl must be emitted for PHP interface, got:\n{}",
            emission.setup_block
        );
        assert!(
            emission.setup_block.contains("public function priority()"),
            "priority() with default impl must be emitted for PHP interface, got:\n{}",
            emission.setup_block
        );
        assert!(
            emission.setup_block.contains("test-default-impl"),
            "Backend name must be correct, got:\n{}",
            emission.setup_block
        );
    }
}

#[cfg(test)]
mod composer_json_tests {
    use super::super::project::{render_composer_json, render_install_sh};
    use crate::e2e::config::DependencyMode;

    #[test]
    fn registry_composer_json_omits_ext_platform_req() {
        let content = render_composer_json(
            "sample_crate/e2e-php",
            "SampleLlm\\\\E2e\\\\",
            "demo_client",
            "sample_crate/demo-client",
            "../../packages/php",
            "1.4.0-rc.32",
            DependencyMode::Registry,
        );
        // Must NOT declare the ext-<name> platform require. The extension is
        // installed via PIE (in install.sh) before `composer install` runs, so
        // Composer doesn't manage it. Declaring ext-<name> in composer.json causes
        // Composer's platform resolver to fail when the extension hasn't been
        // loaded into the current PHP process yet.
        assert!(
            !content.contains(r#""ext-demo_client":"#),
            "registry composer.json must NOT require ext-demo_client (PIE installs it), got:\n{content}"
        );
        // Must declare the php platform require.
        assert!(
            content.contains(r#""php": ">=8.2""#),
            "registry composer.json must require php >=8.2, got:\n{content}"
        );
        // Must NOT contain a direct package require (composer can't resolve it
        // before PIE has installed the .so).
        assert!(
            !content.contains("sample_crate/demo-client"),
            "registry composer.json must not contain a direct package require, got:\n{content}"
        );
        // Must NOT carry minimum-stability / prefer-stable (not load-bearing with
        // only php platform req).
        assert!(
            !content.contains("minimum-stability"),
            "registry composer.json must not contain minimum-stability, got:\n{content}"
        );
        assert!(
            !content.contains("prefer-stable"),
            "registry composer.json must not contain prefer-stable, got:\n{content}"
        );
        // Must keep require-dev (phpunit + guzzle).
        assert!(
            content.contains("phpunit/phpunit"),
            "registry composer.json must keep phpunit in require-dev, got:\n{content}"
        );
        assert!(
            content.contains("guzzlehttp/guzzle"),
            "registry composer.json must keep guzzle in require-dev, got:\n{content}"
        );
    }

    #[test]
    fn registry_install_sh_contains_pie_install() {
        let content = render_install_sh("sample_crate/demo-client", "demo_client", "1.4.0-rc.32");
        // The script uses $PIE as the resolved pie binary path.
        assert!(
            content.contains("\"$PIE\" install"),
            "install.sh must invoke pie via $PIE install, got:\n{content}"
        );
        assert!(
            content.contains("sample_crate/demo-client"),
            "install.sh must reference the package name, got:\n{content}"
        );
        assert!(
            content.starts_with("#!/usr/bin/env bash"),
            "install.sh must start with bash shebang, got:\n{content}"
        );
        // Version is baked in so callers can run `bash install.sh` with no args.
        assert!(
            content.contains(r#"VERSION="${1:-1.4.0-rc.32}""#),
            "install.sh must contain version default, got:\n{content}"
        );
    }

    #[test]
    fn registry_install_sh_strips_version_constraints() {
        // Test constraint operators are stripped from version strings.
        let tests = vec![
            (">=3.5.1", "3.5.1"),
            ("^1.2.3", "1.2.3"),
            ("~2.0.0", "2.0.0"),
            (">1.0", "1.0"),
            ("<2.0", "2.0"),
            ("1.4.0-rc.32", "1.4.0-rc.32"), // Already clean
        ];
        for (input, expected) in tests {
            let content = render_install_sh("test/pkg", "ext", input);
            assert!(
                content.contains(&format!(r#"VERSION="${{1:-{expected}}}""#)),
                "install.sh must strip constraint from '{}' to '{}', got:\n{}",
                input,
                expected,
                content
            );
        }
    }

    #[test]
    fn registry_install_sh_downloads_pie_phar() {
        let content = render_install_sh("test/pkg", "ext", "1.0.0");
        // Ensure the script downloads PIE as a PHAR, not via composer require.
        assert!(
            content.contains("https://github.com/php/pie/releases/latest/download/pie.phar"),
            "install.sh must download PIE PHAR from GitHub, got:\n{content}"
        );
        assert!(
            !content.contains("composer global require php/pie"),
            "install.sh must not use composer to install PIE, got:\n{content}"
        );
    }

    #[test]
    fn registry_install_sh_enables_extension_in_php_ini() {
        let content = render_install_sh("test/pkg", "my_ext", "1.0.0");
        // After PIE install, script must locate php.ini and enable the extension.
        assert!(
            content.contains("php --ini"),
            "install.sh must locate php.ini using 'php --ini', got:\n{content}"
        );
        assert!(
            content.contains("Loaded Configuration File:"),
            "install.sh must grep for 'Loaded Configuration File:' from php --ini, got:\n{content}"
        );
        // Script must append the extension line (idempotently).
        assert!(
            content.contains("extension=my_ext"),
            "install.sh must append 'extension=my_ext' to php.ini, got:\n{content}"
        );
        assert!(
            content.contains("^extension=my_ext"),
            "install.sh must guard against duplicate extension entries, got:\n{content}"
        );
        assert!(
            content.contains(">> \"$PHP_INI\""),
            "install.sh must append to php.ini, got:\n{content}"
        );
    }
}
