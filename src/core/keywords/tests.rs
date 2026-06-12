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
fn kotlin_class_is_reserved() {
    assert_eq!(kotlin_safe_name("class"), Some("class_".to_string()));
    assert_eq!(kotlin_safe_name("fun"), Some("fun_".to_string()));
    assert_eq!(kotlin_safe_name("ordinary"), None);
    assert_eq!(kotlin_ident("typealias"), "typealias_");
}

#[test]
fn swift_init_is_reserved() {
    assert_eq!(swift_safe_name("init"), Some("init_".to_string()));
    assert_eq!(swift_safe_name("Self"), Some("Self_".to_string()));
    assert_eq!(swift_safe_name("normal"), None);
    assert_eq!(swift_ident("protocol"), "protocol_");
}

#[test]
fn swift_case_ident_backtick_escapes_reserved_keywords() {
    // Backtick escape is the Swift-idiomatic form for keyword-collision
    // identifiers in *emitted Swift code* (enum cases, struct fields,
    // function parameter labels). Distinct from `swift_ident`, which
    // emits a trailing-underscore form suitable for the Rust side of the
    // bridge.
    assert_eq!(swift_case_ident("default"), "`default`");
    assert_eq!(swift_case_ident("protocol"), "`protocol`");
    assert_eq!(swift_case_ident("init"), "`init`");
    assert_eq!(swift_case_ident("Self"), "`Self`");
    assert_eq!(swift_case_ident("Any"), "`Any`");
    assert_eq!(swift_case_ident("class"), "`class`");
    assert_eq!(swift_case_ident("inout"), "`inout`");
    assert_eq!(swift_case_ident("rethrows"), "`rethrows`");
    // Non-reserved identifiers pass through unchanged.
    assert_eq!(swift_case_ident("gitHub"), "gitHub");
    assert_eq!(swift_case_ident("normal"), "normal");
    assert_eq!(swift_case_ident("dracula"), "dracula");
}

#[test]
fn swift_case_safe_name_returns_some_for_reserved() {
    assert_eq!(swift_case_safe_name("default"), Some("`default`".to_string()));
    assert_eq!(swift_case_safe_name("normal"), None);
}

#[test]
fn dart_async_is_reserved() {
    assert_eq!(dart_safe_name("async"), Some("async_".to_string()));
    assert_eq!(dart_safe_name("late"), Some("late_".to_string()));
    assert_eq!(dart_safe_name("normal"), None);
    assert_eq!(dart_ident("required"), "required_");
}

#[test]
fn gleam_pub_is_reserved() {
    assert_eq!(gleam_safe_name("pub"), Some("pub_".to_string()));
    assert_eq!(gleam_safe_name("opaque"), Some("opaque_".to_string()));
    assert_eq!(gleam_safe_name("normal"), None);
    assert_eq!(gleam_ident("type"), "type_");
}

#[test]
fn zig_comptime_is_reserved() {
    assert_eq!(zig_safe_name("comptime"), Some("comptime_".to_string()));
    assert_eq!(zig_safe_name("errdefer"), Some("errdefer_".to_string()));
    assert_eq!(zig_safe_name("normal"), None);
    assert_eq!(zig_ident("usingnamespace"), "usingnamespace_");
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

#[test]
fn python_str_enum_ident_escapes_str_methods() {
    // str method-name collisions must be escaped with trailing underscore
    assert_eq!(python_str_enum_ident("title"), "title_");
    assert_eq!(python_str_enum_ident("lower"), "lower_");
    assert_eq!(python_str_enum_ident("upper"), "upper_");
    assert_eq!(python_str_enum_ident("count"), "count_");
    assert_eq!(python_str_enum_ident("capitalize"), "capitalize_");
    assert_eq!(python_str_enum_ident("split"), "split_");
}

#[test]
fn python_str_enum_ident_escapes_python_keywords() {
    // Python keywords should still be escaped (del is a keyword, not a method)
    assert_eq!(python_str_enum_ident("del"), "del_");
    assert_eq!(python_str_enum_ident("class"), "class_");
    assert_eq!(python_str_enum_ident("return"), "return_");
}

#[test]
fn python_str_enum_ident_passes_through_ordinary_names() {
    // Names that are neither keywords nor str methods pass through unchanged
    assert_eq!(python_str_enum_ident("body"), "body");
    assert_eq!(python_str_enum_ident("div"), "div");
    assert_eq!(python_str_enum_ident("paragraph"), "paragraph");
}

#[test]
fn python_str_enum_safe_name_returns_some_for_reserved() {
    assert_eq!(python_str_enum_safe_name("title"), Some("title_".to_string()));
    assert_eq!(python_str_enum_safe_name("del"), Some("del_".to_string()));
}

#[test]
fn python_str_enum_safe_name_returns_none_for_ordinary() {
    assert_eq!(python_str_enum_safe_name("body"), None);
    assert_eq!(python_str_enum_safe_name("content"), None);
}

#[test]
fn rust_raw_ident_escapes_rust_keywords() {
    assert_eq!(rust_raw_ident("type"), "r#type");
    assert_eq!(rust_raw_ident("match"), "r#match");
    assert_eq!(rust_raw_ident("fn"), "r#fn");
    assert_eq!(rust_raw_ident("loop"), "r#loop");
    assert_eq!(rust_raw_ident("struct"), "r#struct");
    assert_eq!(rust_raw_ident("move"), "r#move");
    assert_eq!(rust_raw_ident("ref"), "r#ref");
    assert_eq!(rust_raw_ident("async"), "r#async");
}

#[test]
fn rust_raw_ident_passes_through_ordinary_names() {
    assert_eq!(rust_raw_ident("content"), "content");
    assert_eq!(rust_raw_ident("item_type"), "item_type");
    assert_eq!(rust_raw_ident("model"), "model");
}

#[test]
fn rust_raw_ident_safe_returns_some_for_keywords() {
    assert_eq!(rust_raw_ident_safe("type"), Some("r#type".to_string()));
    assert_eq!(rust_raw_ident_safe("fn"), Some("r#fn".to_string()));
}

#[test]
fn rust_raw_ident_safe_returns_none_for_ordinary() {
    assert_eq!(rust_raw_ident_safe("content"), None);
    assert_eq!(rust_raw_ident_safe("model"), None);
}

#[test]
fn is_valid_rust_ident_chars_accepts_valid_identifiers() {
    assert!(is_valid_rust_ident_chars("content"));
    assert!(is_valid_rust_ident_chars("self_harm"));
    assert!(is_valid_rust_ident_chars("_private"));
    assert!(is_valid_rust_ident_chars("type")); // keyword, but char-valid
    assert!(is_valid_rust_ident_chars("CamelCase"));
}

#[test]
fn is_valid_rust_ident_chars_rejects_invalid_identifiers() {
    assert!(!is_valid_rust_ident_chars("self-harm")); // hyphen
    assert!(!is_valid_rust_ident_chars("self-harm/intent")); // hyphen and slash
    assert!(!is_valid_rust_ident_chars("sexual/minors")); // slash
    assert!(!is_valid_rust_ident_chars("")); // empty
    assert!(!is_valid_rust_ident_chars("123abc")); // starts with digit
}
