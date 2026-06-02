use crate::codegen::naming::{dart_tuple_field_identifier, dart_type_identifier, dart_value_identifier};

/// Make a generated class name safe for use as a Dart type declaration.
///
/// Dart core library classes (like `List`, `Map`, `Set`, `String`, etc.) cannot be
/// shadowed by generated classes: doing so breaks `List<T>` generics in the same file.
///
/// When `name` conflicts with a Dart core type, the parent enum or struct name is
/// prepended (e.g. `NodeContent` + `List` → `NodeContentList`). If `parent` is empty
/// or None, a trailing `Node` suffix is appended instead.
#[allow(dead_code)]
pub(crate) fn dart_safe_type_name(name: &str, parent: Option<&str>) -> String {
    dart_type_identifier(name, parent)
}

/// Escape a Dart identifier to avoid conflicts with reserved keywords or
/// invalid names such as numeric tuple-variant field indices.
///
/// Rules applied in order:
/// 1. Names whose first character is an ASCII digit (e.g. `"0"`) get `field`
///    prepended: `"0"` → `"field0"`.
/// 2. Names that exactly match a Dart reserved word get a trailing `_`
///    appended: `"default"` → `"default_"`.
/// 3. All other names are returned unchanged.
pub(crate) fn dart_safe_ident(name: &str) -> String {
    dart_value_identifier(name)
}

/// Make a generated tuple field safe in Dart value context.
#[allow(dead_code)]
pub(crate) fn dart_safe_tuple_field(name: &str) -> String {
    dart_tuple_field_identifier(name)
}

#[cfg(test)]
mod tests {
    use super::dart_safe_ident;

    #[test]
    fn reserved_keyword_default_gets_trailing_underscore() {
        assert_eq!(dart_safe_ident("default"), "default_");
    }

    #[test]
    fn reserved_keyword_final_gets_trailing_underscore() {
        assert_eq!(dart_safe_ident("final"), "final_");
    }

    #[test]
    fn reserved_keyword_class_gets_trailing_underscore() {
        assert_eq!(dart_safe_ident("class"), "class_");
    }

    #[test]
    fn reserved_keyword_return_gets_trailing_underscore() {
        assert_eq!(dart_safe_ident("return"), "return_");
    }

    #[test]
    fn reserved_keyword_required_gets_trailing_underscore() {
        assert_eq!(dart_safe_ident("required"), "required_");
    }

    #[test]
    fn numeric_ident_zero_gets_field_prefix() {
        assert_eq!(dart_safe_ident("0"), "field0");
    }

    #[test]
    fn numeric_ident_one_gets_field_prefix() {
        assert_eq!(dart_safe_ident("1"), "field1");
    }

    #[test]
    fn normal_ident_passes_through_unchanged() {
        assert_eq!(dart_safe_ident("radius"), "radius");
        assert_eq!(dart_safe_ident("xCoord"), "xCoord");
        assert_eq!(dart_safe_ident("field0"), "field0");
    }
}
