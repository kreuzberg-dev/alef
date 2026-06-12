use crate::core::ir::PrimitiveType;

/// Returns true if a primitive type needs i64 casting (NAPI/PHP — JS/PHP lack native u64).
pub(crate) fn needs_i64_cast(p: &PrimitiveType) -> bool {
    matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)
}

/// Returns true if a primitive type needs i32 casting (extendr — R maps small ints to i32).
pub(crate) fn needs_i32_cast(p: &PrimitiveType) -> bool {
    matches!(
        p,
        PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32 | PrimitiveType::I8 | PrimitiveType::I16
    )
}

/// Returns true if a primitive type needs f64 casting (extendr — R maps large ints and f32 to f64).
///
/// Includes `I64` because the extendr `TypeMapper` maps `PrimitiveType::I64` to `"f64"` (R has
/// no native i64), so binding structs store `i64` fields as `f64`. Cast conversions must mirror
/// this mapping in both binding→core and core→binding From impls.
pub(crate) fn needs_f64_cast(p: &PrimitiveType) -> bool {
    matches!(
        p,
        PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::Usize | PrimitiveType::Isize | PrimitiveType::F32
    )
}

/// Returns the core primitive type string for cast primitives.
pub(crate) fn core_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 => "u64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
        PrimitiveType::F32 => "f32",
        PrimitiveType::Bool => "bool",
        PrimitiveType::U8 => "u8",
        PrimitiveType::U16 => "u16",
        PrimitiveType::U32 => "u32",
        PrimitiveType::I8 => "i8",
        PrimitiveType::I16 => "i16",
        PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::F64 => "f64",
    }
}

/// Returns the binding primitive type string for cast primitives (core→binding direction).
pub(crate) fn binding_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize => "i64",
        PrimitiveType::F32 => "f64",
        PrimitiveType::Bool => "bool",
        PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32 => "i32",
        PrimitiveType::I8 | PrimitiveType::I16 | PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::F64 => "f64",
    }
}
