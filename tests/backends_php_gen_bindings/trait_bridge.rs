use super::*;

#[test]
fn test_php_visitor_bridge_produces_visitor_struct() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("PhpHtmlVisitorBridge"),
        "PHP visitor bridge struct must be named Php{{TraitName}}Bridge"
    );
    assert!(
        code.code.contains("impl my_lib::HtmlVisitor for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement the trait"
    );
}

#[test]
fn test_php_visitor_bridge_has_php_obj_field() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("php_obj: *mut ext_php_rs::types::ZendObject"),
        "PHP visitor bridge must store a raw ZendObject pointer in 'php_obj'"
    );
    assert!(
        code.code.contains("cached_name: String"),
        "PHP visitor bridge must cache the plugin name"
    );
}

#[test]
fn test_php_plugin_bridge_produces_wrapper_struct_with_inner_and_cached_name() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("pub struct PhpOcrBackendBridge"),
        "PHP plugin bridge wrapper struct must be PhpOcrBackendBridge"
    );
    assert!(
        code.code.contains("inner:"),
        "PHP plugin bridge wrapper must have an 'inner' field"
    );
    assert!(
        code.code.contains("cached_name: String"),
        "PHP plugin bridge wrapper must have a 'cached_name: String' field"
    );
}

#[test]
fn test_php_plugin_bridge_generates_super_trait_impl() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::Plugin for PhpOcrBackendBridge"),
        "PHP plugin bridge must implement Plugin super-trait"
    );
    assert!(code.code.contains("fn name("), "Plugin impl must contain name()");
    assert!(
        code.code.contains("fn initialize("),
        "Plugin impl must contain initialize()"
    );
    assert!(
        code.code.contains("fn shutdown("),
        "Plugin impl must contain shutdown()"
    );
}

#[test]
fn test_php_plugin_bridge_generates_trait_impl_with_forwarded_methods() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("impl my_lib::OcrBackend for PhpOcrBackendBridge"),
        "PHP plugin bridge must implement the trait itself"
    );
    assert!(
        code.code.contains("fn process("),
        "trait impl must forward the 'process' method"
    );
}

#[test]
fn test_php_plugin_bridge_generates_registration_fn_with_php_function_attribute() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "OcrBackend",
        vec![make_method_php("process", TypeRef::String, true, false)],
    );
    let bridge_cfg = make_plugin_bridge_cfg_php("OcrBackend");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("#[php_function]"),
        "PHP registration function must carry the #[php_function] attribute"
    );
    assert!(
        code.code.contains("pub fn register_ocrbackend("),
        "PHP registration function must use the configured name"
    );
}

#[test]
fn test_php_trait_registry_methods_use_matching_native_facade_and_stub_names() {
    let backend = PhpBackend;
    let mut config = make_config();
    config.trait_bridges = vec![alef::core::config::TraitBridgeConfig {
        trait_name: "OcrBackend".to_string(),
        super_trait: Some("Plugin".to_string()),
        registry_getter: Some("my_lib::get_registry".to_string()),
        register_fn: Some("register_ocr_backend".to_string()),
        unregister_fn: Some("unregister_ocr_backend".to_string()),
        clear_fn: Some("clear_ocr_backends".to_string()),
        ..Default::default()
    }];
    let api = ApiSurface {
        types: vec![make_trait_def_php(
            "OcrBackend",
            vec![make_method_php("process", TypeRef::String, true, false)],
        )],
        ..make_api_php()
    };

    let files = backend.generate_bindings(&api, &config).unwrap();
    let lib = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("lib.rs generated");
    assert!(
        lib.content
            .contains("#[php(name = \"registerOcrBackend\")]\n    pub fn register_ocr_backend(")
            && lib
                .content
                .contains("#[php(name = \"unregisterOcrBackend\")]\n    pub fn unregister_ocr_backend(")
            && lib
                .content
                .contains("#[php(name = \"clearOcrBackends\")]\n    pub fn clear_ocr_backends("),
        "native Api class methods must expose the same camelCase names used by the facade:\n{}",
        lib.content
    );

    let public = backend.generate_public_api(&api, &config).unwrap();
    let facade = &public[0].content;
    assert!(
        facade.contains("public static function registerOcrBackend(\nOcrBackend $backend) : void")
            && facade.contains("\\Test\\Lib\\TestLibApi::registerOcrBackend($backend)")
            && facade.contains("\\Test\\Lib\\TestLibApi::unregisterOcrBackend($name)")
            && facade.contains("\\Test\\Lib\\TestLibApi::clearOcrBackends()"),
        "facade methods must call the native Api class public names:\n{facade}"
    );

    let stubs = backend.generate_type_stubs(&api, &config).unwrap();
    let stub = &stubs[0].content;
    assert!(
        stub.contains("public static function registerOcrBackend(\\Test\\Lib\\OcrBackend $backend): void")
            && stub.contains("public static function unregisterOcrBackend(string $name): void")
            && stub.contains("public static function clearOcrBackends(): void"),
        "extension stubs must expose registry methods on the native Api class:\n{stub}"
    );
}

#[test]
fn test_php_plugin_bridge_validates_required_methods() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "Analyzer",
        vec![
            make_method_php("analyze", TypeRef::String, true, false), // required
            make_method_php("describe", TypeRef::String, false, true), // optional
        ],
    );
    let bridge_cfg = alef::core::config::TraitBridgeConfig {
        trait_name: "Analyzer".to_string(),
        super_trait: Some("Plugin".to_string()),
        registry_getter: Some("my_lib::get_registry".to_string()),
        register_fn: Some("register_analyzer".to_string()),
        unregister_fn: None,
        clear_fn: None,
        type_alias: None,
        param_name: None,
        register_extra_args: None,
        exclude_languages: Vec::new(),
        ffi_skip_methods: Vec::new(),
        bind_via: alef::core::config::BridgeBinding::FunctionParam,
        options_type: None,
        options_field: None,
        context_type: None,
        result_type: None,
    };
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    // Registration fn must null-check the required method "analyze" via get_property
    assert!(
        code.code.contains("\"analyze\""),
        "PHP registration fn must validate required method 'analyze'"
    );
    assert!(
        code.code.contains("try_call_method"),
        "PHP registration fn must check method presence via try_call_method"
    );
}

#[test]
fn test_php_sync_method_body_uses_try_call_method() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php("Scanner", vec![make_method_php("scan", TypeRef::String, true, false)]);
    let bridge_cfg = make_plugin_bridge_cfg_php("Scanner");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("try_call_method"),
        "PHP sync method body must use try_call_method to dispatch to PHP"
    );
}

#[test]
fn test_php_async_method_body_uses_box_pin() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php("Processor", vec![make_async_method_php("run", TypeRef::Unit)]);
    let bridge_cfg = make_plugin_bridge_cfg_php("Processor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("WORKER_RUNTIME.block_on(async"),
        "PHP async method body must use WORKER_RUNTIME.block_on(async {{ ... }})"
    );
}

#[test]
fn test_php_visitor_bridge_has_send_sync_impls() {
    use alef::backends::php::trait_bridge::gen_trait_bridge;

    let trait_def = make_trait_def_php(
        "HtmlVisitor",
        vec![make_method_php("visit_node", TypeRef::Unit, false, true)],
    );
    let bridge_cfg = make_visitor_bridge_cfg_php("HtmlVisitor", "HtmlVisitor");
    let api = make_api_php();

    let code = gen_trait_bridge(&trait_def, &bridge_cfg, "my_lib", "Error", "Error::from({msg})", &api);

    assert!(
        code.code.contains("unsafe impl Send for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement Send"
    );
    assert!(
        code.code.contains("unsafe impl Sync for PhpHtmlVisitorBridge"),
        "PHP visitor bridge must implement Sync"
    );
}
