use skif_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to the C FFI parameter type (input position).
pub fn c_param_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => c_primitive(prim),
        TypeRef::String => "*const std::ffi::c_char".to_string(),
        TypeRef::Bytes => "*const u8".to_string(),
        TypeRef::Optional(inner) => {
            // Optional params use nullable pointers or sentinel values
            match inner.as_ref() {
                TypeRef::Primitive(PrimitiveType::Bool) => "i32".to_string(), // -1 = None, 0 = false, 1 = true
                TypeRef::Primitive(_) => c_param_type(inner),                 // caller uses sentinel
                TypeRef::String | TypeRef::Path | TypeRef::Json => {
                    "*const std::ffi::c_char".to_string() // null = None
                }
                TypeRef::Named(_) => format!("*const {}", c_param_type(inner)), // null = None
                _ => "*const std::ffi::c_char".to_string(),                     // fallback: JSON string, null = None
            }
        }
        TypeRef::Vec(_) => "*const std::ffi::c_char".to_string(), // JSON array string
        TypeRef::Map(_, _) => "*const std::ffi::c_char".to_string(), // JSON object string
        TypeRef::Named(name) => format!("*const {name}"),
        TypeRef::Path => "*const std::ffi::c_char".to_string(),
        TypeRef::Unit => "".to_string(),
        TypeRef::Json => "*const std::ffi::c_char".to_string(),
    }
}

/// Maps a TypeRef to the C FFI return type (output position).
pub fn c_return_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Primitive(prim) => c_primitive(prim),
        TypeRef::String => "*mut std::ffi::c_char".to_string(),
        TypeRef::Bytes => "*mut u8".to_string(), // paired with out-param length
        TypeRef::Optional(inner) => {
            // Optional returns use nullable pointers
            match inner.as_ref() {
                TypeRef::Primitive(PrimitiveType::Bool) => "i32".to_string(), // -1 = None
                TypeRef::Primitive(_) => c_return_type(inner),
                TypeRef::String | TypeRef::Path | TypeRef::Json => "*mut std::ffi::c_char".to_string(),
                TypeRef::Named(name) => format!("*mut {name}"),
                _ => "*mut std::ffi::c_char".to_string(),
            }
        }
        TypeRef::Vec(_) => "*mut std::ffi::c_char".to_string(), // JSON array string
        TypeRef::Map(_, _) => "*mut std::ffi::c_char".to_string(), // JSON object string
        TypeRef::Named(name) => format!("*mut {name}"),
        TypeRef::Path => "*mut std::ffi::c_char".to_string(),
        TypeRef::Unit => "()".to_string(),
        TypeRef::Json => "*mut std::ffi::c_char".to_string(),
    }
}

/// Maps a primitive type to its C FFI equivalent.
fn c_primitive(prim: &PrimitiveType) -> String {
    match prim {
        PrimitiveType::Bool => "i32".to_string(),
        PrimitiveType::U8 => "u8".to_string(),
        PrimitiveType::U16 => "u16".to_string(),
        PrimitiveType::U32 => "u32".to_string(),
        PrimitiveType::U64 => "u64".to_string(),
        PrimitiveType::I8 => "i8".to_string(),
        PrimitiveType::I16 => "i16".to_string(),
        PrimitiveType::I32 => "i32".to_string(),
        PrimitiveType::I64 => "i64".to_string(),
        PrimitiveType::F32 => "f32".to_string(),
        PrimitiveType::F64 => "f64".to_string(),
        PrimitiveType::Usize => "usize".to_string(),
        PrimitiveType::Isize => "isize".to_string(),
    }
}

/// Returns `true` if the type is a pointer type in FFI (needs null-check).
/// Useful for downstream backends (Go, Java, C#) that consume the FFI layer.
#[allow(dead_code)]
pub fn is_pointer_type(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::String
            | TypeRef::Bytes
            | TypeRef::Path
            | TypeRef::Json
            | TypeRef::Named(_)
            | TypeRef::Vec(_)
            | TypeRef::Map(_, _)
    )
}

/// Returns `true` if the return type is void in C.
#[allow(dead_code)]
pub fn is_void_return(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Unit)
}
