use crate::core::ir::TypeRef;

use crate::extract::type_resolver;

/// Build the fully qualified rust_path for an item, taking into account
/// the accumulated module path.
pub(crate) fn build_rust_path(crate_name: &str, module_path: &str, name: &str) -> String {
    if module_path.is_empty() {
        format!("{crate_name}::{name}")
    } else {
        format!("{crate_name}::{module_path}::{name}")
    }
}

/// Check if a syn::Type is `Box<T>` or `Option<Box<T>>`.
pub(crate) fn syn_type_is_boxed(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();
            if ident == "Box" {
                // Direct Box<T> — but not Box<dyn Trait> (those are opaque)
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            // Box<dyn Trait> is not a "boxed field" in our sense
                            if matches!(inner, syn::Type::TraitObject(_)) {
                                return false;
                            }
                            return true;
                        }
                    }
                }
            } else if ident == "Option" {
                // Option<Box<T>>
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            return syn_type_is_boxed(inner);
                        }
                    }
                }
            }
        }
    }
    false
}

/// Extract the fully qualified Rust path for a field's type when it uses a multi-segment
/// neutral fixture crate names.
/// path (e.g., `crate::types::OutputFormat` → `sample_core::types::OutputFormat`).
/// Returns `None` for simple single-segment types like `OutputFormat` or primitives.
///
/// When `crate_name` is provided, `crate::` prefixes are resolved to the crate name
/// (e.g., `crate::types::OutputFormat` → `sample_core::types::OutputFormat`).
/// `super::` paths are still skipped since they require full module context.
pub(crate) fn extract_field_type_rust_path(ty: &syn::Type, crate_name: Option<&str>) -> Option<String> {
    // Unwrap Option<T> to look at inner type
    let inner_ty = if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    args.args.iter().find_map(|arg| {
                        if let syn::GenericArgument::Type(inner) = arg {
                            Some(inner)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let check_ty = inner_ty.unwrap_or(ty);

    // Unwrap Box<T> to look at inner type
    let check_ty = if let syn::Type::Path(type_path) = check_ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Box" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    args.args
                        .iter()
                        .find_map(|arg| {
                            if let syn::GenericArgument::Type(inner) = arg {
                                Some(inner)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(check_ty)
                } else {
                    check_ty
                }
            } else {
                check_ty
            }
        } else {
            check_ty
        }
    } else {
        check_ty
    };

    // Now check if the type has a multi-segment path
    if let syn::Type::Path(type_path) = check_ty {
        if type_path.path.segments.len() >= 2 {
            let first_segment = type_path.path.segments[0].ident.to_string();
            // Skip `super::` paths — these require full module context and would produce
            // invalid paths like `sample_core::super::super::pdf::PdfConfig` in codegen.
            if first_segment == "super" {
                return None;
            }
            // Resolve `crate::` paths using the crate name when available.
            // This enables disambiguation of types with the same short name but different
            // module paths (e.g., `crate::types::OutputFormat` vs `crate::core::config::OutputFormat`).
            if first_segment == "crate" {
                if let Some(name) = crate_name {
                    let mut segments: Vec<String> =
                        type_path.path.segments.iter().map(|s| s.ident.to_string()).collect();
                    segments[0] = name.replace('-', "_").to_string();
                    return Some(segments.join("::"));
                }
                return None;
            }
            let segments: Vec<String> = type_path.path.segments.iter().map(|s| s.ident.to_string()).collect();
            return Some(segments.join("::"));
        }
    }
    None
}

/// Get the last segment ident of a type, unwrapping Option if present.
fn outermost_ident(ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            let ident = seg.ident.to_string();
            if ident == "Option" {
                // Recurse into Option<T>
                if let Some(inner) = type_resolver::extract_single_generic_arg_syn(seg) {
                    return outermost_ident(&inner);
                }
            }
            return Some(ident);
        }
    }
    None
}

/// Detect if a syn::Type is wrapped in Cow, Arc, Arc<Mutex<T>>, Arc<RwLock<T>>, or Bytes
/// (before resolution).
///
/// Peeks through `Option<...>` so `Option<Arc<Mutex<T>>>` resolves the same as the
/// bare `Arc<Mutex<T>>` form. `Arc<dyn Trait>` is deliberately left as plain `Arc` —
/// trait objects have different ownership semantics and must not be collapsed into
/// `ArcMutex`. The `Mutex`/`RwLock` check uses last-segment matching, so both
/// `std::sync::Mutex` and `tokio::sync::Mutex` map to `CoreWrapper::ArcMutex`
/// (intentional — both share the same lock/unlock binding shape).
pub(crate) fn detect_core_wrapper(ty: &syn::Type) -> crate::core::ir::CoreWrapper {
    use crate::core::ir::CoreWrapper;

    // Peek through Option<...> so Option<Arc<Mutex<T>>> is treated like Arc<Mutex<T>>.
    let inner_ty: Option<Box<syn::Type>> = if let syn::Type::Path(p) = ty {
        p.path.segments.last().and_then(|seg| {
            if seg.ident == "Option" {
                type_resolver::extract_single_generic_arg_syn(seg)
            } else {
                None
            }
        })
    } else {
        None
    };
    let probe: &syn::Type = inner_ty.as_deref().unwrap_or(ty);

    if let syn::Type::Path(p) = probe {
        if let Some(seg) = p.path.segments.last() {
            let ident = seg.ident.to_string();
            match ident.as_str() {
                "Cow" => return CoreWrapper::Cow,
                "Bytes" => return CoreWrapper::Bytes,
                // `Box<str>` is a common compact-string idiom in storage-heavy
                // structs (e.g. SHA-256 hex digests, immutable filenames). The
                // resolved IR ty is `String`, so binding emitters need to
                // `.into()` to round-trip back to `Box<str>` on the core side.
                // Box<dyn Trait> and Box<NamedType> are handled differently
                // (opaque or normal), so only flag Box<str> / Box<Bytes>.
                "Box" => {
                    if let Some(box_inner) = type_resolver::extract_single_generic_arg_syn(seg) {
                        if let syn::Type::Path(inner_path) = &*box_inner {
                            if let Some(inner_seg) = inner_path.path.segments.last() {
                                let inner_ident = inner_seg.ident.to_string();
                                if inner_ident == "str" {
                                    return CoreWrapper::Box;
                                }
                            }
                        }
                    }
                }
                "Arc" => {
                    // Inspect Arc's inner type. If it's Mutex<T> or RwLock<T>, return ArcMutex.
                    // `Arc<dyn Trait>` stays as plain Arc — trait-object semantics differ.
                    if let Some(arc_inner) = type_resolver::extract_single_generic_arg_syn(seg) {
                        if let syn::Type::Path(inner_path) = &*arc_inner {
                            if let Some(inner_seg) = inner_path.path.segments.last() {
                                let inner_ident = inner_seg.ident.to_string();
                                if inner_ident == "Mutex" || inner_ident == "RwLock" {
                                    return CoreWrapper::ArcMutex;
                                }
                            }
                        }
                    }
                    return CoreWrapper::Arc;
                }
                _ => {}
            }
        }
    }
    CoreWrapper::None
}

/// Detect if a Vec's inner type is wrapped in Arc (e.g., `Vec<Arc<T>>`).
pub(crate) fn detect_vec_inner_core_wrapper(ty: &syn::Type) -> crate::core::ir::CoreWrapper {
    use crate::core::ir::CoreWrapper;
    // Unwrap Option<Vec<Arc<T>>> → check Vec inner
    let check_ty = if let syn::Type::Path(p) = ty {
        if let Some(seg) = p.path.segments.last() {
            if seg.ident == "Option" {
                type_resolver::extract_single_generic_arg_syn(seg)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let ty_ref = check_ty.as_deref().unwrap_or(ty);

    if let syn::Type::Path(p) = ty_ref {
        if let Some(seg) = p.path.segments.last() {
            if seg.ident == "Vec" {
                if let Some(vec_inner) = type_resolver::extract_single_generic_arg_syn(seg) {
                    if let Some(ident) = outermost_ident(&vec_inner) {
                        if ident == "Arc" {
                            return CoreWrapper::Arc;
                        }
                    }
                }
            }
        }
    }
    CoreWrapper::None
}

/// If the resolved type is `TypeRef::Optional(inner)`, unwrap it and mark as optional.
pub(crate) fn unwrap_optional(ty: TypeRef) -> (TypeRef, bool) {
    match ty {
        TypeRef::Optional(inner) => (*inner, true),
        other => (other, false),
    }
}
