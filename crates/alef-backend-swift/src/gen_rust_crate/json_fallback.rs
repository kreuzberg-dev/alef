//! Shared predicate for deciding whether a serde-enabled DTO needs a JSON-roundtrip
//! `*_from_json` shim emitted on the Rust crate side.
//!
//! Both the Swift binding side (`gen_bindings::emit_into_rust_direct_call`) and the
//! Rust crate side (`gen_rust_crate::emit_lib_rs`) must agree on which types fall
//! through to the JSON fallback path. If they disagree, Swift can emit a
//! `RustBridge.{type}FromJson(json)` call with no matching Rust shim — yielding a
//! link error against the swift-bridge module.

use super::extern_block::has_constructor_extern;
use alef_core::ir::TypeDef;
use std::collections::HashSet;

/// Returns `true` when Swift's `intoRust()` for `ty` must fall back to a
/// `JSONEncoder` roundtrip via `{type_snake}_from_json` instead of calling the
/// direct positional constructor extern.
///
/// A type needs the JSON fallback exactly when the swift-bridge `extern "Rust"`
/// block has *no* `#[swift_bridge(init)] fn new(...)` declaration — captured by
/// the inverse of `has_constructor_extern`.
///
/// `exclude_fields` is the flat `TypeName.field_name` exclude set from the resolved
/// crate config — same shape passed to `has_constructor_extern`.
pub(crate) fn requires_json_fallback(ty: &TypeDef, exclude_fields: &HashSet<String>) -> bool {
    !has_constructor_extern(ty, exclude_fields)
}

/// Collect all serde-enabled, non-opaque, non-trait DTOs from `visible_types` that
/// will fall through to the JSON-roundtrip path on the Swift side, so the Rust
/// crate can emit matching `{type_snake}_from_json` shims.
pub(crate) fn collect_json_fallback_types<'a>(
    visible_types: &[&'a TypeDef],
    exclude_fields: &HashSet<String>,
) -> Vec<&'a TypeDef> {
    visible_types
        .iter()
        .copied()
        .filter(|ty| ty.has_serde && !ty.is_opaque && !ty.is_trait)
        .filter(|ty| requires_json_fallback(ty, exclude_fields))
        .collect()
}
