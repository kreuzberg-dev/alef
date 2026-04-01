use skif_codegen::type_mapper::TypeMapper;
use skif_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for ext-php-rs bindings.
/// PHP integers are signed, so U64/Usize/Isize map to i64.
/// JSON is handled as String.
pub struct PhpMapper;

impl TypeMapper for PhpMapper {
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
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Usize => "i64",
            PrimitiveType::Isize => "i64",
        })
    }

    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn error_wrapper(&self) -> &str {
        "PhpResult"
    }
}
