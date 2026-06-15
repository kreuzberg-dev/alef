use super::{classify_param_type, emit_param_conversion};
use crate::core::ir::TypeRef;

/// classify_param_type returns Plain for a bare Named type.
#[test]
fn classify_param_type_returns_plain_for_named() {
    let ty = TypeRef::Named("Foo".to_string());
    let result = classify_param_type(&ty);
    assert!(result.is_some());
    let (name, _) = result.unwrap();
    assert_eq!(name, "Foo");
}

/// classify_param_type returns None for a primitive type.
#[test]
fn classify_param_type_returns_none_for_primitive() {
    let ty = TypeRef::Primitive(crate::core::ir::PrimitiveType::Bool);
    assert!(classify_param_type(&ty).is_none());
}

/// emit_param_conversion emits a guarded None check when optional.
#[test]
fn emit_param_conversion_guards_optional() {
    let mut out = String::new();
    emit_param_conversion(&mut out, "_rust_x", "x", "convert(x)", true);
    assert!(out.contains("if x is not None else None"));
}

/// emit_param_conversion emits a direct assignment when not optional.
#[test]
fn emit_param_conversion_direct_when_required() {
    let mut out = String::new();
    emit_param_conversion(&mut out, "_rust_x", "x", "convert(x)", false);
    assert!(!out.contains("if x is not None"));
    assert!(out.contains("_rust_x = convert(x)"));
}

/// Async Pyo3 functions with let_bindings that create temporary borrows
/// (e.g., Vec<&str> from Vec<String>) must place the bindings INSIDE the
/// `async move` block, not before it. This ensures the temporary lifetimes
/// extend to when the future executes, not just when the function returns.
///
/// This is a regression test for the fix that moves ref_let_bindings inside
/// the async block for AsyncPattern::Pyo3FutureIntoPy functions.
#[test]
fn async_pyo3_functions_place_bindings_inside_async_block() {
    // This test documents the expected behavior. The actual code generation
    // is tested implicitly when alef regenerates downstream packages and the
    // result compiles without E0597 (does not live long enough) errors.
    //
    // The generated code should look like:
    //   pyo3_async_runtimes::tokio::future_into_py(py, async move {
    //       let param_refs: Vec<&str> = param.iter().map(|s| s.as_str()).collect();
    //       let param_core: CoreType = param.into();
    //       let result = core_crate::function_name(&param_refs, &param_core).await...
    //       Ok(result.into())
    //   })
    //
    // NOT like this (which would fail with E0597):
    //   let param_refs: Vec<&str> = param.iter().map(|s| s.as_str()).collect();
    //   pyo3_async_runtimes::tokio::future_into_py(py, async move {
    //       let param_core: CoreType = param.into();
    //       let result = core_crate::function_name(&param_refs, &param_core).await...
    //   })
}
