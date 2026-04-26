use alef_backend_swift::gen_rust_crate;
use alef_core::config::{AlefConfig, CrateConfig};
use alef_core::ir::{
    ApiSurface, CoreWrapper, EnumDef, EnumVariant, FieldDef, FunctionDef, ParamDef, PrimitiveType, TypeDef,
    TypeRef,
};
use alef_core::template_versions;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_field(name: &str, ty: TypeRef) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        ty,
        optional: false,
        default: None,
        doc: String::new(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: None,
        core_wrapper: CoreWrapper::None,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
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
    }
}

fn make_type(name: &str, fields: Vec<FieldDef>) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        rust_path: format!("demo::{name}"),
        original_rust_path: String::new(),
        fields,
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        doc: String::new(),
        cfg: None,
        is_trait: false,
        has_default: false,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: false,
        super_traits: vec![],
    }
}

fn make_enum(name: &str, variants: Vec<&str>) -> EnumDef {
    EnumDef {
        name: name.to_string(),
        rust_path: format!("demo::{name}"),
        original_rust_path: String::new(),
        variants: variants
            .into_iter()
            .map(|v| EnumVariant {
                name: v.to_string(),
                fields: vec![],
                doc: String::new(),
                is_default: false,
                serde_rename: None,
            is_tuple: false,
            })
            .collect(),
        doc: String::new(),
        cfg: None,
        serde_tag: None,
        serde_rename_all: None,
    }
}

fn make_config() -> AlefConfig {
    AlefConfig {
        version: None,
        crate_config: CrateConfig {
            name: "demo-crate".to_string(),
            sources: vec![],
            version_from: "Cargo.toml".to_string(),
            core_import: None,
            workspace_root: None,
            skip_core_import: false,
            features: vec![],
            path_mappings: std::collections::HashMap::new(),
            auto_path_mappings: Default::default(),
            extra_dependencies: Default::default(),
            source_crates: vec![],
            error_type: None,
            error_constructor: None,
        },
        languages: vec![],
        exclude: Default::default(),
        include: Default::default(),
        output: Default::default(),
        python: None,
        node: None,
        ruby: None,
        php: None,
        elixir: None,
        wasm: None,
        ffi: None,
        gleam: None,
        go: None,
        java: None,
        kotlin: None,
        dart: None,
        swift: None,
        csharp: None,
        r: None,
        zig: None,
        scaffold: None,
        readme: None,
        lint: None,
        update: None,
        test: None,
        setup: None,
        clean: None,
        build_commands: None,
        publish: None,
        custom_files: None,
        adapters: vec![],
        custom_modules: alef_core::config::CustomModulesConfig::default(),
        custom_registrations: alef_core::config::CustomRegistrationsConfig::default(),
        opaque_types: std::collections::HashMap::new(),
        generate: alef_core::config::GenerateConfig::default(),
        generate_overrides: std::collections::HashMap::new(),
        dto: Default::default(),
        sync: None,
        e2e: None,
        trait_bridges: vec![],
        tools: alef_core::config::ToolsConfig::default(),
    format: ::alef_core::config::FormatConfig::default(),
    format_overrides: ::std::collections::HashMap::new(),
    }
}

// ── Cargo.toml tests ──────────────────────────────────────────────────────────

#[test]
fn cargo_toml_contains_swift_bridge_version() {
    let api = ApiSurface {
        crate_name: "my-lib".into(),
        version: "1.2.3".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let cargo = files.iter().find(|f| f.path.ends_with("Cargo.toml")).unwrap();

    let expected_bridge = template_versions::cargo::SWIFT_BRIDGE;
    let expected_build = template_versions::cargo::SWIFT_BRIDGE_BUILD;

    assert!(
        cargo.content.contains(&format!("swift-bridge = \"{expected_bridge}\"")),
        "Cargo.toml missing swift-bridge version: {}",
        cargo.content
    );
    assert!(
        cargo.content.contains(&format!("swift-bridge-build = \"{expected_build}\"")),
        "Cargo.toml missing swift-bridge-build version: {}",
        cargo.content
    );
}

#[test]
fn cargo_toml_contains_crate_name_and_version() {
    let api = ApiSurface {
        crate_name: "my-lib".into(),
        version: "0.5.1".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let cargo = files.iter().find(|f| f.path.ends_with("Cargo.toml")).unwrap();

    assert!(
        cargo.content.contains("name = \"my-lib-swift\""),
        "Cargo.toml missing package name: {}",
        cargo.content
    );
    assert!(
        cargo.content.contains("version = \"0.5.1\""),
        "Cargo.toml missing version: {}",
        cargo.content
    );
}

#[test]
fn cargo_toml_has_cdylib_and_staticlib() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let cargo = files.iter().find(|f| f.path.ends_with("Cargo.toml")).unwrap();

    assert!(
        cargo.content.contains("\"cdylib\""),
        "Cargo.toml missing cdylib: {}",
        cargo.content
    );
    assert!(
        cargo.content.contains("\"staticlib\""),
        "Cargo.toml missing staticlib: {}",
        cargo.content
    );
}

// ── lib.rs tests ──────────────────────────────────────────────────────────────

#[test]
fn lib_rs_contains_bridge_module() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("#[swift_bridge::bridge]"),
        "lib.rs missing bridge attribute: {}",
        lib.content
    );
    assert!(
        lib.content.contains("mod ffi {"),
        "lib.rs missing ffi module: {}",
        lib.content
    );
}

#[test]
fn lib_rs_has_extern_rust_block_per_type() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![
            make_type(
                "Point",
                vec![
                    make_field("x_coord", TypeRef::Primitive(PrimitiveType::I64)),
                    make_field("y_coord", TypeRef::Primitive(PrimitiveType::I64)),
                ],
            ),
            make_type("Empty", vec![]),
        ],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("extern \"Rust\""),
        "lib.rs missing extern Rust block: {}",
        lib.content
    );
    assert!(
        lib.content.contains("type Point;"),
        "lib.rs missing Point type decl: {}",
        lib.content
    );
    assert!(
        lib.content.contains("type Empty;"),
        "lib.rs missing Empty type decl: {}",
        lib.content
    );
}

#[test]
fn lib_rs_type_has_constructor_and_getters() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![make_type(
            "Point",
            vec![
                make_field("x_coord", TypeRef::Primitive(PrimitiveType::I64)),
                make_field("y_coord", TypeRef::Primitive(PrimitiveType::I64)),
            ],
        )],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("#[swift_bridge(init)]"),
        "lib.rs missing init attribute: {}",
        lib.content
    );
    assert!(
        lib.content.contains("fn new("),
        "lib.rs missing constructor: {}",
        lib.content
    );
    assert!(
        lib.content.contains("fn x_coord("),
        "lib.rs missing x_coord getter: {}",
        lib.content
    );
    assert!(
        lib.content.contains("fn y_coord("),
        "lib.rs missing y_coord getter: {}",
        lib.content
    );
}

#[test]
fn lib_rs_has_free_function_shim() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "fetch_data".into(),
            rust_path: "demo::fetch_data".into(),
            original_rust_path: String::new(),
            params: vec![],
            return_type: TypeRef::String,
            is_async: false,
            error_type: None,
            doc: String::new(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("fn fetch_data("),
        "lib.rs missing fetch_data shim: {}",
        lib.content
    );
    // The shim should delegate to the source crate
    assert!(
        lib.content.contains("demo::fetch_data("),
        "lib.rs shim not delegating to source crate: {}",
        lib.content
    );
}

#[test]
fn lib_rs_async_function_emits_todo_marker_not_swift_bridge_async() {
    // swift-bridge v0.1.x has no `async` attribute or async-fn extern support;
    // async functions emit a TODO marker pointing the user at callback bridging.
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "load_async".into(),
            rust_path: "demo::load_async".into(),
            original_rust_path: String::new(),
            params: vec![],
            return_type: TypeRef::String,
            is_async: true,
            error_type: None,
            doc: String::new(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        !lib.content.contains("#[swift_bridge(async)]"),
        "swift_bridge(async) is not a real attribute in v0.1.x: {}",
        lib.content
    );
    assert!(
        lib.content.contains("TODO(swift-bridge async)"),
        "lib.rs should carry a TODO marker for async functions: {}",
        lib.content
    );
    // The outer wrapper fn (outside the extern block) keeps `async fn` so the
    // user's source-call site stays type-correct.
    assert!(
        lib.content.contains("pub async fn load_async("),
        "outer wrapper should remain async fn: {}",
        lib.content
    );
}

#[test]
fn lib_rs_result_function_has_map_err_chain() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![FunctionDef {
            name: "parse_input".into(),
            rust_path: "demo::parse_input".into(),
            original_rust_path: String::new(),
            params: vec![make_param("raw", TypeRef::String)],
            return_type: TypeRef::Primitive(PrimitiveType::I32),
            is_async: false,
            error_type: Some("ParseError".into()),
            doc: String::new(),
            cfg: None,
            sanitized: false,
            returns_ref: false,
            returns_cow: false,
            return_newtype_wrapper: None,
        }],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains(".map_err(|e| e.to_string())"),
        "lib.rs missing map_err chain: {}",
        lib.content
    );
    assert!(
        lib.content.contains("Result<"),
        "lib.rs missing Result return type: {}",
        lib.content
    );
}

// ── build.rs tests ────────────────────────────────────────────────────────────

#[test]
fn build_rs_calls_parse_bridges() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let build = files.iter().find(|f| f.path.ends_with("build.rs")).unwrap();

    assert!(
        build.content.contains("swift_bridge_build::parse_bridges"),
        "build.rs missing parse_bridges call: {}",
        build.content
    );
    assert!(
        build.content.contains("OUT_DIR"),
        "build.rs missing OUT_DIR: {}",
        build.content
    );
}

// ── file count and path tests ─────────────────────────────────────────────────

#[test]
fn emit_returns_three_files() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    assert_eq!(files.len(), 3, "expected 3 generated files, got {}", files.len());

    let paths: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("Cargo.toml")),
        "missing Cargo.toml in {:?}",
        paths
    );
    assert!(
        paths.iter().any(|p| p.ends_with("src/lib.rs")),
        "missing src/lib.rs in {:?}",
        paths
    );
    assert!(
        paths.iter().any(|p| p.ends_with("build.rs")),
        "missing build.rs in {:?}",
        paths
    );
}

#[test]
fn lib_rs_has_generated_header_comment() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("// Generated by alef. Do not edit by hand."),
        "lib.rs missing generated header comment: {}",
        lib.content
    );
}

#[test]
fn lib_rs_has_wrapper_newtype_for_type() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![make_type(
            "Point",
            vec![
                make_field("x_coord", TypeRef::Primitive(PrimitiveType::I64)),
                make_field("y_coord", TypeRef::Primitive(PrimitiveType::I64)),
            ],
        )],
        functions: vec![],
        enums: vec![],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    // Wrapper newtype should reference the source crate
    assert!(
        lib.content.contains("pub struct Point("),
        "lib.rs missing Point wrapper newtype: {}",
        lib.content
    );
    assert!(
        lib.content.contains("demo::Point"),
        "lib.rs wrapper not referencing source crate: {}",
        lib.content
    );
}

#[test]
fn lib_rs_enum_extern_block_and_wrapper() {
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![make_enum("Status", vec!["Active", "Inactive"])],
        errors: vec![],
    };

    let files = gen_rust_crate::emit(&api, &make_config()).unwrap();
    let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();

    assert!(
        lib.content.contains("type Status;"),
        "lib.rs missing Status extern type: {}",
        lib.content
    );
    assert!(
        lib.content.contains("pub enum Status {"),
        "lib.rs missing Status wrapper enum: {}",
        lib.content
    );
}
