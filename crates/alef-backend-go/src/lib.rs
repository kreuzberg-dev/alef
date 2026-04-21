//! Go (cgo) binding generator backend for alef.

mod gen_bindings;
mod gen_visitor;
mod type_map;

pub use gen_bindings::GoBackend;
