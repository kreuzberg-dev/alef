//! Python (PyO3) binding generator backend for alef.

mod gen_bindings;
mod gen_stubs;
mod type_map;

pub use gen_bindings::Pyo3Backend;
