use alef_codegen::type_mapper::TypeMapper;
use alef_core::ir::PrimitiveType;
use std::borrow::Cow;

/// TypeMapper for Kotlin bindings.
///
/// Maps Rust types to idiomatic Kotlin types:
/// - Unsigned integers map to Kotlin's unsigned types (u32→UInt, u64→ULong)
/// - Signed integers map to Kotlin's signed types (i32→Int, i64→Long)
/// - Booleans map to `Boolean`
/// - Strings map to `String`
/// - Bytes map to `ByteArray`
/// - Paths map to `java.nio.file.Path`
/// - JSON becomes `String`
/// - Optionals use Kotlin's nullable syntax (`T?`)
/// - Collections use `List<T>` and `Map<K, V>`
/// - Duration maps to `kotlin.time.Duration`
pub struct KotlinMapper;

impl TypeMapper for KotlinMapper {
    fn primitive(&self, prim: &PrimitiveType) -> Cow<'static, str> {
        use alef_core::ir::PrimitiveType;
        match prim {
            PrimitiveType::Bool => Cow::Borrowed("Boolean"),
            PrimitiveType::U8 => Cow::Borrowed("UByte"),
            PrimitiveType::U16 => Cow::Borrowed("UShort"),
            PrimitiveType::U32 => Cow::Borrowed("UInt"),
            PrimitiveType::U64 => Cow::Borrowed("ULong"),
            PrimitiveType::Usize => Cow::Borrowed("ULong"),
            PrimitiveType::I8 => Cow::Borrowed("Byte"),
            PrimitiveType::I16 => Cow::Borrowed("Short"),
            PrimitiveType::I32 => Cow::Borrowed("Int"),
            PrimitiveType::I64 => Cow::Borrowed("Long"),
            PrimitiveType::Isize => Cow::Borrowed("Long"),
            PrimitiveType::F32 => Cow::Borrowed("Float"),
            PrimitiveType::F64 => Cow::Borrowed("Double"),
        }
    }

    fn string(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn bytes(&self) -> Cow<'static, str> {
        Cow::Borrowed("ByteArray")
    }

    fn path(&self) -> Cow<'static, str> {
        Cow::Borrowed("Path")
    }

    fn json(&self) -> Cow<'static, str> {
        Cow::Borrowed("String")
    }

    fn unit(&self) -> Cow<'static, str> {
        Cow::Borrowed("Unit")
    }

    fn duration(&self) -> Cow<'static, str> {
        Cow::Borrowed("Duration")
    }

    fn optional(&self, inner: &str) -> String {
        format!("{inner}?")
    }

    fn vec(&self, inner: &str) -> String {
        format!("List<{inner}>")
    }

    fn map(&self, key: &str, value: &str) -> String {
        format!("Map<{key}, {value}>")
    }

    fn error_wrapper(&self) -> &str {
        "Result"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::TypeRef;

    #[test]
    fn test_primitive_bool() {
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::Bool), "Boolean");
    }

    #[test]
    fn test_primitive_unsigned() {
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::U8), "UByte");
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::U32), "UInt");
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::U64), "ULong");
    }

    #[test]
    fn test_primitive_signed() {
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::I8), "Byte");
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::I32), "Int");
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::I64), "Long");
    }

    #[test]
    fn test_primitive_float() {
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::F32), "Float");
        assert_eq!(KotlinMapper.primitive(&PrimitiveType::F64), "Double");
    }

    #[test]
    fn test_string() {
        assert_eq!(KotlinMapper.string(), "String");
    }

    #[test]
    fn test_bytes() {
        assert_eq!(KotlinMapper.bytes(), "ByteArray");
    }

    #[test]
    fn test_path() {
        assert_eq!(KotlinMapper.path(), "Path");
    }

    #[test]
    fn test_optional() {
        assert_eq!(KotlinMapper.optional("String"), "String?");
    }

    #[test]
    fn test_vec() {
        assert_eq!(KotlinMapper.vec("Int"), "List<Int>");
    }

    #[test]
    fn test_map() {
        assert_eq!(KotlinMapper.map("String", "Int"), "Map<String, Int>");
    }

    #[test]
    fn test_map_type_json() {
        assert_eq!(KotlinMapper.map_type(&TypeRef::Json), "String");
    }

    #[test]
    fn test_optional_string() {
        assert_eq!(
            KotlinMapper.map_type(&TypeRef::Optional(Box::new(TypeRef::String))),
            "String?"
        );
    }

    #[test]
    fn test_duration() {
        assert_eq!(KotlinMapper.map_type(&TypeRef::Duration), "Duration");
    }
}
