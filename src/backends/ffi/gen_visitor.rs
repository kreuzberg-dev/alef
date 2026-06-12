/// Generate visitor/callback FFI bindings.
///
/// This module produces the `#[repr(C)]` callback struct, an opaque `Visitor`
/// handle that bridges C function pointers into the Rust visitor trait,
/// and the three public FFI entry points:
///
/// - `{prefix}_visitor_create(callbacks: *const {Prefix}VisitorCallbacks) -> *mut {Prefix}Visitor`
/// - `{prefix}_visitor_free(visitor: *mut {Prefix}Visitor)`
/// - `{prefix}_options_set_visitor_handle(options, visitor)` — attach visitor to options before `{prefix}_convert`
///
/// # Coverage
///
/// All compatible visitor trait methods are covered. The callback struct field
/// order matches the trait definition order (and therefore the Go binding's
/// expected layout).
mod binding_emission;
mod callback_specs;
mod context;
mod legacy_conversion;
mod protocol;
#[cfg(test)]
mod tests;
mod visitor_refs;

#[cfg(test)]
pub use binding_emission::gen_visitor_bindings;
pub use binding_emission::gen_visitor_bindings_with_api;
#[allow(unused_imports)]
pub(crate) use callback_specs::{CallbackSpec, callback_specs_from_trait};
pub use legacy_conversion::gen_convert_no_visitor;
