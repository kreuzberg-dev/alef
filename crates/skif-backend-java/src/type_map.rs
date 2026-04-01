use skif_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to its Java type representation.
pub fn java_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => java_primitive(prim),
        TypeRef::String => "String".to_string(),
        TypeRef::Bytes => "byte[]".to_string(),
        TypeRef::Optional(inner) => java_boxed_type(inner),
        TypeRef::Vec(inner) => {
            let inner_type = java_boxed_type(inner);
            format!("java.util.List<{}>", inner_type)
        }
        TypeRef::Map(k, v) => {
            let key_type = java_boxed_type(k);
            let val_type = java_boxed_type(v);
            format!("java.util.Map<{}, {}>", key_type, val_type)
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "java.nio.file.Path".to_string(),
        TypeRef::Unit => "void".to_string(),
        TypeRef::Json => "String".to_string(),
    }
}

/// Maps a TypeRef to its Java boxed type (for Optional/null-safe contexts).
pub fn java_boxed_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => match prim {
            PrimitiveType::Bool => "Boolean".to_string(),
            PrimitiveType::U8 | PrimitiveType::I8 => "Byte".to_string(),
            PrimitiveType::U16 | PrimitiveType::I16 => "Short".to_string(),
            PrimitiveType::U32 | PrimitiveType::I32 => "Integer".to_string(),
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => "Long".to_string(),
            PrimitiveType::F32 => "Float".to_string(),
            PrimitiveType::F64 => "Double".to_string(),
        },
        TypeRef::String => "String".to_string(),
        TypeRef::Bytes => "byte[]".to_string(),
        TypeRef::Optional(inner) => java_boxed_type(inner),
        TypeRef::Vec(inner) => {
            let inner_type = java_boxed_type(inner);
            format!("java.util.List<{}>", inner_type)
        }
        TypeRef::Map(k, v) => {
            let key_type = java_boxed_type(k);
            let val_type = java_boxed_type(v);
            format!("java.util.Map<{}, {}>", key_type, val_type)
        }
        TypeRef::Named(name) => name.clone(),
        TypeRef::Path => "java.nio.file.Path".to_string(),
        TypeRef::Unit => "Void".to_string(),
        TypeRef::Json => "String".to_string(),
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

fn java_primitive(prim: &PrimitiveType) -> String {
    match prim {
        PrimitiveType::Bool => "boolean".to_string(),
        PrimitiveType::U8 | PrimitiveType::I8 => "byte".to_string(),
        PrimitiveType::U16 | PrimitiveType::I16 => "short".to_string(),
        PrimitiveType::U32 | PrimitiveType::I32 => "int".to_string(),
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize => "long".to_string(),
        PrimitiveType::F32 => "float".to_string(),
        PrimitiveType::F64 => "double".to_string(),
    }
}
