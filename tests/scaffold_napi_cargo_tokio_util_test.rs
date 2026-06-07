use alef::core::config::{ResolvedCrateConfig, TraitBridgeConfig, new_config::NewAlefConfig};
use alef::core::ir::{ApiSurface, CoreWrapper, FunctionDef, ParamDef, PrimitiveType, TypeDef, TypeRef};
use alef::scaffold::languages::node::scaffold_node_cargo;

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
        core_wrapper: CoreWrapper::None,
    }
}

fn make_type(name: &str, is_trait: bool) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        rust_path: format!("demo::{name}"),
        original_rust_path: String::new(),
        fields: vec![],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        doc: String::new(),
        cfg: None,
        is_trait,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
        binding_excluded: false,
        binding_exclusion_reason: None,
        is_variant_wrapper: false,
        has_lifetime_params: false,
    }
}

#[test]
fn scaffold_napi_cargo_includes_tokio_util_with_sync_feature_when_trait_bridges_present() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![make_type("MyTrait", true)],
        functions: vec![],
        enums: vec![],
        errors: vec![],
        constants: vec![],
        modules: vec![],
    };

    let mut config = ResolvedCrateConfig::default();
    config.name = "demo".to_string();
    config.workspace_root = Some(std::path::PathBuf::from("/workspace"));

    // Add a trait bridge to enable tokio-util dependency generation
    config.trait_bridges = vec![TraitBridgeConfig {
        type_alias: "MyTrait".to_string(),
        trait_type: "demo::traits::MyTrait".to_string(),
        super_trait: None,
        register_fn: Some("register_my_trait".to_string()),
        unregister_fn: None,
        clear_fn: None,
        bind_via: alef::core::config::BridgeBinding::FunctionParam,
        options_type: None,
    }];

    let result = scaffold_node_cargo(&api, &config).expect("scaffold_node_cargo failed");

    assert_eq!(result.len(), 1, "Should generate one file");
    let content = &result[0].content;

    // Verify tokio-util with sync feature is present in dependencies
    assert!(
        content.contains("tokio-util"),
        "Cargo.toml must include tokio-util when trait bridges are present"
    );
    assert!(
        content.contains(r#"features = ["sync"]"#)
            || content.contains("tokio-util = { version = \"0.7\", features = [\"sync\"] }"),
        "tokio-util must include the 'sync' feature"
    );

    // Verify tokio-util is in the cargo-machete ignored list
    assert!(
        content.contains("tokio-util") && content.contains("[package.metadata.cargo-machete]"),
        "tokio-util should be in the cargo-machete ignored list"
    );

    // Verify async-trait is also present (existing behavior)
    assert!(
        content.contains("async-trait = \"0.1\""),
        "async-trait must still be present for trait bridges"
    );
}

#[test]
fn scaffold_napi_cargo_excludes_tokio_util_when_no_trait_bridges() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
        constants: vec![],
        modules: vec![],
    };

    let mut config = ResolvedCrateConfig::default();
    config.name = "demo".to_string();
    config.workspace_root = Some(std::path::PathBuf::from("/workspace"));
    config.trait_bridges = vec![]; // No trait bridges

    let result = scaffold_node_cargo(&api, &config).expect("scaffold_node_cargo failed");

    assert_eq!(result.len(), 1, "Should generate one file");
    let content = &result[0].content;

    // tokio-util should not be present when there are no trait bridges
    assert!(
        !content.contains("tokio-util"),
        "tokio-util should not be included when there are no trait bridges"
    );
}
