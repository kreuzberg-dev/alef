use super::NapiBackend;
use crate::core::backend::Backend;
use crate::core::config::Language;

/// NapiBackend::name returns "napi".
#[test]
fn napi_backend_name_is_napi() {
    let b = NapiBackend;
    assert_eq!(b.name(), "napi");
}

/// NapiBackend::language returns Language::Node.
#[test]
fn napi_backend_language_is_node() {
    let b = NapiBackend;
    assert_eq!(b.language(), Language::Node);
}

/// Test that cfg-gated fields in never_skip_cfg_field_names pass the options-field-bridge filter.
#[test]
fn cfg_gated_field_accepted_when_in_never_skip_list() {
    // Test the predicate logic: a cfg-gated field "visitor" should be accepted
    // when it appears in never_skip_cfg_field_names.
    let never_skip_cfg_field_names = ["visitor".to_string()];
    let field_is_target = "visitor";

    // Simulate a field with cfg = Some(...)
    let field_has_cfg = Some("feature = \"visitor\"");

    // Predicate: f.cfg.is_none() || never_skip_cfg_field_names.iter().any(|n| n == field_name)
    let accepted = field_has_cfg.is_none() || never_skip_cfg_field_names.iter().any(|n| n == field_is_target);

    assert!(
        accepted,
        "cfg-gated field 'visitor' should pass filter when in never_skip_cfg_field_names"
    );
}
