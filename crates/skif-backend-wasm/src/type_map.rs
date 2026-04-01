use skif_codegen::type_mapper::TypeMapper;
use std::borrow::Cow;
use std::collections::HashMap;

/// TypeMapper for WASM bindings with configurable type overrides.
pub struct WasmMapper {
    pub overrides: HashMap<String, String>,
}

impl WasmMapper {
    pub fn new(overrides: HashMap<String, String>) -> Self {
        Self { overrides }
    }
}

impl TypeMapper for WasmMapper {
    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        match self.overrides.get(name) {
            Some(override_ty) => Cow::Owned(override_ty.clone()),
            None => Cow::Borrowed(name),
        }
    }

    fn path(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("JsValue")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }

    /// WASM wraps errors as `Result<T, JsValue>`.
    fn wrap_return(&self, base: &str, has_error: bool) -> String {
        if has_error {
            format!("Result<{base}, JsValue>")
        } else {
            base.to_string()
        }
    }
}
