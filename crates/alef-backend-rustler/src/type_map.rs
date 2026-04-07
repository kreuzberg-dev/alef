use alef_codegen::type_mapper::TypeMapper;
use std::borrow::Cow;

/// TypeMapper for Rustler/Elixir NIFs — default Rust types with String for Json.
pub struct RustlerMapper;

impl TypeMapper for RustlerMapper {
    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }

    /// Rustler wraps errors as `Result<T, String>`.
    fn wrap_return(&self, base: &str, has_error: bool) -> String {
        if has_error {
            format!("Result<{base}, String>")
        } else {
            base.to_string()
        }
    }
}
