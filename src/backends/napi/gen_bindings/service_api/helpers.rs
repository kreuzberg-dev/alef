use crate::core::ir::{ApiSurface, HandlerContractDef, TypeRef};

pub(super) fn typescript_type_annotation(ty: &TypeRef) -> String {
    match ty {
        TypeRef::String | TypeRef::Char => "string".to_owned(),
        TypeRef::Primitive(p) => {
            use crate::core::ir::PrimitiveType;
            match p {
                PrimitiveType::Bool => "boolean".to_owned(),
                PrimitiveType::F32 | PrimitiveType::F64 => "number".to_owned(),
                _ => "number".to_owned(),
            }
        }
        TypeRef::Bytes => "Buffer".to_owned(),
        TypeRef::Optional(inner) => format!("{} | undefined", typescript_type_annotation(inner)),
        TypeRef::Vec(inner) => format!("{}[]", typescript_type_annotation(inner)),
        TypeRef::Map(k, v) => format!(
            "Record<{}, {}>",
            typescript_type_annotation(k),
            typescript_type_annotation(v)
        ),
        TypeRef::Unit => "void".to_owned(),
        TypeRef::Named(n) => n.clone(),
        TypeRef::Json => "any".to_owned(),
        TypeRef::Path => "string".to_owned(),
        TypeRef::Duration => "number".to_owned(),
    }
}

/// Find the `HandlerContractDef` by trait name in the surface.
pub(super) fn find_contract<'a>(api: &'a ApiSurface, trait_name: &str) -> Option<&'a HandlerContractDef> {
    api.handler_contracts.iter().find(|c| c.trait_name == trait_name)
}

// `api` is used in the TypeRef::Named arm to look up opaque types; clippy
// incorrectly classifies it as only-used-in-recursion because the check
// happens inside a match arm rather than at the top of the function.
#[allow(clippy::only_used_in_recursion)]
pub(super) fn gen_metadata_extraction(ty: &TypeRef, core_import: &str, api: &ApiSurface) -> String {
    match ty {
        TypeRef::String | TypeRef::Char => {
            "val.as_str().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected string metadata\"))?.to_owned()".to_owned()
        }
        TypeRef::Primitive(p) => {
            use crate::core::ir::PrimitiveType;
            match p {
                PrimitiveType::Bool => {
                    "val.as_bool().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected bool metadata\"))?".to_owned()
                }
                PrimitiveType::F64 => {
                    "val.as_f64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))?".to_owned()
                }
                PrimitiveType::F32 => {
                    "val.as_f64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as f32".to_owned()
                }
                PrimitiveType::U8 => {
                    "val.as_u64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as u8".to_owned()
                }
                PrimitiveType::U16 => {
                    "val.as_u64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as u16".to_owned()
                }
                PrimitiveType::U32 => {
                    "val.as_u64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as u32".to_owned()
                }
                PrimitiveType::U64 | PrimitiveType::Usize => {
                    "val.as_u64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))?".to_owned()
                }
                PrimitiveType::I8 => {
                    "val.as_i64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as i8".to_owned()
                }
                PrimitiveType::I16 => {
                    "val.as_i64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as i16".to_owned()
                }
                PrimitiveType::I32 => {
                    "val.as_i64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))? as i32".to_owned()
                }
                PrimitiveType::I64 | PrimitiveType::Isize => {
                    "val.as_i64().ok_or_else(|| napi::Error::new(napi::Status::InvalidArg, \"expected number metadata\"))?".to_owned()
                }
            }
        }
        TypeRef::Optional(inner) => {
            let inner_extraction = gen_metadata_extraction(inner, core_import, api);
            format!("if val.is_null() {{ None }} else {{ Some({{ {inner_extraction} }}) }}")
        }
        TypeRef::Named(n) => {
            // Check if this Named type is opaque in the API surface
            let is_opaque = api.types
                .iter()
                .find(|t| &t.name == n && !t.is_trait && t.is_opaque)
                .is_some();

            if is_opaque {
                // For opaque types: deserialize as the NAPI binding wrapper class,
                // then unwrap .inner to get the core type.
                // This follows the pattern: extract wrapper, then unwrap.inner
                format!(
                    "{{ \
                        let binding = serde_json::from_value::<crate::{name}>(val.clone()) \
                            .map_err(|e| napi::Error::from_reason(format!(\"opaque type deserialization failed: {{}}\", e)))?; \
                        binding.inner.clone() \
                    }}",
                    name = n
                )
            } else {
                // For non-opaque Named types: deserialize directly via serde_json
                "serde_json::from_value(val.clone())
                    .map_err(|e| napi::Error::from_reason(format!(\"metadata deserialization failed: {}\", e)))?".to_owned()
            }
        }
        _ => {
            // For other complex types: deserialize directly from serde_json::Value
            "serde_json::from_value(val.clone())
                .map_err(|e| napi::Error::from_reason(format!(\"metadata deserialization failed: {}\", e)))?".to_owned()
        }
    }
}

/// Map a `TypeRef` to a Rust type string for use in generated function signatures.
pub(super) fn typeref_to_rust_type(ty: &TypeRef, core_import: &str) -> String {
    match ty {
        TypeRef::String | TypeRef::Char => "String".to_owned(),
        TypeRef::Primitive(p) => {
            use crate::core::ir::PrimitiveType;
            match p {
                PrimitiveType::Bool => "bool".to_owned(),
                PrimitiveType::U8 => "u8".to_owned(),
                PrimitiveType::U16 => "u16".to_owned(),
                PrimitiveType::U32 => "u32".to_owned(),
                PrimitiveType::U64 => "u64".to_owned(),
                PrimitiveType::I8 => "i8".to_owned(),
                PrimitiveType::I16 => "i16".to_owned(),
                PrimitiveType::I32 => "i32".to_owned(),
                PrimitiveType::I64 => "i64".to_owned(),
                PrimitiveType::F32 => "f32".to_owned(),
                PrimitiveType::F64 => "f64".to_owned(),
                PrimitiveType::Usize => "usize".to_owned(),
                PrimitiveType::Isize => "isize".to_owned(),
            }
        }
        TypeRef::Bytes => "Vec<u8>".to_owned(),
        TypeRef::Optional(inner) => format!("Option<{}>", typeref_to_rust_type(inner, core_import)),
        TypeRef::Vec(inner) => format!("Vec<{}>", typeref_to_rust_type(inner, core_import)),
        TypeRef::Map(k, v) => format!(
            "std::collections::HashMap<{}, {}>",
            typeref_to_rust_type(k, core_import),
            typeref_to_rust_type(v, core_import)
        ),
        TypeRef::Unit => "()".to_owned(),
        TypeRef::Named(n) => format!("{core_import}::{n}"),
        TypeRef::Json => "serde_json::Value".to_owned(),
        TypeRef::Path => "std::path::PathBuf".to_owned(),
        TypeRef::Duration => "std::time::Duration".to_owned(),
    }
}
