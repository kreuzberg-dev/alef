//! Emits the **inbound** plugin trait bridge — Swift implements a Rust trait, Rust calls back.
//!
//! Whereas [`trait_bridge`](super::trait_bridge) generates **outbound** glue (Swift caller →
//! Rust trait object), this module generates the inverse: a Swift class conforms to a
//! protocol, Rust holds a handle, and Rust calls each method on the Swift instance via
//! `extern "Swift"` declarations.
//!
//! This facade preserves the historical `gen_rust_crate::plugin_inbound` module path
//! while keeping inbound generation split by concern.

mod inbound_externs;
mod method_impls;
mod options_fields;
mod wrappers;

use crate::backends::swift::gen_rust_crate::type_bridge::{needs_json_bridge, swift_bridge_rust_type};
use crate::core::config::TraitBridgeConfig;
use crate::core::ir::TypeRef;

pub(crate) use inbound_externs::{emit_extern_block_for_inbound, emit_extern_block_for_inbound_registration};
pub(crate) use options_fields::{
    emit_options_field_factory, emit_options_field_from_impls, emit_options_field_options_helper,
};
pub(crate) use wrappers::{emit_inbound_wrapper, emit_plugin_error_helper};

/// Inbound-specific type bridging.
///
/// All `Named` types are JSON-bridged at the inbound boundary because the Swift side of an
/// `extern "Swift"` shim cannot produce the opaque Rust newtype the way `extern "Rust"`
/// callers do; it has to send a JSON payload that Rust deserialises into the source type.
/// Primitive scalars, `String`, `Vec<u8>`, and `Vec<leaf>` pass through as-is.
pub(super) fn inbound_bridge_type(ty: &TypeRef) -> String {
    if needs_inbound_json_bridge(ty) {
        return "String".to_string();
    }
    match ty {
        TypeRef::Vec(inner) => format!("Vec<{}>", inbound_bridge_type(inner)),
        _ => swift_bridge_rust_type(ty),
    }
}

/// Like [`needs_json_bridge`] but additionally treats every `Named` type as JSON-bridged
/// for inbound transport. Vec<Named-leaf> stays a typed Vec (e.g. `Vec<String>`) when
/// the inner type is a primitive/leaf — only Named-leaf gets escalated.
pub(super) fn needs_inbound_json_bridge(ty: &TypeRef) -> bool {
    if needs_json_bridge(ty) {
        return true;
    }
    matches!(ty, TypeRef::Named(_))
}

/// Returns true when the trait bridge config declares a Plugin super-trait.
pub(super) fn has_plugin_super(bridge_config: &TraitBridgeConfig) -> bool {
    bridge_config
        .super_trait
        .as_deref()
        .map(|s| s == "Plugin" || s.ends_with("::Plugin"))
        .unwrap_or(false)
}
