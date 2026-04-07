use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::{PrimitiveType, TypeRef};
use std::borrow::Cow;

/// TypeMapper for PyO3 bindings — uses Rust defaults except for Json.
pub struct Pyo3Mapper;

impl TypeMapper for Pyo3Mapper {
    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String") // JSON as string, user deserializes
    }

    fn error_wrapper(&self) -> &str {
        "PyResult"
    }
}

/// Maps a TypeRef to its Python representation for .pyi stubs.
pub fn python_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8
            | PrimitiveType::U16
            | PrimitiveType::U32
            | PrimitiveType::U64
            | PrimitiveType::I8
            | PrimitiveType::I16
            | PrimitiveType::I32
            | PrimitiveType::I64
            | PrimitiveType::Usize
            | PrimitiveType::Isize => "int".to_string(),
            PrimitiveType::F32 | PrimitiveType::F64 => "float".to_string(),
        },
        TypeRef::String => "str".to_string(),
        TypeRef::Bytes => "bytes".to_string(),
        TypeRef::Optional(inner) => format!("{} | None", python_type(inner)),
        TypeRef::Vec(inner) => format!("list[{}]", python_type(inner)),
        TypeRef::Map(k, v) => {
            format!("dict[{}, {}]", python_type(k), python_type(v))
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "str".to_string(),
        TypeRef::Json => "dict[str, Any]".to_string(),
        TypeRef::Unit => "None".to_string(),
        TypeRef::Duration => "int".to_string(),
    }
}
