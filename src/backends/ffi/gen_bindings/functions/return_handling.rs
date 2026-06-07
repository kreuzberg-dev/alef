use crate::core::ir::TypeRef;
use ahash::AHashSet;

use super::super::helpers::gen_owned_value_to_c;

/// Returns true when the return type requires JSON serialization of a Named type that is NOT
/// in `serde_names` (i.e. does not derive `serde::Serialize`).
///
/// FFI returns `Vec<T>` and `Map<K, V>` by serializing to JSON, which requires all
/// contained Named types to implement `serde::Serialize`. When a Named type lacks that
/// derive, generating the JSON path would produce a compile error in the output crate.
/// Such functions are stubbed out (emit unimplemented) instead.
pub(super) fn return_type_needs_non_serde_named(ty: &TypeRef, serde_names: &AHashSet<String>) -> bool {
    match ty {
        TypeRef::Vec(inner) => {
            if let TypeRef::Named(n) = inner.as_ref() {
                return !serde_names.contains(n.as_str());
            }
            false
        }
        TypeRef::Map(k, v) => {
            let k_bad = matches!(k.as_ref(), TypeRef::Named(n) if !serde_names.contains(n.as_str()));
            let v_bad = matches!(v.as_ref(), TypeRef::Named(n) if !serde_names.contains(n.as_str()));
            k_bad || v_bad
        }
        TypeRef::Optional(inner) => return_type_needs_non_serde_named(inner, serde_names),
        _ => false,
    }
}

/// Returns true when a TypeRef maps to `*mut c_char` in return position — meaning the
/// FFI consumer must NUL-scan to find the byte length. A `_len()` companion is emitted
/// for every free function whose return type satisfies this predicate.
pub(in crate::backends::ffi::gen_bindings) fn returns_c_char(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => true,
        TypeRef::Vec(_) | TypeRef::Map(_, _) => true,
        TypeRef::Optional(inner) => matches!(
            inner.as_ref(),
            TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _)
        ),
        _ => false,
    }
}

/// Generate a C-string return expression that records the byte length before
/// transferring ownership to the caller.
///
/// The matching `_len()` companion reads this thread-local length instead of
/// re-executing the wrapped Rust function.
pub(super) fn gen_owned_c_char_to_c_with_len(expr: &str, ty: &TypeRef, indent: &str) -> String {
    match ty {
        TypeRef::String | TypeRef::Char => format!(
            "{indent}{{\n\
             {indent}    let __alef_return = {expr}.to_string();\n\
             {indent}    match CString::new(__alef_return) {{\n\
             {indent}        Ok(cs) => {{\n\
             {indent}            set_last_return_len(cs.as_bytes().len());\n\
             {indent}            cs.into_raw()\n\
             {indent}        }}\n\
             {indent}        Err(_) => {{\n\
             {indent}            set_last_return_len(0);\n\
             {indent}            std::ptr::null_mut()\n\
             {indent}        }}\n\
             {indent}    }}\n\
             {indent}}}"
        ),
        TypeRef::Path => format!(
            "{indent}{{\n\
             {indent}    let __alef_return = {expr}.to_string_lossy().to_string();\n\
             {indent}    match CString::new(__alef_return) {{\n\
             {indent}        Ok(cs) => {{\n\
             {indent}            set_last_return_len(cs.as_bytes().len());\n\
             {indent}            cs.into_raw()\n\
             {indent}        }}\n\
             {indent}        Err(_) => {{\n\
             {indent}            set_last_return_len(0);\n\
             {indent}            std::ptr::null_mut()\n\
             {indent}        }}\n\
             {indent}    }}\n\
             {indent}}}"
        ),
        TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _) => format!(
            "{indent}{{\n\
             {indent}    match serde_json::to_string(&{expr}) {{\n\
             {indent}        Ok(__alef_return) => match CString::new(__alef_return) {{\n\
             {indent}            Ok(cs) => {{\n\
             {indent}                set_last_return_len(cs.as_bytes().len());\n\
             {indent}                cs.into_raw()\n\
             {indent}            }}\n\
             {indent}            Err(_) => {{\n\
             {indent}                set_last_return_len(0);\n\
             {indent}                std::ptr::null_mut()\n\
             {indent}            }}\n\
             {indent}        }},\n\
             {indent}        Err(_) => {{\n\
             {indent}            set_last_return_len(0);\n\
             {indent}            std::ptr::null_mut()\n\
             {indent}        }}\n\
             {indent}    }}\n\
             {indent}}}"
        ),
        TypeRef::Optional(inner) => {
            let inner_conversion = gen_owned_c_char_to_c_with_len("val", inner, &format!("{indent}        "));
            format!(
                "{indent}match {expr} {{\n\
                 {indent}    Some(val) => {{\n\
                 {inner_conversion}\n\
                 {indent}    }}\n\
                 {indent}    None => {{\n\
                 {indent}        set_last_return_len(0);\n\
                 {indent}        std::ptr::null_mut()\n\
                 {indent}    }}\n\
                 {indent}}}"
            )
        }
        _ => gen_owned_value_to_c(expr, ty, indent, &AHashSet::new()),
    }
}
