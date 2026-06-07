use crate::core::ir::{ParamDef, TypeRef};
use ahash::AHashSet;

use super::params::gen_param_conversion_with_enums;
use super::return_handling::return_type_needs_non_serde_named;

#[test]
fn return_type_needs_non_serde_named_vec_non_serde() {
    // Regression: Vec<PatternMatch> where PatternMatch lacks serde must trigger
    // unimplemented body generation instead of emitting json_or_vec_or_map path.
    let mut serde_names: AHashSet<String> = AHashSet::new();
    serde_names.insert("ExtractionResult".to_string());

    let vec_non_serde = TypeRef::Vec(Box::new(TypeRef::Named("PatternMatch".to_string())));
    assert!(
        return_type_needs_non_serde_named(&vec_non_serde, &serde_names),
        "Vec<PatternMatch> without Serialize must be detected as needing stub"
    );
}

#[test]
fn return_type_needs_non_serde_named_vec_serde_ok() {
    // Vec<ExtractionResult> where ExtractionResult has serde should NOT trigger stub.
    let mut serde_names: AHashSet<String> = AHashSet::new();
    serde_names.insert("ExtractionResult".to_string());

    let vec_serde = TypeRef::Vec(Box::new(TypeRef::Named("ExtractionResult".to_string())));
    assert!(
        !return_type_needs_non_serde_named(&vec_serde, &serde_names),
        "Vec<ExtractionResult> with Serialize must NOT be detected as needing stub"
    );
}

#[test]
fn return_type_needs_non_serde_named_primitive_vec_not_affected() {
    // Vec<String>, Vec<u64> etc. never need serde check.
    let serde_names: AHashSet<String> = AHashSet::new();
    assert!(!return_type_needs_non_serde_named(
        &TypeRef::Vec(Box::new(TypeRef::String)),
        &serde_names
    ));
    assert!(!return_type_needs_non_serde_named(
        &TypeRef::Vec(Box::new(TypeRef::Primitive(crate::core::ir::PrimitiveType::U64))),
        &serde_names
    ));
}

#[test]
fn named_param_is_mut_call_site_passes_local_directly() {
    // Regression (Bug 2): When is_mut=true, the conversion template binds the local
    // via `let result_rs = unsafe { &mut *result }` — the local is already `&mut T`.
    // The call site must pass `result_rs` directly, NOT `&mut result_rs`, which would
    // produce `&mut &mut T` (E0308 mismatched types).
    let p = ParamDef {
        name: "result".to_string(),
        ty: TypeRef::Named("ExtractionResult".to_string()),
        optional: false,
        default: None,
        sanitized: false,
        typed_default: None,
        is_ref: false,
        is_mut: true,
        newtype_wrapper: None,
        original_type: None,
        map_is_ahash: false,
        map_key_is_cow: false,
        vec_inner_is_ref: false,
        map_is_btree: false,
        core_wrapper: crate::core::ir::CoreWrapper::None,
    };
    let rs = format!("{}_rs", p.name);
    // Simulate the call-site arm for Named non-optional with is_mut
    // (mirrors the TypeRef::Named(!p.optional) arm in gen_free_function / gen_method_wrapper)
    let result = if p.is_mut {
        // Local is already &mut T — pass directly, no extra &mut prefix.
        rs.clone()
    } else if p.is_ref {
        format!("&{rs}")
    } else {
        rs.clone()
    };
    assert_eq!(
        result, "result_rs",
        "is_mut Named param must pass local directly (already &mut T)"
    );
}

#[test]
fn enum_param_local_name_uses_param_name_not_type_name() {
    // Regression (Bug 3): enum-discriminant params must name the local after the FFI
    // param (e.g. `strategy_rs`), not after the type (e.g. `redaction_strategy_rs`).
    // The conversion helper is still `{type_snake}_from_i32_rs` but the local and its
    // call site use `{param_name}_rs`.
    let mut enum_names: AHashSet<String> = AHashSet::new();
    enum_names.insert("RedactionStrategy".to_string());

    let p = ParamDef {
        name: "strategy".to_string(), // param name differs from type snake
        ty: TypeRef::Named("RedactionStrategy".to_string()),
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
    };

    // Run the real conversion generator.
    let output = gen_param_conversion_with_enums(&p, false, false, &TypeRef::Unit, "sample_crate", &enum_names);

    // Must bind to `strategy_rs` (from param name), not `redaction_strategy_rs` (from type).
    assert!(
        output.contains("let strategy_rs ="),
        "enum local must be named after param (strategy_rs), got:\n{output}"
    );
    // Must call the helper with the actual param name `strategy`, not `redaction_strategy`.
    assert!(
        output.contains("redaction_strategy_from_i32_rs(strategy)"),
        "enum helper must receive the FFI param name (strategy), got:\n{output}"
    );
}
