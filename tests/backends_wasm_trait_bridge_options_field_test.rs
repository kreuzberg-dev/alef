use alef::backends::wasm::WasmBackend;
use alef::core::backend::Backend;
use alef::core::config::{BridgeBinding, NewAlefConfig, TraitBridgeConfig};
use alef::core::ir::{ApiSurface, FieldDef, FunctionDef, MethodDef, ParamDef, ReceiverKind, TypeDef, TypeRef};

fn resolved_wasm_config() -> alef::core::config::ResolvedCrateConfig {
    let cfg: NewAlefConfig = toml::from_str(
        r#"
[workspace]
languages = ["wasm"]

[[crates]]
name = "test-lib"
sources = ["src/lib.rs"]

[crates.wasm]
"#,
    )
    .expect("test config must parse");
    cfg.resolve().expect("test config must resolve").remove(0)
}

fn field(name: &str, ty: TypeRef, optional: bool) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        ty,
        optional,
        ..Default::default()
    }
}

fn param(name: &str, ty: TypeRef, optional: bool) -> ParamDef {
    ParamDef {
        name: name.to_string(),
        ty,
        optional,
        ..Default::default()
    }
}

fn trait_type() -> TypeDef {
    TypeDef {
        name: "Renderer".to_string(),
        rust_path: "test_lib::Renderer".to_string(),
        methods: vec![MethodDef {
            name: "render_text".to_string(),
            return_type: TypeRef::String,
            receiver: Some(ReceiverKind::Ref),
            error_type: Some("Error".to_string()),
            ..Default::default()
        }],
        is_trait: true,
        ..Default::default()
    }
}

fn options_type() -> TypeDef {
    TypeDef {
        name: "RenderOptions".to_string(),
        rust_path: "test_lib::RenderOptions".to_string(),
        fields: vec![field("renderer", TypeRef::Named("RendererHandle".to_string()), true)],
        is_clone: true,
        ..Default::default()
    }
}

fn render_function() -> FunctionDef {
    FunctionDef {
        name: "render_document".to_string(),
        rust_path: "test_lib::render_document".to_string(),
        params: vec![param("options", TypeRef::Named("RenderOptions".to_string()), true)],
        return_type: TypeRef::String,
        error_type: Some("Error".to_string()),
        ..Default::default()
    }
}

fn options_field_bridge() -> TraitBridgeConfig {
    TraitBridgeConfig {
        trait_name: "Renderer".to_string(),
        super_trait: Some("Plugin".to_string()),
        registry_getter: Some("test_lib::renderer_registry".to_string()),
        register_fn: Some("register_renderer".to_string()),
        type_alias: Some("RendererHandle".to_string()),
        param_name: Some("renderer".to_string()),
        bind_via: BridgeBinding::OptionsField,
        options_type: Some("RenderOptions".to_string()),
        options_field: Some("renderer".to_string()),
        ..Default::default()
    }
}

#[test]
fn options_field_bridge_injects_visitor_handle() {
    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "1.0.0".to_string(),
        types: vec![trait_type(), options_type()],
        functions: vec![render_function()],
        ..Default::default()
    };
    let mut config = resolved_wasm_config();
    config.trait_bridges = vec![options_field_bridge()];

    let files = WasmBackend
        .generate_bindings(&api, &config)
        .expect("options-field trait bridge generation should succeed");
    let content = &files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with("lib.rs"))
        .expect("lib.rs must be generated")
        .content;

    assert!(
        content.contains(
            "pub fn render_document(options: Option<WasmRenderOptions>, renderer: Option<wasm_bindgen::JsValue>)"
        ),
        "options-field bridge wrapper must append the JS visitor parameter;\n{content}"
    );
    assert!(
        content.contains("let renderer_handle: Option<test_lib::RendererHandle> = renderer.map(|v|"),
        "options-field bridge body must wrap the JS value in the configured handle;\n{content}"
    );
    assert!(
        content.contains("result.renderer = renderer_handle.clone();"),
        "options-field bridge body must inject the visitor handle into converted options;\n{content}"
    );
    assert!(
        content.contains(concat!(
            "test_lib::render_document(options_core).map(|val| val.into())",
            ".map_err(|e| wasm_bindgen::JsError::new(&e.to_string()).into())"
        )),
        "options-field bridge body must preserve fallible core call mapping;\n{content}"
    );
}
