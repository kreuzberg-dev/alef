use skif_codegen::type_mapper::TypeMapper;
use skif_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for NAPI bindings.
/// JS numbers are 53-bit safe, so U64/Usize/Isize map to i64.
/// Named types get a "Js" prefix.
pub struct NapiMapper;

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
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Usize => "i64",
            PrimitiveType::Isize => "i64",
        })
    }

    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("Js{name}"))
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}

/// Maps a TypeRef to its JavaScript representation for type stubs.
#[allow(dead_code)]
pub fn javascript_type(ty: &skif_core::ir::TypeRef) -> String {
    use skif_core::ir::TypeRef;
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => "boolean".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "number".to_string(),
            _ => "number".to_string(),
        },
        TypeRef::String => "string".to_string(),
        TypeRef::Bytes => "Buffer".to_string(),
        TypeRef::Optional(inner) => format!("{} | null", javascript_type(inner)),
        TypeRef::Vec(inner) => format!("{}[]", javascript_type(inner)),
        TypeRef::Map(k, v) => {
            format!("Map<{}, {}>", javascript_type(k), javascript_type(v))
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "string".to_string(),
        TypeRef::Json => "Record<string, any>".to_string(),
        TypeRef::Unit => "void".to_string(),
    }
}
