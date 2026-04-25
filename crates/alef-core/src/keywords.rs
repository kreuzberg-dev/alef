//! Reserved keyword lists and field-name escaping for all supported language backends.
//!
//! Each language backend may encounter Rust field names that are reserved keywords
//! in the target language. This module provides a central registry of those keywords
//! and a function to compute the safe name to use in the generated binding.
//!
//! # Escape strategy
//!
//! When a field name is reserved in the target language it is escaped by appending
//! a trailing underscore (e.g. `class` → `class_`).  The original name is preserved
//! in language-level attribute annotations so the user-visible API still exposes the
//! original name (e.g. `#[pyo3(get, name = "class")]`, `#[serde(rename = "class")]`).

/// Python reserved keywords and soft-keywords that cannot be used as identifiers.
///
/// Includes the `type` soft-keyword (Python 3.12+) and the built-in constants
/// `None`, `True`, `False` which are also reserved in identifier position.
pub const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
    "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal",
    "not", "or", "pass", "raise", "return", "try", "type", "while", "with", "yield",
];

/// Java reserved keywords (including all contextual/reserved identifiers).
pub const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
];

/// C# reserved keywords.
pub const CSHARP_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

/// PHP reserved keywords.
pub const PHP_KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "as",
    "break",
    "callable",
    "case",
    "catch",
    "class",
    "clone",
    "const",
    "continue",
    "declare",
    "default",
    "die",
    "do",
    "echo",
    "else",
    "elseif",
    "empty",
    "enddeclare",
    "endfor",
    "endforeach",
    "endif",
    "endswitch",
    "endwhile",
    "eval",
    "exit",
    "extends",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "function",
    "global",
    "goto",
    "if",
    "implements",
    "include",
    "instanceof",
    "insteadof",
    "interface",
    "isset",
    "list",
    "match",
    "namespace",
    "new",
    "or",
    "print",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "return",
    "static",
    "switch",
    "throw",
    "trait",
    "try",
    "unset",
    "use",
    "var",
    "while",
    "xor",
    "yield",
];

/// Ruby reserved keywords.
pub const RUBY_KEYWORDS: &[&str] = &[
    "__ENCODING__",
    "__FILE__",
    "__LINE__",
    "BEGIN",
    "END",
    "alias",
    "and",
    "begin",
    "break",
    "case",
    "class",
    "def",
    "defined?",
    "do",
    "else",
    "elsif",
    "end",
    "ensure",
    "false",
    "for",
    "if",
    "in",
    "module",
    "next",
    "nil",
    "not",
    "or",
    "redo",
    "rescue",
    "retry",
    "return",
    "self",
    "super",
    "then",
    "true",
    "undef",
    "unless",
    "until",
    "when",
    "while",
    "yield",
];

/// Elixir reserved keywords (including sigil names and special atoms).
pub const ELIXIR_KEYWORDS: &[&str] = &[
    "after", "and", "catch", "do", "else", "end", "false", "fn", "in", "nil", "not", "or", "rescue", "true", "when",
];

/// Go reserved keywords.
pub const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

/// JavaScript / TypeScript reserved keywords (union of both).
pub const JS_KEYWORDS: &[&str] = &[
    "abstract",
    "arguments",
    "await",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "function",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "int",
    "interface",
    "let",
    "long",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "volatile",
    "while",
    "with",
    "yield",
];

/// R reserved keywords.
pub const R_KEYWORDS: &[&str] = &[
    "FALSE", "Inf", "NA", "NaN", "NULL", "TRUE", "break", "else", "for", "function", "if", "in", "next", "repeat",
    "return", "while",
];

/// Return the escaped field name for use in the generated binding of the given language,
/// or `None` if the name is not reserved and no escaping is needed.
///
/// The escape strategy appends `_` to the name (e.g. `class` → `class_`).
/// Call sites should use the returned value as the Rust field name in the binding struct
/// and add language-appropriate attribute annotations to preserve the original name in
/// the user-facing API.
pub fn python_safe_name(name: &str) -> Option<String> {
    if PYTHON_KEYWORDS.contains(&name) {
        Some(format!("{name}_"))
    } else {
        None
    }
}

/// Like `python_safe_name` but always returns a `String`, using the original when no
/// escaping is needed. Convenience wrapper for call sites that always need a `String`.
pub fn python_ident(name: &str) -> String {
    python_safe_name(name).unwrap_or_else(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_class_is_reserved() {
        assert_eq!(python_safe_name("class"), Some("class_".to_string()));
    }

    #[test]
    fn python_ordinary_name_is_none() {
        assert_eq!(python_safe_name("layout_class"), None);
    }

    #[test]
    fn python_ident_reserved() {
        assert_eq!(python_ident("class"), "class_");
    }

    #[test]
    fn python_ident_ordinary() {
        assert_eq!(python_ident("layout_class"), "layout_class");
    }

    #[test]
    fn python_keywords_covers_common_cases() {
        for kw in &[
            "def", "return", "yield", "pass", "import", "from", "type", "None", "True", "False",
        ] {
            assert!(
                python_safe_name(kw).is_some(),
                "expected {kw:?} to be a Python reserved keyword"
            );
        }
    }
}
