use skif_core::ir::{PrimitiveType, TypeRef};
use std::borrow::Cow;

/// Trait for mapping IR types to language-specific type strings.
/// Backends implement only what differs from the Rust default.
pub trait TypeMapper {
    /// Map a primitive type. Default: Rust type names.
    fn primitive(&self, prim: &PrimitiveType) -> Cow<'static, str> {
        Cow::Borrowed(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Usize => "usize",
            PrimitiveType::Isize => "isize",
        })
    }

    /// Map a string type. Default: "String"
    fn string(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    /// Map a bytes type. Default: "Vec<u8>"
    fn bytes(&self) -> Cow<'static, str> {
        Cow::Borrowed("Vec<u8>")
    }

    /// Map a path type. Default: "String"
    fn path(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    /// Map a JSON type. Default: "serde_json::Value"
    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("serde_json::Value")
    }

    /// Map a unit type. Default: "()"
    fn unit(&self) -> Cow<'static, str> {
        Cow::Borrowed("()")
    }

    /// Map an optional type. Default: "Option<T>"
    fn optional(&self, inner: &str) -> String {
        format!("Option<{inner}>")
    }

    /// Map a vec type. Default: "Vec<T>"
    fn vec(&self, inner: &str) -> String {
        format!("Vec<{inner}>")
    }

    /// Map a map type. Default: "HashMap<K, V>"
    fn map(&self, key: &str, value: &str) -> String {
        format!("HashMap<{key}, {value}>")
    }

    /// Map a named type. Default: identity.
    fn named<'a>(&self, name: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(name)
    }

    /// Map a full TypeRef. Typically not overridden.
    fn map_type(&self, ty: &TypeRef) -> String {
        match ty {
            TypeRef::Primitive(p) => self.primitive(p).into_owned(),
            TypeRef::String => self.string().into_owned(),
            TypeRef::Bytes => self.bytes().into_owned(),
            TypeRef::Path => self.path().into_owned(),
            TypeRef::Json => self.json().into_owned(),
            TypeRef::Unit => self.unit().into_owned(),
            TypeRef::Optional(inner) => self.optional(&self.map_type(inner)),
            TypeRef::Vec(inner) => self.vec(&self.map_type(inner)),
            TypeRef::Map(k, v) => self.map(&self.map_type(k), &self.map_type(v)),
            TypeRef::Named(name) => self.named(name).into_owned(),
        }
    }

    /// The error wrapper type for this language. e.g. "PyResult", "napi::Result", "PhpResult"
    fn error_wrapper(&self) -> &str;

    /// Wrap a return type with error handling if needed.
    fn wrap_return(&self, base: &str, has_error: bool) -> String {
        if has_error {
            format!("{}<{base}>", self.error_wrapper())
        } else {
            base.to_string()
        }
    }
}
