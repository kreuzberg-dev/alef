//! Dart binding generator backend for alef.
//!
//! Phase 2A skeleton: registers `DartBackend` and exposes `BuildConfig`
//! with `BuildDependency::None`. Dart consumes the Rust library via
//! flutter_rust_bridge (FRB) — no separate C FFI layer required.
//! Real codegen (type emission, function wrappers) lands in Phase 2B.

mod gen_bindings;
mod type_map;

pub use gen_bindings::DartBackend;
