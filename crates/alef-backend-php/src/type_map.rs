use ahash::AHashSet;
use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for ext-php-rs bindings.
/// PHP integers are signed, so U64/Usize/Isize map to i64.
/// JSON is handled as String.
/// Enum named types map to String (ext-php-rs does not support Rust enums as PHP
/// types; they are represented as string constants instead).
pub struct PhpMapper {
    /// Names of enum types in the API surface. These are mapped to String.
    pub enum_names: AHashSet<String>,
}

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

    /// Map enum types to String for PHP.
    /// ext-php-rs does not support Rust enums as PHP types; enum fields are
    /// represented as strings and paired with the generated string constants.
    /// Struct (class) types pass through unchanged so PHP can pass objects.
    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        if self.enum_names.contains(name) {
            Cow::Borrowed("String")
        } else {
            Cow::Borrowed(name)
        }
    }

    fn error_wrapper(&self) -> &str {
        "PhpResult"
    }
}
