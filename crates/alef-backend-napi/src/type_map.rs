use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for NAPI bindings.
/// JS numbers are 53-bit safe, so U64/Usize/Isize map to i64.
/// Named types get a configurable prefix (defaults to "Js").
pub struct NapiMapper {
    pub prefix: String,
}

impl NapiMapper {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }
}

impl TypeMapper for NapiMapper {
    fn primitive(&self, prim: &PrimitiveType) -> Cow<'static, str> {
        Cow::Borrowed(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "i64",
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::F32 => "f64", // NAPI-RS doesn't impl FromNapiValue for f32
            PrimitiveType::F64 => "f64",
            PrimitiveType::Usize => "i64",
            PrimitiveType::Isize => "i64",
        })
    }

    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("{}{name}", self.prefix))
    }

    /// NAPI uses i64 for Duration (JS numbers are 53-bit safe).
    fn duration(&self) -> Cow<'static, str> {
        Cow::Borrowed("i64")
    }

    /// NAPI doesn't implement FromNapiValue/ToNapiValue for serde_json::Value,
    /// so JSON is passed as a String and parsed on the JS side.
    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}
