use super::call_args::needs_napi_cast;
use crate::codegen::generators;
use crate::core::ir::TypeRef;
use ahash::AHashSet;

fn arc_wrap(val: &str, type_name: &str, mutex_types: &AHashSet<String>) -> String {
    if mutex_types.contains(type_name) {
        format!("Arc::new(std::sync::Mutex::new({val}))")
    } else {
        format!("Arc::new({val})")
    }
}

#[allow(clippy::too_many_arguments)]
/// NAPI-specific return wrapping for opaque instance methods.
/// Extends the shared `wrap_return` with i64 casts for u64/usize/isize primitives.
pub(in crate::backends::napi::gen_bindings) fn napi_wrap_return(
    expr: &str,
    return_type: &TypeRef,
    type_name: &str,
    opaque_types: &AHashSet<String>,
    self_is_opaque: bool,
    returns_ref: bool,
    prefix: &str,
    mutex_types: &AHashSet<String>,
) -> String {
    match return_type {
        TypeRef::Primitive(p) if needs_napi_cast(p) => {
            format!("{expr} as i64")
        }
        TypeRef::Duration => format!("{expr}.as_millis() as i64"),
        // Opaque Named returns need prefix
        TypeRef::Named(n) if n == type_name && self_is_opaque => {
            // When expr is self.inner or self.inner.clone(), it's already Arc<T>, so don't wrap again.
            let already_arc = expr == "self.inner"
                || expr == "self.inner.clone()"
                || expr.starts_with("self.inner.as_ref()")
                || expr.starts_with("self.inner.clone()");
            if already_arc {
                format!("Self {{ inner: {expr} }}")
            } else if returns_ref {
                format!(
                    "Self {{ inner: {} }}",
                    arc_wrap(&format!("{expr}.clone()"), n, mutex_types)
                )
            } else {
                format!("Self {{ inner: {} }}", arc_wrap(expr, n, mutex_types))
            }
        }
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            // When expr is self.inner or self.inner.clone(), it's already Arc<T>, so don't wrap again.
            // For method calls that return the inner type directly, we need to wrap in Arc.
            let already_arc = expr == "self.inner"
                || expr == "self.inner.clone()"
                || expr.starts_with("self.inner.as_ref()")
                || expr.starts_with("self.inner.clone()");
            if already_arc {
                format!("{prefix}{n} {{ inner: {expr} }}")
            } else if returns_ref {
                format!(
                    "{prefix}{n} {{ inner: {} }}",
                    arc_wrap(&format!("{expr}.clone()"), n, mutex_types)
                )
            } else {
                format!("{prefix}{n} {{ inner: {} }}", arc_wrap(expr, n, mutex_types))
            }
        }
        TypeRef::Named(_) => {
            if returns_ref {
                format!("{expr}.clone().into()")
            } else {
                format!("{expr}.into()")
            }
        }
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!(
                        "{expr}.map(|v| {prefix}{name} {{ inner: {} }})",
                        arc_wrap("v.clone()", name, mutex_types)
                    )
                } else {
                    format!(
                        "{expr}.map(|v| {prefix}{name} {{ inner: {} }})",
                        arc_wrap("v", name, mutex_types)
                    )
                }
            }
            TypeRef::Vec(vec_inner) => match vec_inner.as_ref() {
                TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
                    if returns_ref {
                        format!(
                            "{expr}.map(|v| v.into_iter().map(|x| {prefix}{n} {{ inner: {} }}).collect())",
                            arc_wrap("x.clone()", n, mutex_types)
                        )
                    } else {
                        format!(
                            "{expr}.map(|v| v.into_iter().map(|x| {prefix}{n} {{ inner: {} }}).collect())",
                            arc_wrap("x", n, mutex_types)
                        )
                    }
                }
                _ => generators::wrap_return(
                    expr,
                    return_type,
                    type_name,
                    opaque_types,
                    self_is_opaque,
                    returns_ref,
                    false,
                ),
            },
            _ => generators::wrap_return(
                expr,
                return_type,
                type_name,
                opaque_types,
                self_is_opaque,
                returns_ref,
                false,
            ),
        },
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!(
                        "{expr}.into_iter().map(|v| {prefix}{name} {{ inner: {} }}).collect()",
                        arc_wrap("v.clone()", name, mutex_types)
                    )
                } else {
                    format!(
                        "{expr}.into_iter().map(|v| {prefix}{name} {{ inner: {} }}).collect()",
                        arc_wrap("v", name, mutex_types)
                    )
                }
            }
            _ => generators::wrap_return(
                expr,
                return_type,
                type_name,
                opaque_types,
                self_is_opaque,
                returns_ref,
                false,
            ),
        },
        _ => generators::wrap_return(
            expr,
            return_type,
            type_name,
            opaque_types,
            self_is_opaque,
            returns_ref,
            false,
        ),
    }
}

/// NAPI-specific return wrapping for free functions (no type_name context).
pub(in crate::backends::napi::gen_bindings) fn napi_wrap_return_fn(
    expr: &str,
    return_type: &TypeRef,
    opaque_types: &AHashSet<String>,
    returns_ref: bool,
    prefix: &str,
    capsule_types: Option<&std::collections::HashMap<String, crate::core::config::NodeCapsuleTypeConfig>>,
    mutex_types: &AHashSet<String>,
) -> String {
    match return_type {
        TypeRef::Primitive(p) if needs_napi_cast(p) => {
            format!("{expr} as i64")
        }
        TypeRef::Duration => format!("{expr}.as_millis() as i64"),
        TypeRef::Named(n) if capsule_types.is_some_and(|ct| ct.contains_key(n.as_str())) => {
            // Capsule types are returned as-is from the core, no wrapper wrapping
            expr.to_string()
        }
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if returns_ref {
                format!(
                    "{prefix}{n} {{ inner: {} }}",
                    arc_wrap(&format!("{expr}.clone()"), n, mutex_types)
                )
            } else {
                format!("{prefix}{n} {{ inner: {} }}", arc_wrap(expr, n, mutex_types))
            }
        }
        TypeRef::Named(_) => {
            if returns_ref {
                format!("{expr}.clone().into()")
            } else {
                format!("{expr}.into()")
            }
        }
        TypeRef::String | TypeRef::Char => {
            if returns_ref {
                format!("{expr}.into()")
            } else {
                expr.to_string()
            }
        }
        // Bytes always converts: core returns Vec<u8>/Bytes, binding expects napi Buffer.
        TypeRef::Bytes => format!("{expr}.into()"),
        TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
        TypeRef::Json => format!("{expr}.to_string()"),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) if capsule_types.is_some_and(|ct| ct.contains_key(name.as_str())) => {
                // Capsule types wrapped in Option are returned as-is
                expr.to_string()
            }
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!(
                        "{expr}.map(|v| {prefix}{name} {{ inner: {} }})",
                        arc_wrap("v.clone()", name, mutex_types)
                    )
                } else {
                    format!(
                        "{expr}.map(|v| {prefix}{name} {{ inner: {} }})",
                        arc_wrap("v", name, mutex_types)
                    )
                }
            }
            TypeRef::Named(_) => {
                if returns_ref {
                    format!("{expr}.map(|v| v.clone().into())")
                } else {
                    format!("{expr}.map(Into::into)")
                }
            }
            TypeRef::Map(_, _) => {
                // Optional map return: wrap the map conversion in .map(...)
                if returns_ref {
                    format!("{expr}.map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())")
                } else {
                    format!("{expr}.map(|m| m.into_iter().collect())")
                }
            }
            TypeRef::Vec(inner) => match inner.as_ref() {
                TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
                    if returns_ref {
                        format!(
                            "{expr}.map(|v| v.into_iter().map(|x| {prefix}{n} {{ inner: {} }}).collect())",
                            arc_wrap("x.clone()", n, mutex_types)
                        )
                    } else {
                        format!(
                            "{expr}.map(|v| v.into_iter().map(|x| {prefix}{n} {{ inner: {} }}).collect())",
                            arc_wrap("x", n, mutex_types)
                        )
                    }
                }
                TypeRef::Named(_) => {
                    if returns_ref {
                        format!("{expr}.map(|v| v.into_iter().map(|x| x.clone().into()).collect())")
                    } else {
                        format!("{expr}.map(|v| v.into_iter().map(Into::into).collect())")
                    }
                }
                _ => expr.to_string(),
            },
            TypeRef::Path => {
                format!("{expr}.map(Into::into)")
            }
            TypeRef::String | TypeRef::Char => {
                if returns_ref {
                    format!("{expr}.map(Into::into)")
                } else {
                    expr.to_string()
                }
            }
            TypeRef::Bytes => format!("{expr}.map(Into::into)"),
            _ => expr.to_string(),
        },
        TypeRef::Map(_, _) => {
            // Map returns: core may return &BTreeMap or &HashMap.
            // The NAPI binding maps to HashMap<K, V> (owned).
            // When core returns a reference (returns_ref=true), iterate and clone both keys and values.
            if returns_ref {
                format!("{expr}.iter().map(|(k, v)| (k.clone(), v.clone())).collect()")
            } else {
                // Owned map: collect into HashMap (works for BTreeMap and HashMap via IntoIterator).
                format!("{expr}.into_iter().collect()")
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(p) if needs_napi_cast(p) => {
                // Vec<usize>, Vec<f32>, etc. need element-wise casting to i64 or f64
                let target_ty = match p {
                    crate::core::ir::PrimitiveType::F32 => "f64",
                    _ => "i64", // u64, usize, isize
                };
                format!("{expr}.into_iter().map(|v| v as {target_ty}).collect()")
            }
            // Vec<Vec<T>> where the inner primitive needs widening (e.g. Vec<Vec<f32>> → Vec<Vec<f64>>)
            TypeRef::Vec(inner2) => {
                if let TypeRef::Primitive(p) = inner2.as_ref() {
                    if needs_napi_cast(p) {
                        let target_ty = match p {
                            crate::core::ir::PrimitiveType::F32 => "f64",
                            _ => "i64",
                        };
                        return format!(
                            "{expr}.into_iter().map(|row| row.into_iter().map(|x| x as {target_ty}).collect::<Vec<_>>()).collect::<Vec<_>>()"
                        );
                    }
                }
                expr.to_string()
            }
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if returns_ref {
                    format!("{expr}.into_iter().map(|v| {prefix}{name} {{ inner: Arc::new(v.clone()) }}).collect()")
                } else {
                    format!("{expr}.into_iter().map(|v| {prefix}{name} {{ inner: Arc::new(v) }}).collect()")
                }
            }
            TypeRef::Named(_) => {
                if returns_ref {
                    // `&[T]` → `Vec<U>`: clone each `&T` and convert. Use `.iter()`
                    // not `.into_iter()` because `.into_iter()` on `&[T]` yields `&T`
                    // (clippy::into_iter_on_ref under -D warnings).
                    format!("{expr}.iter().map(|v| v.clone().into()).collect()")
                } else {
                    format!("{expr}.into_iter().map(Into::into).collect()")
                }
            }
            TypeRef::Path => {
                format!("{expr}.into_iter().map(Into::into).collect()")
            }
            TypeRef::String | TypeRef::Char => {
                if returns_ref {
                    // `&[&str]` → `Vec<String>`: convert each `&&str` element through
                    // `.to_string()`. `Into::into` would need
                    // `impl From<&&str> for String`, which doesn't exist.
                    format!("{expr}.iter().map(|s| s.to_string()).collect()")
                } else {
                    expr.to_string()
                }
            }
            TypeRef::Bytes => {
                if returns_ref {
                    format!("{expr}.iter().map(|b| b.to_vec()).collect()")
                } else {
                    expr.to_string()
                }
            }
            _ => expr.to_string(),
        },
        _ => expr.to_string(),
    }
}
