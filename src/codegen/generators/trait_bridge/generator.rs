use super::TraitBridgeSpec;
use crate::core::ir::MethodDef;

/// Backend-specific trait bridge generation.
///
/// Each binding backend (PyO3, NAPI-RS, wasm-bindgen, etc.) implements this trait
/// to provide the language-specific parts of bridge codegen. The shared functions
/// in this module call these methods to fill in the backend-dependent pieces.
pub trait TraitBridgeGenerator {
    /// The type of the wrapped foreign object (e.g., `"Py<PyAny>"`, `"ThreadsafeFunction"`).
    fn foreign_object_type(&self) -> &str;

    /// Additional `use` imports needed for the bridge code.
    fn bridge_imports(&self) -> Vec<String>;

    /// Generate the body of a synchronous method bridge.
    ///
    /// The returned string is inserted inside the trait impl method. It should
    /// call through to the foreign object and convert the result.
    fn gen_sync_method_body(&self, method: &MethodDef, spec: &TraitBridgeSpec) -> String;

    /// Generate the body of an async method bridge.
    ///
    /// The returned string is the body of a `Box::pin(async move { ... })` block.
    fn gen_async_method_body(&self, method: &MethodDef, spec: &TraitBridgeSpec) -> String;

    /// Generate the constructor body that validates and wraps the foreign object.
    ///
    /// Should check that the foreign object provides all required methods and
    /// return `Self { ... }` on success.
    fn gen_constructor(&self, spec: &TraitBridgeSpec) -> String;

    /// Generate the complete registration function including attributes, signature, and body.
    ///
    /// Each backend needs different function signatures (PyO3 takes `py: Python`,
    /// NAPI takes `#[napi]` with JS params, FFI takes `extern "C"` with raw pointers),
    /// so the generator owns the full function.
    fn gen_registration_fn(&self, spec: &TraitBridgeSpec) -> String;

    /// Generate an unregistration function for the bridge.
    ///
    /// Default implementation returns an empty string — backends opt in by
    /// emitting a function whose name is `spec.bridge_config.unregister_fn`
    /// (when set) and whose body calls into the host crate's
    /// `unregister_*(name)` plugin entry point.
    fn gen_unregistration_fn(&self, _spec: &TraitBridgeSpec) -> String {
        String::new()
    }

    /// Generate a clear-all-plugins function for the bridge.
    ///
    /// Default implementation returns an empty string — backends opt in by
    /// emitting a function whose name is `spec.bridge_config.clear_fn`
    /// (when set) and whose body calls into the host crate's `clear_*()`
    /// plugin entry point. Typically used in test teardown.
    fn gen_clear_fn(&self, _spec: &TraitBridgeSpec) -> String {
        String::new()
    }

    /// Whether the `#[async_trait]` macro should require `Send` on its futures.
    ///
    /// Returns `true` (default) for most targets. WASM is single-threaded so its
    /// trait bounds don't include `Send`; implementors should return `false` there.
    fn async_trait_is_send(&self) -> bool {
        true
    }
}
