/// Regression test for Java enum parameter _FREE handle emission
///
/// When an enum with serde is used as a function parameter (and marshaled via _from_json),
/// the NativeLib must emit the matching _FREE handle, even if the enum is also used as
/// a return type elsewhere. This test ensures that the _FREE handle is declared in NativeLib.java
/// when it's referenced by the wrapper code.
///
/// Reproducer: RegionKind enum exported as opaque via FFI, used as parameter in functions.
use alef::backends::java::JavaBackend;
use alef::core::backend::Backend;
use alef::core::config::{NewAlefConfig, ResolvedCrateConfig};
use alef::core::ir::{ApiSurface, EnumDef, EnumVariant, FunctionDef, ParamDef, PrimitiveType, TypeRef};

fn resolved_one(toml: &str) -> ResolvedCrateConfig {
    let cfg: NewAlefConfig = toml::from_str(toml).unwrap();
    cfg.resolve().unwrap().remove(0)
}

fn make_test_config(package: &str) -> ResolvedCrateConfig {
    resolved_one(&format!(
        r#"
[workspace]
languages = ["java", "ffi"]

[[crates]]
name = "test_lib"
sources = ["src/lib.rs"]

[crates.ffi]
prefix = "test"

[crates.java]
package = "{package}"
"#
    ))
}

#[test]
fn enum_param_emits_free_handle_in_native_lib() {
    let enum_def = EnumDef {
        name: "MyMode".to_string(),
        rust_path: "test_lib::MyMode".to_string(),
        original_rust_path: String::new(),
        variants: vec![
            EnumVariant {
                name: "A".to_string(),
                fields: vec![],
                is_default: false,
                serde_rename: None,
                is_tuple: false,
                binding_excluded: false,
                binding_exclusion_reason: None,
                doc: String::new(),
                originally_had_data_fields: false,
            },
            EnumVariant {
                name: "B".to_string(),
                fields: vec![],
                is_default: false,
                serde_rename: None,
                is_tuple: false,
                binding_excluded: false,
                binding_exclusion_reason: None,
                doc: String::new(),
                originally_had_data_fields: false,
            },
        ],
        doc: "Synthetic enum for testing".to_string(),
        serde_rename_all: None,
        has_serde: true,
        serde_tag: Default::default(),
        serde_untagged: false,
        binding_excluded: false,
        binding_exclusion_reason: None,
        cfg: None,
        excluded_variants: Default::default(),
        is_copy: true,
    };

    let function = FunctionDef {
        name: "process".to_string(),
        rust_path: "test_lib::process".to_string(),
        original_rust_path: String::new(),
        params: vec![ParamDef {
            name: "mode".to_string(),
            ty: TypeRef::Named("MyMode".to_string()),
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
            core_wrapper: alef::core::ir::CoreWrapper::None,
        }],
        return_type: TypeRef::Primitive(PrimitiveType::I32),
        is_async: false,
        error_type: None,
        doc: String::new(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        binding_excluded: false,
        binding_exclusion_reason: None,
    };

    let api = ApiSurface {
        crate_name: "test_lib".to_string(),
        version: "0.1.0".to_string(),
        types: vec![],
        functions: vec![function],
        enums: vec![enum_def],
        errors: vec![],
        excluded_type_paths: Default::default(),
        excluded_trait_names: Default::default(),
        services: vec![],
        handler_contracts: vec![],
        unsupported_public_items: Vec::new(),
    };

    let files = JavaBackend
        .generate_bindings(&api, &make_test_config("com.example"))
        .unwrap();

    let native_lib = files
        .iter()
        .find(|f| f.path.file_name().and_then(|n| n.to_str()) == Some("NativeLib.java"))
        .expect("NativeLib.java")
        .content
        .as_str();

    // The emitted wrapper code will use NativeLib.TEST_MY_MODE_FROM_JSON to marshal the enum param
    assert!(
        native_lib.contains("TEST_MY_MODE_FROM_JSON"),
        "NativeLib must declare TEST_MY_MODE_FROM_JSON handle for enum param marshaling"
    );

    // The matching _FREE handle MUST be declared, even if MY_MODE is not an opaque type,
    // because the wrapper code will invoke it to clean up the allocated pointer
    assert!(
        native_lib.contains("TEST_MY_MODE_FREE"),
        "NativeLib must declare TEST_MY_MODE_FREE handle when TEST_MY_MODE_FROM_JSON is declared\n\n{native_lib}"
    );

    // Verify the FFI wrapper actually uses the handle
    let wrapper = files
        .iter()
        .find(|f| f.path.file_name().and_then(|n| n.to_str()) == Some("TestLibRs.java"))
        .expect("TestLibRs.java")
        .content
        .as_str();

    assert!(
        wrapper.contains("NativeLib.TEST_MY_MODE_FROM_JSON"),
        "Wrapper must use TEST_MY_MODE_FROM_JSON for enum param"
    );

    assert!(
        wrapper.contains("NativeLib.TEST_MY_MODE_FREE"),
        "Wrapper must use TEST_MY_MODE_FREE to clean up allocated enum param\n\n{wrapper}"
    );
}
