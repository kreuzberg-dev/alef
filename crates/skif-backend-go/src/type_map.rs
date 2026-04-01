use skif_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to its Go type representation.
/// Used for non-optional types in general contexts.
pub fn go_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => go_primitive(prim),
        TypeRef::String => "string".to_string(),
        TypeRef::Bytes => "[]byte".to_string(),
        TypeRef::Optional(inner) => format!("*{}", go_type(inner)),
        TypeRef::Vec(inner) => format!("[]{}", go_type(inner)),
        TypeRef::Map(k, v) => {
            format!("map[{}]{}", go_type(k), go_type(v))
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "string".to_string(),
        TypeRef::Json => "map[string]interface{}".to_string(),
        TypeRef::Unit => "".to_string(), // void
    }
}

/// Maps a TypeRef to its optional Go type representation (pointer for option).
pub fn go_optional_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Optional(_) => go_type(ty),
        _ => format!("*{}", go_type(ty)),
    }
}

/// Maps a primitive type to its Go equivalent.
fn go_primitive(prim: &PrimitiveType) -> String {
    match prim {
        PrimitiveType::Bool => "bool".to_string(),
        PrimitiveType::U8 => "uint8".to_string(),
        PrimitiveType::U16 => "uint16".to_string(),
        PrimitiveType::U32 => "uint32".to_string(),
        PrimitiveType::U64 => "uint64".to_string(),
        PrimitiveType::I8 => "int8".to_string(),
        PrimitiveType::I16 => "int16".to_string(),
        PrimitiveType::I32 => "int32".to_string(),
        PrimitiveType::I64 => "int64".to_string(),
        PrimitiveType::F32 => "float32".to_string(),
        PrimitiveType::F64 => "float64".to_string(),
        PrimitiveType::Usize => "uint".to_string(),
        PrimitiveType::Isize => "int".to_string(),
    }
}
