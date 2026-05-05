use ahash::AHashSet;
use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for NAPI bindings.
/// JS numbers are 53-bit safe, so U64/Usize/Isize map to i64.
/// Named types get a configurable prefix (defaults to "Js").
/// Trait types are mapped to JsVisitorRef (a Clone-able wrapper around Object<'static>).
pub struct NapiMapper {
    pub prefix: String,
    /// Names of types in the IR that are trait definitions (TypeDef::is_trait == true).
    pub trait_type_names: AHashSet<String>,
}

impl NapiMapper {
    pub fn new(prefix: String) -> Self {
        Self {
            prefix,
            trait_type_names: AHashSet::new(),
        }
    }

    pub fn with_traits(prefix: String, trait_type_names: AHashSet<String>) -> Self {
        Self {
            prefix,
            trait_type_names,
        }
    }
}

impl TypeMapper for NapiMapper {
    fn primitive(&self, prim: &PrimitiveType) -> Cow<'static, str> {
        Cow::Borrowed(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "i64",
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::F32 => "f64", // NAPI-RS doesn't impl FromNapiValue for f32
            PrimitiveType::F64 => "f64",
            PrimitiveType::Usize => "i64",
            PrimitiveType::Isize => "i64",
        })
    }

    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        if self.trait_type_names.contains(name) {
            // Trait types cannot be used as bare Object<'static> fields because
            // Object doesn't implement Clone. Use JsVisitorRef wrapper: a newtype that
            // wraps napi::Object and implements Clone via Arc.
            Cow::Borrowed("JsVisitorRef")
        } else {
            Cow::Owned(format!("{}{name}", self.prefix))
        }
    }

    /// NAPI uses i64 for Duration (JS numbers are 53-bit safe).
    fn duration(&self) -> Cow<'static, str> {
        Cow::Borrowed("i64")
    }

    /// NAPI doesn't implement FromNapiValue/ToNapiValue for serde_json::Value,
    /// so JSON is passed as a String and parsed on the JS side.
    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    /// NAPI accepts JS Buffer as napi::Buffer which auto-converts to &[u8].
    /// Functions receive napi::Buffer and must convert to Vec<u8> via .to_vec().
    fn bytes(&self) -> Cow<'static, str> {
        Cow::Borrowed("napi::Buffer")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}
