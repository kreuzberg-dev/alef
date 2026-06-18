//! Sub-helper functions for rendering individual assertion types in Rust e2e tests.

use std::fmt::Write as FmtWrite;

use crate::e2e::field_access::FieldResolver;
use crate::e2e::fixture::Assertion;

use super::args::json_to_rust_literal;
use super::assertion_synthetic::{numeric_literal, value_to_rust_string};

pub(super) fn render_equals_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(val) = &assertion.value {
        let expected = value_to_rust_string(val);
        // For string equality, trim trailing whitespace to handle trailing newlines
        // from the converter.
        if val.is_string() {
            // When the field is Optional<String> and was NOT pre-unwrapped to a local
            // var (e.g. inside a result_is_vec iteration where the call-site unwrap
            // pass is skipped), emit `.as_deref().unwrap_or("").trim()` so the
            // expression is `&str` rather than `Option<String>`.
            let is_opt_str_not_unwrapped = assertion.field.as_ref().is_some_and(|f| {
                let resolved = field_resolver.resolve(f);
                let is_opt = field_resolver.is_optional(resolved);
                let is_arr = field_resolver.is_array(resolved);
                is_opt && !is_arr && !is_unwrapped
            });
            // For fields whose `Option<T>` inner type is a display/content union (not
            // plain `String`), `.as_deref()` does not compile because the inner type
            // does not implement `Deref<Target=str>`. Use `.as_ref().map(|v|
            // v.to_string()).unwrap_or_default()` instead, which works for any type
            // that implements `Display` (including `String` itself).
            let is_display_as_text = assertion
                .field
                .as_ref()
                .is_some_and(|f| field_resolver.is_display_as_text(f));
            let field_expr = if is_opt_str_not_unwrapped && is_display_as_text {
                // Optional non-String content field not yet pre-unwrapped: use Display via
                // `.as_ref().map(|v| v.to_string())` so the inner type need not impl
                // `Deref<Target=str>`.
                format!("{field_access}.as_ref().map(|v| v.to_string()).unwrap_or_default().trim()")
            } else if is_opt_str_not_unwrapped {
                // Optional string-like field that wasn't pre-unwrapped: use `.as_deref()`
                // when the inner type is `String`; for inner types that impl Display we
                // can also do `.as_ref().map(ToString::to_string)`. Default to as_deref
                // which is the common String case — types without Display (rare) need
                // a separate fixture-level path resolution to land on a string child.
                format!("{field_access}.as_deref().unwrap_or(\"\").trim()")
            } else {
                // Non-optional string-like field: rely on Display impl via `.to_string()`.
                // This is correct for `String`, `&str`, and `Cow<str>` — Debug would
                // wrap them in extra quotes and break literal comparison.
                format!("{field_access}.to_string().as_str().trim()")
            };
            let _ = writeln!(
                out,
                "    assert_eq!({field_expr}, {expected}, \"equals assertion failed\");"
            );
        } else if val.is_boolean() {
            // Use assert!/assert!(!...) for booleans — clippy prefers this over assert_eq!(_, true/false).
            if val.as_bool() == Some(true) {
                let _ = writeln!(out, "    assert!({field_access}, \"equals assertion failed\");");
            } else {
                let _ = writeln!(out, "    assert!(!{field_access}, \"equals assertion failed\");");
            }
        } else {
            // Wrap expected value in Some() for optional fields.
            let is_opt = assertion.field.as_ref().is_some_and(|f| {
                let resolved = field_resolver.resolve(f);
                field_resolver.is_optional(resolved)
            });
            if is_opt && !is_unwrapped && assertion.field.as_ref().is_some_and(|_| true) {
                let _ = writeln!(
                    out,
                    "    assert_eq!({field_access}, Some({expected}), \"equals assertion failed\");"
                );
            } else {
                let _ = writeln!(
                    out,
                    "    assert_eq!({field_access}, {expected}, \"equals assertion failed\");"
                );
            }
        }
    }
}

pub(super) fn render_not_empty_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    result_var: &str,
    result_is_option: bool,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(f) = &assertion.field {
        let resolved = field_resolver.resolve(f);
        let is_opt = !is_unwrapped && field_resolver.is_optional(resolved);
        let is_arr = field_resolver.is_array(resolved);
        if is_opt && is_arr {
            // Option<Vec<T>>: must be Some AND inner non-empty.
            let accessor = field_resolver.accessor(f, "rust", result_var);
            let _ = writeln!(
                out,
                "    assert!({accessor}.as_ref().is_some_and(|v| !v.is_empty()), \"expected {f} to be present and non-empty\");"
            );
        } else if is_opt {
            // `is_optional` registers ANY path that crosses an Option<...> on the
            // way down, even when the leaf itself is concrete. For e.g. summary.text
            // (`Option<Summary>`, leaf String), the accessor already auto-unwraps the
            // parent — `result.summary.as_ref().unwrap().text` — so the final
            // expression has type String. Emitting `.is_some()` against that is a
            // compile error. Detect "leaf is post-unwrap concrete" by checking that
            // the accessor contains `.as_ref().unwrap().` (the trailing dot is the
            // marker that more field access follows the unwrap) and fall through to
            // the is_empty() form. If the accessor ENDS with `.as_ref().unwrap()`
            // (i.e. the Option itself IS the leaf), keep the is_some() form.
            let accessor = field_resolver.accessor(f, "rust", result_var);
            let leaf_is_concrete = accessor.contains(".as_ref().unwrap().");
            if leaf_is_concrete {
                let _ = writeln!(
                    out,
                    "    assert!(!{accessor}.is_empty(), \"expected {f} to be non-empty\");"
                );
            } else {
                let _ = writeln!(
                    out,
                    "    assert!({accessor}.is_some(), \"expected {f} to be present\");"
                );
            }
        } else {
            let _ = writeln!(
                out,
                "    assert!(!{field_access}.is_empty(), \"expected non-empty value\");"
            );
        }
    } else if result_is_option {
        // Bare result is Option<T>: not_empty == is_some().
        let _ = writeln!(
            out,
            "    assert!({field_access}.is_some(), \"expected non-empty value\");"
        );
    } else {
        // Bare result is a struct/string/collection — non-empty via is_empty().
        let _ = writeln!(
            out,
            "    assert!(!{field_access}.is_empty(), \"expected non-empty value\");"
        );
    }
}

pub(super) fn render_is_empty_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(f) = &assertion.field {
        let resolved = field_resolver.resolve(f);
        let is_opt = !is_unwrapped && field_resolver.is_optional(resolved);
        let is_arr = field_resolver.is_array(resolved);
        if is_opt && is_arr {
            // Option<Vec<T>>: empty means None or empty vec.
            let _ = writeln!(
                out,
                "    assert!({field_access}.as_ref().is_none_or(|v| v.is_empty()), \"expected {f} to be empty or absent\");"
            );
        } else if is_opt {
            let _ = writeln!(
                out,
                "    assert!({field_access}.is_none(), \"expected {f} to be absent\");"
            );
        } else {
            let _ = writeln!(out, "    assert!({field_access}.is_empty(), \"expected empty value\");");
        }
    } else {
        let _ = writeln!(out, "    assert!({field_access}.is_none(), \"expected empty value\");");
    }
}

pub(super) fn render_gte_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(val) = &assertion.value {
        let lit = numeric_literal(val);
        // Check whether this field is optional but not an array — e.g. Option<usize>.
        // Directly comparing Option<usize> >= N is a type error; wrap with unwrap_or(0).
        let is_opt_numeric = assertion.field.as_ref().is_some_and(|f| {
            let resolved = field_resolver.resolve(f);
            let is_opt = !is_unwrapped && field_resolver.is_optional(resolved);
            let is_arr = field_resolver.is_array(resolved);
            is_opt && !is_arr
        });
        if val.as_u64() == Some(1) && field_access.ends_with(".len()") {
            // Clippy prefers !is_empty() over len() >= 1 for collections.
            let base = field_access.strip_suffix(".len()").unwrap_or(field_access);
            let _ = writeln!(out, "    assert!(!{base}.is_empty(), \"expected >= 1\");");
        } else if is_opt_numeric {
            // Option<usize> / Option<u64> / Option<f64>: unwrap with appropriate zero literal
            // depending on whether the comparison value is float or integer.
            // Check if the rendered literal contains _f64 or a decimal point (float type indicator).
            let default_literal = if lit.contains("_f64") || lit.contains('.') {
                "0.0"
            } else {
                "0"
            };
            let _ = writeln!(
                out,
                "    assert!({field_access}.unwrap_or({default_literal}) >= {lit}, \"expected >= {lit}\");"
            );
        } else {
            let _ = writeln!(out, "    assert!({field_access} >= {lit}, \"expected >= {lit}\");");
        }
    }
}

pub(super) fn render_count_min_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(val) = &assertion.value {
        if let Some(n) = val.as_u64() {
            let opt_arr_field = assertion.field.as_ref().is_some_and(|f| {
                let resolved = field_resolver.resolve(f);
                let is_opt = !is_unwrapped && field_resolver.is_optional(resolved);
                let is_arr = field_resolver.is_array(resolved);
                is_opt && is_arr
            });
            let base = field_access.strip_suffix(".len()").unwrap_or(field_access);
            if opt_arr_field {
                // Option<Vec<T>>: must be Some AND inner len >= n.
                if n == 0 {
                    // count_min: 0 is always true — no assertion needed
                } else if n == 1 {
                    let _ = writeln!(
                        out,
                        "    assert!({base}.as_ref().is_some_and(|v| !v.is_empty()), \"expected >= {n}\");"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "    assert!({base}.as_ref().is_some_and(|v| v.len() >= {n}), \"expected at least {n} elements\");"
                    );
                }
            } else if n == 0 {
                // count_min: 0 is always true — no assertion needed
            } else if n == 1 {
                let _ = writeln!(out, "    assert!(!{base}.is_empty(), \"expected >= {n}\");");
            } else {
                let _ = writeln!(
                    out,
                    "    assert!({field_access}.len() >= {n}, \"expected at least {n} elements, got {{}}\", {field_access}.len());"
                );
            }
        }
    }
}

pub(super) fn render_count_equals_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    is_unwrapped: bool,
    field_resolver: &FieldResolver,
) {
    if let Some(val) = &assertion.value {
        if let Some(n) = val.as_u64() {
            let opt_arr_field = assertion.field.as_ref().is_some_and(|f| {
                let resolved = field_resolver.resolve(f);
                let is_opt = !is_unwrapped && field_resolver.is_optional(resolved);
                let is_arr = field_resolver.is_array(resolved);
                is_opt && is_arr
            });
            let base = field_access.strip_suffix(".len()").unwrap_or(field_access);
            if opt_arr_field {
                let _ = writeln!(
                    out,
                    "    assert!({base}.as_ref().is_some_and(|v| v.len() == {n}), \"expected exactly {n} elements\");"
                );
            } else {
                let _ = writeln!(
                    out,
                    "    assert_eq!({field_access}.len(), {n}, \"expected exactly {n} elements, got {{}}\", {field_access}.len());"
                );
            }
        }
    }
}

pub(super) fn render_method_result_assertion(
    out: &mut String,
    assertion: &Assertion,
    field_access: &str,
    result_is_tree: bool,
    module: &str,
) {
    if let Some(method_name) = &assertion.method {
        // Build the call expression. When the result is a tree-sitter Tree (an opaque
        // type), methods like `root_child_count` do not exist on `Tree` directly —
        // they are free functions in the crate or are accessed via `root_node()`.
        let call_expr = if result_is_tree {
            super::assertion_synthetic::build_tree_call_expr(field_access, method_name, assertion.args.as_ref(), module)
        } else if let Some(args) = &assertion.args {
            let arg_lit = json_to_rust_literal(args, "");
            format!("{field_access}.{method_name}({arg_lit})")
        } else {
            format!("{field_access}.{method_name}()")
        };

        // Determine whether the call expression returns a numeric type so we can
        // choose the right comparison strategy for `greater_than_or_equal`.
        let returns_numeric = result_is_tree && super::assertion_synthetic::is_tree_numeric_method(method_name);

        let check = assertion.check.as_deref().unwrap_or("is_true");
        match check {
            "equals" => {
                if let Some(val) = &assertion.value {
                    if val.is_boolean() {
                        if val.as_bool() == Some(true) {
                            let _ = writeln!(
                                out,
                                "    assert!({call_expr}, \"method_result equals assertion failed\");"
                            );
                        } else {
                            let _ = writeln!(
                                out,
                                "    assert!(!{call_expr}, \"method_result equals assertion failed\");"
                            );
                        }
                    } else {
                        let expected = value_to_rust_string(val);
                        let _ = writeln!(
                            out,
                            "    assert_eq!({call_expr}, {expected}, \"method_result equals assertion failed\");"
                        );
                    }
                }
            }
            "is_true" => {
                let _ = writeln!(
                    out,
                    "    assert!({call_expr}, \"method_result is_true assertion failed\");"
                );
            }
            "is_false" => {
                let _ = writeln!(
                    out,
                    "    assert!(!{call_expr}, \"method_result is_false assertion failed\");"
                );
            }
            "greater_than_or_equal" => {
                if let Some(val) = &assertion.value {
                    let lit = numeric_literal(val);
                    if returns_numeric {
                        // Numeric return (e.g., child_count()) — always use >= comparison.
                        let _ = writeln!(out, "    assert!({call_expr} >= {lit}, \"expected >= {lit}\");");
                    } else if val.as_u64() == Some(1) {
                        // Clippy prefers !is_empty() over len() >= 1 for collections.
                        let _ = writeln!(out, "    assert!(!{call_expr}.is_empty(), \"expected >= 1\");");
                    } else {
                        let _ = writeln!(out, "    assert!({call_expr} >= {lit}, \"expected >= {lit}\");");
                    }
                }
            }
            "count_min" => {
                if let Some(val) = &assertion.value {
                    let n = val.as_u64().unwrap_or(0);
                    if n <= 1 {
                        let _ = writeln!(out, "    assert!(!{call_expr}.is_empty(), \"expected >= {n}\");");
                    } else {
                        let _ = writeln!(
                            out,
                            "    assert!({call_expr}.len() >= {n}, \"expected at least {n} elements, got {{}}\", {call_expr}.len());"
                        );
                    }
                }
            }
            "is_error" => {
                // For is_error we need the raw Result without .unwrap().
                let raw_call = call_expr.strip_suffix(".unwrap()").unwrap_or(&call_expr);
                let _ = writeln!(
                    out,
                    "    assert!({raw_call}.is_err(), \"expected method to return error\");"
                );
            }
            "contains" => {
                if let Some(val) = &assertion.value {
                    let expected = value_to_rust_string(val);
                    let _ = writeln!(
                        out,
                        "    assert!({call_expr}.contains({expected}), \"expected result to contain {{}}\", {expected});"
                    );
                }
            }
            "not_empty" => {
                let _ = writeln!(
                    out,
                    "    assert!(!{call_expr}.is_empty(), \"expected non-empty result\");"
                );
            }
            "is_empty" => {
                let _ = writeln!(out, "    assert!({call_expr}.is_empty(), \"expected empty result\");");
            }
            other_check => {
                panic!("Rust e2e generator: unsupported method_result check type: {other_check}");
            }
        }
    } else {
        panic!("Rust e2e generator: method_result assertion missing 'method' field");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::e2e::field_access::FieldResolver;
    use crate::e2e::fixture::Assertion;

    fn empty_resolver() -> FieldResolver {
        FieldResolver::new(
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        )
    }

    /// Resolver with `content` as optional and display_as_text.
    fn display_as_text_resolver() -> FieldResolver {
        let mut optional = HashSet::new();
        optional.insert("content".to_string());
        let mut dat_fields = HashSet::new();
        dat_fields.insert("content".to_string());
        FieldResolver::new(
            &HashMap::new(),
            &optional,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        )
        .with_display_as_text_fields(dat_fields)
    }

    fn make_assertion(assertion_type: &str, field: Option<&str>, value: Option<serde_json::Value>) -> Assertion {
        Assertion {
            assertion_type: assertion_type.to_string(),
            field: field.map(|s| s.to_string()),
            value,
            ..Default::default()
        }
    }

    #[test]
    fn render_equals_assertion_string_produces_trim_call() {
        let resolver = empty_resolver();
        let assertion = make_assertion("equals", None, Some(serde_json::Value::String("hello".into())));
        let mut out = String::new();
        render_equals_assertion(&mut out, &assertion, "result", false, &resolver);
        assert!(out.contains(".trim()"), "got: {out}");
    }

    /// When a field is `Option<String>` (NOT display_as_text) and not pre-unwrapped,
    /// the assertion must use `.as_deref().unwrap_or("").trim()` — not `map(|v| v.to_string())`.
    /// This guards against regression where the DAT path is taken for plain strings.
    #[test]
    fn render_equals_assertion_plain_optional_string_uses_as_deref_not_to_string() {
        let mut optional = HashSet::new();
        optional.insert("content".to_string());
        let resolver = FieldResolver::new(
            &HashMap::new(),
            &optional,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        let assertion = make_assertion("equals", Some("content"), Some(serde_json::Value::String("hi".into())));
        let mut out = String::new();
        // is_unwrapped=false simulates result_is_vec=true where the pre-unwrap pass is skipped.
        render_equals_assertion(&mut out, &assertion, "result.content", false, &resolver);
        assert!(out.contains(".as_deref().unwrap_or(\"\").trim()"), "got: {out}");
        assert!(
            !out.contains("to_string"),
            "plain optional string should NOT use to_string(); got: {out}"
        );
    }

    /// When the field is `Option<AssistantContent>` (display_as_text) and not pre-unwrapped,
    /// the assertion must use `.as_ref().map(|v| v.to_string()).unwrap_or_default().trim()`
    /// so that `AssistantContent` (which implements `Display` but NOT `Deref<Target=str>`)
    /// compiles correctly.
    #[test]
    fn render_equals_assertion_display_as_text_optional_uses_map_to_string_not_as_deref() {
        let resolver = display_as_text_resolver();
        let assertion = make_assertion(
            "equals",
            Some("content"),
            Some(serde_json::Value::String("hello".into())),
        );
        let mut out = String::new();
        // is_unwrapped=false — simulates the result_is_vec=true path where pre-unwrapping is skipped.
        render_equals_assertion(&mut out, &assertion, "result.content", false, &resolver);
        // Must use .to_string() path via Display, NOT .as_deref() which requires Deref<Target=str>.
        assert!(
            out.contains(".as_ref().map(|v| v.to_string()).unwrap_or_default().trim()"),
            "display_as_text field must use map(|v| v.to_string()) path; got: {out}"
        );
        assert!(
            !out.contains("as_deref"),
            "display_as_text field must NOT emit as_deref(); got: {out}"
        );
    }

    /// When `is_unwrapped=true` (pre-unwrap pass already ran), display_as_text fields
    /// should fall through to the non-optional path, same as plain strings.
    #[test]
    fn render_equals_assertion_display_as_text_already_unwrapped_uses_to_string() {
        let resolver = display_as_text_resolver();
        let assertion = make_assertion(
            "equals",
            Some("content"),
            Some(serde_json::Value::String("hello".into())),
        );
        let mut out = String::new();
        // is_unwrapped=true — the pre-unwrap pass already produced a local `_content: String`.
        render_equals_assertion(&mut out, &assertion, "_content", true, &resolver);
        // Should use the regular to_string() path for an already-unwrapped value.
        assert!(out.contains("to_string().as_str().trim()"), "got: {out}");
        assert!(
            !out.contains("as_deref"),
            "unwrapped field must NOT emit as_deref(); got: {out}"
        );
        assert!(
            !out.contains("unwrap_or_default"),
            "unwrapped field must NOT emit unwrap_or_default(); got: {out}"
        );
    }

    #[test]
    fn render_not_empty_assertion_bare_result_emits_is_empty_check() {
        let resolver = empty_resolver();
        let assertion = make_assertion("not_empty", None, None);
        let mut out = String::new();
        render_not_empty_assertion(&mut out, &assertion, "result", "result", false, false, &resolver);
        assert!(out.contains("is_empty()"), "got: {out}");
    }

    #[test]
    fn render_count_min_assertion_small_n_uses_is_empty() {
        let resolver = empty_resolver();
        let assertion = make_assertion("count_min", None, Some(serde_json::json!(1u64)));
        let mut out = String::new();
        render_count_min_assertion(&mut out, &assertion, "result", false, &resolver);
        assert!(out.contains("is_empty()"), "got: {out}");
    }
}
