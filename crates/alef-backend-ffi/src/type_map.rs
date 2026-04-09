use std::borrow::Cow;

use alef_core::ir::{PrimitiveType, TypeRef};

/// Maps a TypeRef to the C FFI parameter type (input position).
pub fn c_param_type(ty: &TypeRef, core_import: &str) -> Cow<'static, str> {
    match ty {
        TypeRef::Primitive(prim) => c_primitive(prim),
        TypeRef::String | TypeRef::Char => Cow::Borrowed("*const std::ffi::c_char"),
        TypeRef::Bytes => Cow::Borrowed("*const u8"),
        TypeRef::Optional(inner) => {
            // Optional params use nullable pointers or sentinel values
            match inner.as_ref() {
                TypeRef::Primitive(PrimitiveType::Bool) => Cow::Borrowed("i32"), // -1 = None, 0 = false, 1 = true
                TypeRef::Primitive(_) => c_param_type(inner, core_import),       // caller uses sentinel
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => {
                    Cow::Borrowed("*const std::ffi::c_char") // null = None
                }
                TypeRef::Named(_) => Cow::Owned(format!("*const {}", c_param_type(inner, core_import))), // null = None
                _ => Cow::Borrowed("*const std::ffi::c_char"), // fallback: JSON string, null = None
            }
        }
        TypeRef::Vec(_) => Cow::Borrowed("*const std::ffi::c_char"), // JSON array string
        TypeRef::Map(_, _) => Cow::Borrowed("*const std::ffi::c_char"), // JSON object string
        TypeRef::Named(name) => Cow::Owned(format!("*const {core_import}::{name}")),
        TypeRef::Path => Cow::Borrowed("*const std::ffi::c_char"),
        TypeRef::Unit => Cow::Borrowed(""),
        TypeRef::Json => Cow::Borrowed("*const std::ffi::c_char"),
        TypeRef::Duration => Cow::Borrowed("u64"),
    }
}

/// Maps a TypeRef to the C FFI return type (output position).
pub fn c_return_type(ty: &TypeRef, core_import: &str) -> Cow<'static, str> {
    match ty {
        TypeRef::Primitive(prim) => c_primitive(prim),
        TypeRef::String | TypeRef::Char => Cow::Borrowed("*mut std::ffi::c_char"),
        TypeRef::Bytes => Cow::Borrowed("*mut u8"), // paired with out-param length
        TypeRef::Optional(inner) => {
            // Optional returns use nullable pointers
            match inner.as_ref() {
                TypeRef::Primitive(PrimitiveType::Bool) => Cow::Borrowed("i32"), // -1 = None
                TypeRef::Primitive(_) => c_return_type(inner, core_import),
                TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => {
                    Cow::Borrowed("*mut std::ffi::c_char")
                }
                TypeRef::Named(name) => Cow::Owned(format!("*mut {core_import}::{name}")),
                TypeRef::Duration => Cow::Borrowed("u64"), // 0 = None sentinel
                _ => Cow::Borrowed("*mut std::ffi::c_char"),
            }
        }
        TypeRef::Vec(_) => Cow::Borrowed("*mut std::ffi::c_char"), // JSON array string
        TypeRef::Map(_, _) => Cow::Borrowed("*mut std::ffi::c_char"), // JSON object string
        TypeRef::Named(name) => Cow::Owned(format!("*mut {core_import}::{name}")),
        TypeRef::Path => Cow::Borrowed("*mut std::ffi::c_char"),
        TypeRef::Unit => Cow::Borrowed("()"),
        TypeRef::Json => Cow::Borrowed("*mut std::ffi::c_char"),
        TypeRef::Duration => Cow::Borrowed("u64"),
    }
}

/// Maps a primitive type to its C FFI equivalent.
fn c_primitive(prim: &PrimitiveType) -> Cow<'static, str> {
    match prim {
        PrimitiveType::Bool => Cow::Borrowed("i32"),
        PrimitiveType::U8 => Cow::Borrowed("u8"),
        PrimitiveType::U16 => Cow::Borrowed("u16"),
        PrimitiveType::U32 => Cow::Borrowed("u32"),
        PrimitiveType::U64 => Cow::Borrowed("u64"),
        PrimitiveType::I8 => Cow::Borrowed("i8"),
        PrimitiveType::I16 => Cow::Borrowed("i16"),
        PrimitiveType::I32 => Cow::Borrowed("i32"),
        PrimitiveType::I64 => Cow::Borrowed("i64"),
        PrimitiveType::F32 => Cow::Borrowed("f32"),
        PrimitiveType::F64 => Cow::Borrowed("f64"),
        PrimitiveType::Usize => Cow::Borrowed("usize"),
        PrimitiveType::Isize => Cow::Borrowed("isize"),
    }
}

/// Returns `true` if the return type is void in C.
pub fn is_void_return(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Unit)
}
