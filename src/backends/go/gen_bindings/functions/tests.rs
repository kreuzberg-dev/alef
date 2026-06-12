use super::*;
use crate::core::ir::{ParamDef, PrimitiveType, TypeRef};

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

#[test]
fn test_params_require_marshal_for_named_non_opaque() {
    let params = vec![make_param("options", TypeRef::Named("Config".to_string()))];
    let opaque: std::collections::HashSet<&str> = std::collections::HashSet::new();
    assert!(params_require_marshal(&params, &opaque));
}

#[test]
fn test_params_require_marshal_false_for_opaque() {
    let params = vec![make_param("client", TypeRef::Named("Client".to_string()))];
    let opaque: std::collections::HashSet<&str> = ["Client"].into();
    assert!(!params_require_marshal(&params, &opaque));
}

#[test]
fn test_is_bridge_param_matches_by_name() {
    let param = make_param("visitor", TypeRef::Named("VisitorHandle".to_string()));
    let bridge_names: HashSet<String> = ["visitor".to_string()].into();
    let aliases: HashSet<String> = HashSet::new();
    assert!(is_bridge_param(&param, &bridge_names, &aliases));
}

#[test]
fn test_params_require_marshal_for_vec() {
    let params = vec![make_param(
        "items",
        TypeRef::Vec(Box::new(TypeRef::Primitive(PrimitiveType::U32))),
    )];
    let opaque: std::collections::HashSet<&str> = std::collections::HashSet::new();
    assert!(params_require_marshal(&params, &opaque));
}

fn make_bytes_result_func(name: &str, with_bytes_param: bool) -> FunctionDef {
    let params = if with_bytes_param {
        vec![ParamDef {
            name: "data".to_string(),
            ty: TypeRef::Bytes,
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
        }]
    } else {
        vec![]
    };
    FunctionDef {
        name: name.to_string(),
        rust_path: String::new(),
        original_rust_path: String::new(),
        params,
        return_type: TypeRef::Bytes,
        is_async: false,
        error_type: Some("SampleCrateError".to_string()),
        doc: String::new(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        binding_excluded: false,
        binding_exclusion_reason: None,
        version: Default::default(),
    }
}

fn make_bytes_result_method(name: &str) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        doc: String::new(),
        params: vec![ParamDef {
            name: "data".to_string(),
            ty: TypeRef::Bytes,
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
        }],
        return_type: TypeRef::Bytes,
        is_static: false,
        is_async: false,
        error_type: Some("SampleCrateError".to_string()),
        receiver: None,
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

#[test]
fn test_is_bytes_result_func_detects_bytes_with_error() {
    let func = make_bytes_result_func("process_image", true);
    assert!(is_bytes_result_func(&func));
}

#[test]
fn test_is_bytes_result_func_false_for_bytes_without_error() {
    let mut func = make_bytes_result_func("get_data", false);
    func.error_type = None;
    assert!(!is_bytes_result_func(&func));
}

#[test]
fn test_is_bytes_result_func_false_for_string_with_error() {
    let func = FunctionDef {
        name: "get_text".to_string(),
        rust_path: String::new(),
        original_rust_path: String::new(),
        params: vec![],
        return_type: TypeRef::String,
        is_async: false,
        error_type: Some("SampleCrateError".to_string()),
        doc: String::new(),
        cfg: None,
        sanitized: false,
        return_sanitized: false,
        returns_ref: false,
        returns_cow: false,
        return_newtype_wrapper: None,
        binding_excluded: false,
        binding_exclusion_reason: None,
        version: Default::default(),
    };
    assert!(!is_bytes_result_func(&func));
}

#[test]
fn test_is_bytes_result_method_detects_correctly() {
    let method = make_bytes_result_method("render_page");
    assert!(is_bytes_result_method(&method));
}

#[test]
fn test_gen_function_wrapper_bytes_result_emits_out_params() {
    let func = make_bytes_result_func("process_image", true);
    let opaque: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let bridge_names: HashSet<String> = HashSet::new();
    let bridge_aliases: HashSet<String> = HashSet::new();
    let value_only_types: HashSet<String> = HashSet::new();
    let enum_names: HashSet<String> = HashSet::new();
    let ffi_param_enum_names: HashSet<String> = HashSet::new();
    let out = gen_function_wrapper(
        &func,
        "krz",
        &opaque,
        &bridge_names,
        &bridge_aliases,
        &value_only_types,
        &enum_names,
        &ffi_param_enum_names,
    );
    // Return type must be ([]byte, error)
    assert!(out.contains("([]byte, error)"), "missing bytes return type in:\n{out}");
    // Must declare out-param variables (outLen and outCap are declared together)
    assert!(out.contains("var outPtr"), "missing outPtr in:\n{out}");
    assert!(out.contains("outLen"), "missing outLen in:\n{out}");
    assert!(out.contains("outCap"), "missing outCap in:\n{out}");
    // Must pass out-params to C call
    assert!(out.contains("&outPtr"), "missing &outPtr in:\n{out}");
    assert!(out.contains("&outLen"), "missing &outLen in:\n{out}");
    assert!(out.contains("&outCap"), "missing &outCap in:\n{out}");
    // Must copy bytes via C.GoBytes
    assert!(out.contains("C.GoBytes"), "missing C.GoBytes in:\n{out}");
    // Must free via krz_free_bytes
    assert!(out.contains("krz_free_bytes"), "missing krz_free_bytes in:\n{out}");
}
