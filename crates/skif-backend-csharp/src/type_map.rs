use skif_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to its C# type representation.
pub fn csharp_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "byte".to_string(),
            PrimitiveType::U16 => "ushort".to_string(),
            PrimitiveType::U32 => "uint".to_string(),
            PrimitiveType::U64 => "ulong".to_string(),
            PrimitiveType::I8 => "sbyte".to_string(),
            PrimitiveType::I16 => "short".to_string(),
            PrimitiveType::I32 => "int".to_string(),
            PrimitiveType::I64 => "long".to_string(),
            PrimitiveType::F32 => "float".to_string(),
            PrimitiveType::F64 => "double".to_string(),
            PrimitiveType::Usize => "nuint".to_string(),
            PrimitiveType::Isize => "nint".to_string(),
        },
        TypeRef::String => "string".to_string(),
        TypeRef::Bytes => "byte[]".to_string(),
        TypeRef::Optional(inner) => format!("{}?", csharp_type(inner)),
        TypeRef::Vec(inner) => format!("List<{}>", csharp_type(inner)),
        TypeRef::Map(k, v) => {
            format!("Dictionary<{}, {}>", csharp_type(k), csharp_type(v))
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "string".to_string(),
        TypeRef::Json => "string".to_string(),
        TypeRef::Unit => "void".to_string(),
    }
}

/// Maps a TypeRef to its C# type for nullable values.
#[allow(dead_code)]
pub fn csharp_nullable_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Optional(_) => csharp_type(ty),
        _ => format!("{}?", csharp_type(ty)),
    }
}

/// Returns the default value for a type in C#.
pub fn csharp_default_value(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(PrimitiveType::Bool) => "false".to_string(),
        TypeRef::Primitive(_) => "default".to_string(),
        TypeRef::String => "null".to_string(),
        TypeRef::Bytes => "null".to_string(),
        TypeRef::Optional(_) => "null".to_string(),
        TypeRef::Vec(_) => "new List<>()".to_string(),
        TypeRef::Map(_, _) => "new Dictionary<,>()".to_string(),
        TypeRef::Named(_) => "null".to_string(),
        TypeRef::Path => "null".to_string(),
        TypeRef::Json => "null".to_string(),
        TypeRef::Unit => "".to_string(),
    }
}
