use super::*;

// ==============================================================================
// Tests for fluent-builder methods that consume self and return Self.
// Verifies that `(self, T) -> Self` shapes — including those with Json params —
// are auto-delegated by codegen instead of falling through to the unimplemented
// stub. Companion: shared::is_delegatable_param / is_simple_non_opaque_param.
// ==============================================================================

fn builder_method(name: &str, receiver: ReceiverKind, return_type: TypeRef, params: Vec<ParamDef>) -> MethodDef {
    MethodDef {
        name: name.to_string(),
        params,
        return_type,
        is_async: false,
        is_static: false,
        error_type: None,
        doc: String::new(),
        receiver: Some(receiver),
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

fn simple_param(name: &str, ty: TypeRef) -> ParamDef {
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
        core_wrapper: alef::core::ir::CoreWrapper::None,
    }
}

#[test]
fn test_fluent_builder_owned_self_returning_self_delegates() {
    // `pub fn with_count(self, count: u32) -> Self` on a non-opaque Clone type
    // must auto-delegate via the "builder for non-opaque types" branch.
    let typ = simple_type_def();
    let method = builder_method(
        "with_count",
        ReceiverKind::Owned,
        TypeRef::Named("MyConfig".to_string()),
        vec![simple_param("count", TypeRef::Primitive(PrimitiveType::U32))],
    );

    let result = gen_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        false,
        &AHashSet::new(),
        &AHashSet::new(),
        &AdapterBodies::default(),
    );

    assert!(result.contains("pub fn with_count"), "should emit method");
    assert!(
        !result.contains("unimplemented!()") && !result.contains("compile_error!"),
        "fluent builder taking self should be delegated, not stubbed: {result}"
    );
    assert!(
        result.contains("core_self.with_count(count)"),
        "should call core method on core_self with the count arg"
    );
    assert!(
        result.contains(".into()"),
        "should convert mutated core back into binding Self"
    );
}

#[test]
fn test_fluent_builder_owned_self_returning_named_type_delegates() {
    // Same body as the Self-return case but the return type is the parent's
    // concrete name (e.g. `pub fn with_count(self, count: u32) -> MyConfig`).
    // Extractor's resolve_self_refs turns `Self` into the parent name, so both
    // forms reach codegen identically — this guards that path.
    let typ = simple_type_def();
    let method = builder_method(
        "with_count",
        ReceiverKind::Owned,
        TypeRef::Named("MyConfig".to_string()),
        vec![simple_param("count", TypeRef::Primitive(PrimitiveType::U32))],
    );

    let result = gen_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        false,
        &AHashSet::new(),
        &AHashSet::new(),
        &AdapterBodies::default(),
    );

    assert!(
        result.contains("core_self.with_count(count)"),
        "should delegate to core method regardless of Self vs. concrete return name"
    );
    assert!(
        !result.contains("unimplemented!()"),
        "concrete-typed builder return must not stub out"
    );
}

#[test]
fn test_fluent_builder_owned_self_with_json_param_delegates() {
    // The motivating case: `pub fn with_extension(self, key: String, value: Value) -> Self`.
    // Previously rejected because TypeRef::Json was not in is_simple_non_opaque_param.
    let typ = simple_type_def();
    let method = builder_method(
        "with_extension",
        ReceiverKind::Owned,
        TypeRef::Named("MyConfig".to_string()),
        vec![
            simple_param("key", TypeRef::String),
            simple_param("value", TypeRef::Json),
        ],
    );

    let result = gen_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        false,
        &AHashSet::new(),
        &AHashSet::new(),
        &AdapterBodies::default(),
    );

    assert!(result.contains("pub fn with_extension"), "should emit method");
    assert!(
        !result.contains("unimplemented!()"),
        "Json param must no longer block builder delegation: {result}"
    );
    // RustMapper (a passthrough mapper used by these tests) maps Json to
    // `serde_json::Value`; real backends override it to `String`. Either way the
    // call site routes the value through `serde_json::from_str(&value)`, which
    // confirms the Json param participates in the auto-delegation pipeline.
    assert!(
        result.contains("serde_json::from_str(&value)"),
        "Json param should be parsed back into serde_json::Value at the call site: {result}"
    );
    assert!(
        result.contains(".with_extension(key, "),
        "should forward both args to the core builder: {result}"
    );
}

#[test]
fn test_fluent_builder_ref_mut_self_returning_ref_mut_self_delegates() {
    // `pub fn set_count(&mut self, count: u32) -> &mut Self` — in-place builder.
    // After type resolution this becomes RefMut receiver returning Named(parent_type).
    // Goes through the functional clone-mutate-return pattern (frozen PyO3 / immutable
    // WASM compatibility).
    let typ = simple_type_def();
    let method = builder_method(
        "set_count",
        ReceiverKind::RefMut,
        TypeRef::Named("MyConfig".to_string()),
        vec![simple_param("count", TypeRef::Primitive(PrimitiveType::U32))],
    );

    let result = gen_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        false,
        &AHashSet::new(),
        &AHashSet::new(),
        &AdapterBodies::default(),
    );

    assert!(result.contains("pub fn set_count"), "should emit method");
    assert!(
        !result.contains("unimplemented!()"),
        "RefMut builder returning &mut Self must delegate, not stub: {result}"
    );
    assert!(
        result.contains("&self"),
        "functional RefMut pattern rewrites &mut self to &self for frozen PyO3 / immutable WASM"
    );
}

#[test]
fn test_fluent_builder_owned_self_returning_different_type_is_not_a_builder() {
    // Negative: `pub fn into_thing(self) -> Thing` is NOT a Self-returning builder —
    // codegen should NOT misclassify it. The non-opaque builder branch is gated on
    // `return_type == Named(parent_type_name)`, so the body must fall back to the
    // generic delegation path (which here lands on `gen_unimplemented_body` because
    // `Thing` is an unknown Named type with no From impl available).
    let typ = simple_type_def();
    let method = builder_method(
        "into_thing",
        ReceiverKind::Owned,
        TypeRef::Named("Thing".to_string()),
        vec![],
    );

    let result = gen_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        false,
        &AHashSet::new(),
        &AHashSet::new(),
        &AdapterBodies::default(),
    );

    // The body must not contain `Self {` (the Self-construction marker of the opaque
    // builder branch) — the method returns a different type.
    assert!(
        !result.contains("Self {"),
        "method returning a different Named type must not be classified as a self-builder: {result}"
    );
}

#[test]
fn test_adapter_body_overrides_delegatable_json_static_method() {
    // Regression guard for the new "adapter-first" precedence: when a user provides
    // an explicit adapter override, it must win even if the codegen could otherwise
    // delegate the method. Previously the adapter check was inside `!can_delegate`,
    // so making Json delegatable would have silently dropped the override.
    let typ = simple_type_def();
    let method = MethodDef {
        name: "create_with_json".to_string(),
        params: vec![simple_param("payload", TypeRef::Json)],
        return_type: TypeRef::Named("MyConfig".to_string()),
        is_async: false,
        is_static: true,
        error_type: None,
        doc: String::new(),
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
    };
    let mut adapter_bodies = AdapterBodies::default();
    adapter_bodies.insert(
        "MyConfig.create_with_json".to_string(),
        "MyConfig::custom_create(payload)".to_string(),
    );

    let result = gen_static_method(
        &method,
        &RustMapper,
        &default_cfg(),
        &typ,
        &adapter_bodies,
        &AHashSet::new(),
        &AHashSet::new(),
    );

    assert!(
        result.contains("MyConfig::custom_create(payload)"),
        "adapter override must take precedence over delegation: {result}"
    );
    assert!(
        !result.contains("serde_json::from_str"),
        "adapter body should not be supplemented with auto-generated Json parsing: {result}"
    );
}
