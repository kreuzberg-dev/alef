//! WebAssembly (wasm-bindgen) binding generator backend for skif.
//!
//! Generates JavaScript-compatible WebAssembly bindings using the wasm-bindgen crate.
//! Supports configurable restriction handling through `WasmConfig`:
//! - `exclude_functions`: Skip generation of specific functions
//! - `exclude_types`: Skip generation of specific types
//! - `type_overrides`: Remap types (e.g., Path → String)

mod gen_bindings;
mod type_map;

pub use gen_bindings::WasmBackend;
