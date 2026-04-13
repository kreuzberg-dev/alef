use std::borrow::Cow;

use alef_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to its Java type representation.
pub fn java_type(ty: &TypeRef) -> Cow<'static, str> {
    match ty {
        TypeRef::Primitive(prim) => java_primitive(prim),
        TypeRef::String | TypeRef::Char => Cow::Borrowed("String"),
        TypeRef::Bytes => Cow::Borrowed("byte[]"),
        TypeRef::Optional(inner) => java_boxed_type(inner),
        TypeRef::Vec(inner) => {
            let inner_type = java_boxed_type(inner);
            Cow::Owned(format!("List<{}>", inner_type))
        }
        TypeRef::Map(k, v) => {
            let key_type = java_boxed_type(k);
            let val_type = java_boxed_type(v);
            Cow::Owned(format!("Map<{}, {}>", key_type, val_type))
        }
        TypeRef::Named(name) => Cow::Owned(name.clone()),
        TypeRef::Path => Cow::Borrowed("java.nio.file.Path"),
        TypeRef::Unit => Cow::Borrowed("void"),
        TypeRef::Json => Cow::Borrowed("Object"),
        TypeRef::Duration => Cow::Borrowed("Long"),
    }
}

/// Maps a TypeRef to its Java boxed type (for Optional/null-safe contexts).
pub fn java_boxed_type(ty: &TypeRef) -> Cow<'static, str> {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => Cow::Borrowed("Boolean"),
            PrimitiveType::U8 | PrimitiveType::I8 => Cow::Borrowed("Byte"),
            PrimitiveType::U16 | PrimitiveType::I16 => Cow::Borrowed("Short"),
            PrimitiveType::U32 | PrimitiveType::I32 => Cow::Borrowed("Integer"),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => {
                Cow::Borrowed("Long")
            }
            PrimitiveType::F32 => Cow::Borrowed("Float"),
            PrimitiveType::F64 => Cow::Borrowed("Double"),
        },
        TypeRef::String | TypeRef::Char => Cow::Borrowed("String"),
        TypeRef::Bytes => Cow::Borrowed("byte[]"),
        TypeRef::Optional(inner) => java_boxed_type(inner),
        TypeRef::Vec(inner) => {
            let inner_type = java_boxed_type(inner);
            Cow::Owned(format!("List<{}>", inner_type))
        }
        TypeRef::Map(k, v) => {
            let key_type = java_boxed_type(k);
            let val_type = java_boxed_type(v);
            Cow::Owned(format!("Map<{}, {}>", key_type, val_type))
        }
        TypeRef::Named(name) => Cow::Owned(name.clone()),
        TypeRef::Path => Cow::Borrowed("java.nio.file.Path"),
        TypeRef::Unit => Cow::Borrowed("Void"),
        TypeRef::Json => Cow::Borrowed("Object"),
        TypeRef::Duration => Cow::Borrowed("Long"),
    }
}

/// Maps a primitive type to its Java FFI equivalent (Panama FFM ValueLayout).
pub fn java_ffi_type(prim: &PrimitiveType) -> &'static str {
    match prim {
        PrimitiveType::Bool => "ValueLayout.JAVA_BOOLEAN",
        PrimitiveType::U8 | PrimitiveType::I8 => "ValueLayout.JAVA_BYTE",
        PrimitiveType::U16 | PrimitiveType::I16 => "ValueLayout.JAVA_SHORT",
        PrimitiveType::U32 | PrimitiveType::I32 => "ValueLayout.JAVA_INT",
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => {
            "ValueLayout.JAVA_LONG"
        }
        PrimitiveType::F32 => "ValueLayout.JAVA_FLOAT",
        PrimitiveType::F64 => "ValueLayout.JAVA_DOUBLE",
    }
}

fn java_primitive(prim: &PrimitiveType) -> Cow<'static, str> {
    match prim {
        PrimitiveType::Bool => Cow::Borrowed("boolean"),
        PrimitiveType::U8 | PrimitiveType::I8 => Cow::Borrowed("byte"),
        PrimitiveType::U16 | PrimitiveType::I16 => Cow::Borrowed("short"),
        PrimitiveType::U32 | PrimitiveType::I32 => Cow::Borrowed("int"),
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => Cow::Borrowed("long"),
        PrimitiveType::F32 => Cow::Borrowed("float"),
        PrimitiveType::F64 => Cow::Borrowed("double"),
    }
}
