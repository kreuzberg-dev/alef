//! C FFI binding generator backend for alef.

mod gen_bindings;
mod gen_visitor;
pub mod trait_bridge;
mod type_map;

pub use gen_bindings::FfiBackend;
