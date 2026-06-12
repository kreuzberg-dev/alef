use super::gen_from_binding_to_core;
use super::gen_from_binding_to_core_cfg;
use crate::codegen::conversions::ConversionConfig;
use crate::core::ir::{CoreWrapper, DefaultValue, FieldDef, TypeDef, TypeRef};
use ahash::AHashSet;

fn type_with_field(field: FieldDef) -> TypeDef {
    TypeDef {
        name: "ProcessConfig".to_string(),
        rust_path: "crate::ProcessConfig".to_string(),
        original_rust_path: String::new(),
        fields: vec![field],
        methods: vec![],
        is_opaque: false,
        is_clone: true,
        is_copy: false,
        doc: String::new(),
        cfg: None,
        is_trait: false,
        has_default: true,
        has_stripped_cfg_fields: false,
        is_return_type: false,
        serde_rename_all: None,
        has_serde: true,
        super_traits: vec![],
        binding_excluded: false,
        binding_exclusion_reason: None,
        is_variant_wrapper: false,
        has_lifetime_params: false,
        version: Default::default(),
    }
}

#[test]
fn sanitized_cow_string_field_converts_to_core() {
    let field = FieldDef {
        name: "language".to_string(),
        ty: TypeRef::String,
        optional: false,
        default: None,
        doc: String::new(),
        sanitized: true,
        is_boxed: false,
        type_rust_path: None,
        cfg: None,
        typed_default: Some(DefaultValue::Empty),
        core_wrapper: CoreWrapper::Cow,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
        serde_rename: None,
        serde_flatten: false,
        binding_excluded: false,
        binding_exclusion_reason: None,
        original_type: None,
    };

    let out = gen_from_binding_to_core(&type_with_field(field), "crate");

    assert!(out.contains("language: val.language.into()"));
    assert!(!out.contains("language: Default::default()"));
}

#[test]
fn binding_excluded_cfg_field_is_not_emitted_into_core_literal() {
    let field = FieldDef {
        name: "di_container".to_string(),
        ty: TypeRef::String,
        optional: true,
        default: None,
        doc: String::new(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: Some("feature = \"di\"".to_string()),
        typed_default: None,
        core_wrapper: CoreWrapper::None,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
        serde_rename: None,
        serde_flatten: false,
        binding_excluded: true,
        binding_exclusion_reason: Some("internal implementation detail".to_string()),
        original_type: None,
    };
    let mut typ = type_with_field(field);
    typ.has_stripped_cfg_fields = true;

    let out = gen_from_binding_to_core(&typ, "crate");

    assert!(
        !out.contains("di_container:"),
        "cfg-gated binding-excluded fields may not exist in the core struct; got:\n{out}"
    );
    assert!(
        out.contains("..Default::default()"),
        "stripped cfg fields should be filled by the default update; got:\n{out}"
    );
}

/// Trait-bridge OptionsField field with Arc wrapper: the binding→core From impl must
/// emit `val.visitor.map(|v| (*v.inner).clone())` and must NOT fall back to
/// `visitor: Default::default()`, which would silently drop the visitor handle.
#[test]
fn trait_bridge_arc_wrapper_field_forwards_value_not_default() {
    let opaque_type_name = "VisitorHandle".to_string();
    let mut opaque_set = AHashSet::new();
    opaque_set.insert(opaque_type_name.clone());

    let field = FieldDef {
        name: "visitor".to_string(),
        ty: TypeRef::Named(opaque_type_name.clone()),
        optional: true,
        default: None,
        doc: String::new(),
        sanitized: false,
        is_boxed: false,
        type_rust_path: None,
        cfg: Some("feature = \"visitor\"".to_string()),
        typed_default: None,
        core_wrapper: CoreWrapper::None,
        vec_inner_core_wrapper: CoreWrapper::None,
        newtype_wrapper: None,
        serde_rename: None,
        serde_flatten: false,
        binding_excluded: false,
        binding_exclusion_reason: None,
        original_type: None,
    };

    let never_skip = vec!["visitor".to_string()];
    let arc_wrapper = vec!["visitor".to_string()];

    let config = ConversionConfig {
        opaque_types: Some(&opaque_set),
        never_skip_cfg_field_names: &never_skip,
        trait_bridge_arc_wrapper_field_names: &arc_wrapper,
        ..ConversionConfig::default()
    };

    let out = gen_from_binding_to_core_cfg(&type_with_field(field), "crate", &config);

    assert!(
        out.contains("val.visitor.map(|v| (*v.inner).clone())"),
        "expected arc-wrapper clone forwarding, got:\n{out}"
    );
    assert!(
        !out.contains("visitor: Default::default()"),
        "must not emit Default::default() for arc-wrapper trait-bridge field, got:\n{out}"
    );
}

/// When `trait_bridge_arc_wrapper_field_names` is empty (default), the old
/// `Default::default()` fallback is preserved for opaque-no-wrapper fields.
#[test]
fn opaque_no_wrapper_field_without_arc_flag_emits_default() {
    let opaque_type_name = "OpaqueHandle".to_string();
    let mut opaque_set = AHashSet::new();
    opaque_set.insert(opaque_type_name.clone());

    let field = FieldDef {
        name: "handle".to_string(),
        ty: TypeRef::Named(opaque_type_name.clone()),
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
        serde_rename: None,
        serde_flatten: false,
        binding_excluded: false,
        binding_exclusion_reason: None,
        original_type: None,
    };

    let config = ConversionConfig {
        opaque_types: Some(&opaque_set),
        // trait_bridge_arc_wrapper_field_names left empty (default)
        ..ConversionConfig::default()
    };

    let out = gen_from_binding_to_core_cfg(&type_with_field(field), "crate", &config);

    assert!(
        out.contains("handle: Default::default()"),
        "expected Default::default() for non-arc-wrapper opaque field, got:\n{out}"
    );
    assert!(
        !out.contains("(*val.handle.inner).clone()"),
        "must not emit arc-clone for non-arc-wrapper opaque field, got:\n{out}"
    );
}
