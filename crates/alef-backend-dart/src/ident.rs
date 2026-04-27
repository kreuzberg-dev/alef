/// Dart reserved words and built-in identifiers that cannot be used as identifiers.
///
/// Includes all reserved words, built-in identifiers, and async-reserved words.
/// Source: <https://dart.dev/language/keywords>
const DART_RESERVED: &[&str] = &[
    "abstract",
    "as",
    "assert",
    "async",
    "await",
    "base",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "covariant",
    "default",
    "deferred",
    "do",
    "dynamic",
    "else",
    "enum",
    "export",
    "extends",
    "extension",
    "external",
    "factory",
    "false",
    "final",
    "finally",
    "for",
    "Function",
    "get",
    "hide",
    "if",
    "implements",
    "import",
    "in",
    "interface",
    "is",
    "late",
    "library",
    "mixin",
    "new",
    "null",
    "on",
    "operator",
    "part",
    "required",
    "rethrow",
    "return",
    "sealed",
    "set",
    "show",
    "static",
    "super",
    "switch",
    "sync",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "var",
    "void",
    "when",
    "while",
    "with",
    "yield",
];

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
    // Numeric tuple-field index: "0", "1", … → "field0", "field1", …
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("field{name}");
    }
    if DART_RESERVED.contains(&name) {
        return format!("{name}_");
    }
    name.to_string()
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
