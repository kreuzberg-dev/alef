use std::borrow::Cow;

use skif_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to its C# type representation.
pub fn csharp_type(ty: &TypeRef) -> Cow<'static, str> {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => Cow::Borrowed("bool"),
            PrimitiveType::U8 => Cow::Borrowed("byte"),
            PrimitiveType::U16 => Cow::Borrowed("ushort"),
            PrimitiveType::U32 => Cow::Borrowed("uint"),
            PrimitiveType::U64 => Cow::Borrowed("ulong"),
            PrimitiveType::I8 => Cow::Borrowed("sbyte"),
            PrimitiveType::I16 => Cow::Borrowed("short"),
            PrimitiveType::I32 => Cow::Borrowed("int"),
            PrimitiveType::I64 => Cow::Borrowed("long"),
            PrimitiveType::F32 => Cow::Borrowed("float"),
            PrimitiveType::F64 => Cow::Borrowed("double"),
            PrimitiveType::Usize => Cow::Borrowed("nuint"),
            PrimitiveType::Isize => Cow::Borrowed("nint"),
        },
        TypeRef::String => Cow::Borrowed("string"),
        TypeRef::Bytes => Cow::Borrowed("byte[]"),
        TypeRef::Optional(inner) => Cow::Owned(format!("{}?", csharp_type(inner))),
        TypeRef::Vec(inner) => Cow::Owned(format!("List<{}>", csharp_type(inner))),
        TypeRef::Map(k, v) => Cow::Owned(format!("Dictionary<{}, {}>", csharp_type(k), csharp_type(v))),
        TypeRef::Named(name) => Cow::Owned(name.clone()),
        TypeRef::Path => Cow::Borrowed("string"),
        TypeRef::Json => Cow::Borrowed("string"),
        TypeRef::Unit => Cow::Borrowed("void"),
    }
}
