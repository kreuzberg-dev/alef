use crate::core::ir::TypeRef;

use super::primitives::needs_i64_cast;

fn php_expr_is_already_arc(expr: &str) -> bool {
    let trimmed = expr.trim();
    trimmed == "self.inner"
        || trimmed == "self.inner.clone()"
        || trimmed.starts_with("self.inner.as_ref()")
        || trimmed.starts_with("self.inner.clone()")
}

#[allow(clippy::too_many_arguments)]
/// PHP-specific return wrapping that handles i64 casts for u64/usize/isize primitives.
/// Extends the shared `wrap_return` with type conversions for primitives that are i64 in PHP.
///
/// For enum returns:
/// - json_string_enum_names: externally-tagged data enums (have data variants); need serde_json::to_string()
/// - string_enum_names: pure unit enums; use serde_json::to_value().as_str() path
pub(crate) fn php_wrap_return(
    expr: &str,
    return_type: &TypeRef,
    type_name: &str,
    opaque_types: &ahash::AHashSet<String>,
    self_is_opaque: bool,
    returns_ref: bool,
    returns_cow: bool,
    mutex_types: &ahash::AHashSet<String>,
    json_string_enum_names: &ahash::AHashSet<String>,
    string_enum_names: &ahash::AHashSet<String>,
) -> String {
    match return_type {
        TypeRef::Bytes => {
            // Core returns Vec<u8> or Bytes; convert to String for PHP binary-safe string.
            // ext-php-rs marshals &[u8] to PHP string, but Vec<u8> to PHP array.
            // So we convert to String using lossy UTF-8 encoding (safe for binary PNG/PDF data).
            let vec_expr = if returns_ref {
                format!("{expr}.to_vec()")
            } else {
                format!("Vec::<u8>::from({expr})")
            };
            format!("String::from_utf8_lossy(&{vec_expr}).into_owned()")
        }
        TypeRef::Primitive(p) if needs_i64_cast(p) => {
            format!("{expr} as i64")
        }
        TypeRef::Duration => format!("{expr}.as_millis() as i64"),
        // Opaque Named returns need Arc wrapper (and Mutex for mutex types)
        TypeRef::Named(n) if n == type_name && self_is_opaque => {
            // If the expression already evaluates to Arc<T> (e.g. `self.inner.clone()`
            // where `inner: Arc<T>`), don't wrap in another Arc.
            if php_expr_is_already_arc(expr) {
                return format!("Self {{ inner: {expr} }}");
            }
            let wrapper = if mutex_types.contains(type_name) {
                |v: String| format!("Arc::new(std::sync::Mutex::new({v}))")
            } else {
                |v: String| format!("Arc::new({v})")
            };
            if returns_cow {
                format!("Self {{ inner: {} }}", wrapper(format!("{expr}.into_owned()")))
            } else if returns_ref {
                format!("Self {{ inner: {} }}", wrapper(format!("{expr}.clone()")))
            } else {
                format!("Self {{ inner: {} }}", wrapper(expr.to_string()))
            }
        }
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if php_expr_is_already_arc(expr) {
                return format!("{n} {{ inner: {expr} }}");
            }
            let wrapper = if mutex_types.contains(n) {
                |v: String| format!("Arc::new(std::sync::Mutex::new({v}))")
            } else {
                |v: String| format!("Arc::new({v})")
            };
            if returns_cow {
                format!("{n} {{ inner: {} }}", wrapper(format!("{expr}.into_owned()")))
            } else if returns_ref {
                format!("{n} {{ inner: {} }}", wrapper(format!("{expr}.clone()")))
            } else {
                format!("{n} {{ inner: {} }}", wrapper(expr.to_string()))
            }
        }
        TypeRef::Named(n) => {
            // Non-opaque Named return type
            if json_string_enum_names.contains(n.as_str()) {
                // Externally-tagged data enum: serialize to JSON string
                if returns_cow {
                    format!("serde_json::to_string(&{expr}.into_owned()).unwrap_or_default()")
                } else if returns_ref {
                    format!("serde_json::to_string(&{expr}.clone()).unwrap_or_default()")
                } else {
                    format!("serde_json::to_string(&{expr}).unwrap_or_default()")
                }
            } else if string_enum_names.contains(n.as_str()) {
                // Pure unit enum: extract discriminant string via serde_json
                if returns_cow {
                    format!(
                        "serde_json::to_value(&{expr}.into_owned()).ok().and_then(|v| v.as_str().map(std::string::ToString::to_string)).unwrap_or_default()"
                    )
                } else if returns_ref {
                    format!(
                        "serde_json::to_value(&{expr}.clone()).ok().and_then(|v| v.as_str().map(std::string::ToString::to_string)).unwrap_or_default()"
                    )
                } else {
                    format!(
                        "serde_json::to_value(&{expr}).ok().and_then(|v| v.as_str().map(std::string::ToString::to_string)).unwrap_or_default()"
                    )
                }
            } else {
                // Non-enum Named type: use .into() for core-to-binding From conversion
                if returns_cow {
                    format!("{expr}.into_owned().into()")
                } else if returns_ref {
                    format!("{expr}.clone().into()")
                } else {
                    format!("{expr}.into()")
                }
            }
        }
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Primitive(p) if needs_i64_cast(p) => {
                format!("{expr}.map(|v| v as i64)")
            }
            TypeRef::Duration => format!("{expr}.map(|d| d.as_millis() as i64)"),
            TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
                if mutex_types.contains(n) {
                    if returns_ref {
                        format!("{expr}.map(|v| {n} {{ inner: Arc::new(std::sync::Mutex::new(v.clone())) }})")
                    } else {
                        format!("{expr}.map(|v| {n} {{ inner: Arc::new(std::sync::Mutex::new(v)) }})")
                    }
                } else {
                    if returns_ref {
                        format!("{expr}.map(|v| {n} {{ inner: Arc::new(v.clone()) }})")
                    } else {
                        format!("{expr}.map(|v| {n} {{ inner: Arc::new(v) }})")
                    }
                }
            }
            TypeRef::Named(n) => {
                if json_string_enum_names.contains(n.as_str()) {
                    // Externally-tagged data enum in Option
                    if returns_ref {
                        format!("{expr}.map(|v| serde_json::to_string(&v.clone()).unwrap_or_default())")
                    } else {
                        format!("{expr}.map(|v| serde_json::to_string(&v).unwrap_or_default())")
                    }
                } else if string_enum_names.contains(n.as_str()) {
                    // Pure unit enum in Option
                    if returns_ref {
                        format!(
                            "{expr}.map(|v| serde_json::to_value(&v.clone()).ok().and_then(|j| j.as_str().map(std::string::ToString::to_string)).unwrap_or_default())"
                        )
                    } else {
                        format!(
                            "{expr}.map(|v| serde_json::to_value(&v).ok().and_then(|j| j.as_str().map(std::string::ToString::to_string)).unwrap_or_default())"
                        )
                    }
                } else {
                    // Non-enum Named type
                    if returns_ref {
                        format!("{expr}.map(|v| v.clone().into())")
                    } else {
                        format!("{expr}.map(Into::into)")
                    }
                }
            }
            _ => {
                // Fall back to shared wrap_return for other Option types
                use crate::codegen::generators;
                generators::wrap_return(
                    expr,
                    return_type,
                    type_name,
                    opaque_types,
                    self_is_opaque,
                    returns_ref,
                    returns_cow,
                )
            }
        },
        TypeRef::Map(_, _) => {
            // The PHP binding layer uses `HashMap<K, V>` for Map returns. When the core
            // returns a reference (`returns_ref=true`), the type is `&BTreeMap` (or `&HashMap`).
            // Iterate over the map and collect into `HashMap` to satisfy the PHP return type.
            if returns_ref {
                format!(
                    "{expr}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<std::collections::HashMap<_, _>>()"
                )
            } else {
                // Owned map: collect into HashMap (works for BTreeMap and AHashMap via IntoIterator).
                format!("{expr}.into_iter().collect::<std::collections::HashMap<_, _>>()")
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(p) if needs_i64_cast(p) => {
                format!("{expr}.into_iter().map(|v| v as i64).collect()")
            }
            // Vec<Vec<T>> where the inner primitive needs widening (e.g. Vec<Vec<usize>> → Vec<Vec<i64>>)
            TypeRef::Vec(inner2) => {
                if let TypeRef::Primitive(p) = inner2.as_ref() {
                    if needs_i64_cast(p) {
                        return format!(
                            "{expr}.into_iter().map(|row| row.into_iter().map(|x| x as i64).collect::<Vec<_>>()).collect::<Vec<_>>()"
                        );
                    }
                }
                // Fall back to shared wrap_return for nested Vec types that don't need casting
                use crate::codegen::generators;
                generators::wrap_return(
                    expr,
                    return_type,
                    type_name,
                    opaque_types,
                    self_is_opaque,
                    returns_ref,
                    returns_cow,
                )
            }
            // Vec<Named> (non-opaque): when core returns &[T], use .iter().cloned() to avoid
            // clippy::into_iter_on_ref — `.into_iter()` on a slice reference is equivalent to
            // `.iter()` and does not consume it.
            TypeRef::Named(n) if !opaque_types.contains(n.as_str()) => {
                if json_string_enum_names.contains(n.as_str()) {
                    // Vec<externally-tagged data enum>
                    if returns_ref {
                        format!("{expr}.iter().map(|v| serde_json::to_string(v).unwrap_or_default()).collect()")
                    } else {
                        format!("{expr}.into_iter().map(|v| serde_json::to_string(&v).unwrap_or_default()).collect()")
                    }
                } else if string_enum_names.contains(n.as_str()) {
                    // Vec<pure unit enum>
                    if returns_ref {
                        format!(
                            "{expr}.iter().map(|v| serde_json::to_value(v).ok().and_then(|j| j.as_str().map(std::string::ToString::to_string)).unwrap_or_default()).collect()"
                        )
                    } else {
                        format!(
                            "{expr}.into_iter().map(|v| serde_json::to_value(&v).ok().and_then(|j| j.as_str().map(std::string::ToString::to_string)).unwrap_or_default()).collect()"
                        )
                    }
                } else {
                    // Non-enum Named type
                    if returns_ref {
                        format!("{expr}.iter().cloned().map(Into::into).collect()")
                    } else {
                        format!("{expr}.into_iter().map(Into::into).collect()")
                    }
                }
            }
            _ => {
                // Fall back to shared wrap_return for other Vec types
                use crate::codegen::generators;
                generators::wrap_return(
                    expr,
                    return_type,
                    type_name,
                    opaque_types,
                    self_is_opaque,
                    returns_ref,
                    returns_cow,
                )
            }
        },
        _ => {
            // Fall back to shared wrap_return for all other types
            use crate::codegen::generators;
            generators::wrap_return(
                expr,
                return_type,
                type_name,
                opaque_types,
                self_is_opaque,
                returns_ref,
                returns_cow,
            )
        }
    }
}
