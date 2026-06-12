use crate::core::ir::{CoreWrapper, ParamDef, TypeRef};
use ahash::AHashSet;

pub(in crate::backends::napi::gen_bindings) fn napi_apply_primitive_casts_to_call_args(
    generic_args: &str,
    params: &[ParamDef],
) -> String {
    // Split args by comma and match with params to apply casting
    let args_list: Vec<&str> = generic_args.split(',').map(|s| s.trim()).collect();
    args_list
        .iter()
        .zip(params.iter())
        .map(|(arg, p)| {
            // Special case: Vec<f32> param with is_ref uses the converted variable
            if needs_vec_f32_conversion(&p.ty) && p.is_ref {
                return format!("&{}_f32", p.name);
            }
            match &p.ty {
                TypeRef::Primitive(prim) if needs_napi_cast(prim) => {
                    let core_ty = core_prim_str(prim);
                    if p.optional {
                        // Optional: arg might be like "param.map(...)" so re-apply map
                        if arg.contains(".map(") || arg.contains(".as_") {
                            // Already handled, keep as is
                            arg.to_string()
                        } else {
                            format!("{}.map(|v| v as {})", arg, core_ty)
                        }
                    } else {
                        // Non-optional: simple cast
                        format!("{} as {}", arg, core_ty)
                    }
                }
                _ => arg.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate let bindings for Vec<f32> parameters that need f64→f32 conversion.
pub(in crate::backends::napi::gen_bindings) fn napi_gen_call_args(
    params: &[ParamDef],
    opaque_types: &AHashSet<String>,
) -> String {
    params
        .iter()
        .map(|p| {
            // Special case: Vec<f32> param with is_ref uses the converted variable
            if needs_vec_f32_conversion(&p.ty) && p.is_ref {
                return format!("&{}_f32", p.name);
            }
            match &p.ty {
                TypeRef::Primitive(prim) if needs_napi_cast(prim) => {
                    let core_ty = core_prim_str(prim);
                    if p.optional {
                        format!("{}.map(|v| v as {})", p.name, core_ty)
                    } else {
                        format!("{} as {}", p.name, core_ty)
                    }
                }
                TypeRef::Duration => {
                    if p.optional {
                        format!("{}.map(|v| std::time::Duration::from_millis(v.max(0) as u64))", p.name)
                    } else {
                        format!("std::time::Duration::from_millis({}.max(0) as u64)", p.name)
                    }
                }
                TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                    // When an opaque type param is required by a builder/owned-receiver method,
                    // clone the inner value (Arc dereference) to get an owned copy.
                    // When used as a reference param, borrow `&v.inner` instead.
                    if p.is_ref {
                        if p.optional {
                            format!("{}.as_ref().map(|v| v.inner.as_ref())", p.name)
                        } else {
                            format!("{}.inner.as_ref()", p.name)
                        }
                    } else if p.optional {
                        format!("{}.as_ref().map(|v| (*v.inner).clone())", p.name)
                    } else {
                        format!("(*{}.inner).clone()", p.name)
                    }
                }
                TypeRef::Named(_) => {
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_ref()", p.name)
                        } else {
                            format!("{}.map(Into::into)", p.name)
                        }
                    } else {
                        format!("{}.into()", p.name)
                    }
                }
                TypeRef::String | TypeRef::Char => {
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_deref()", p.name)
                        } else if p.core_wrapper == CoreWrapper::Cow {
                            // Core takes Option<Cow<str>>: convert via .map(Cow::Owned).
                            format!("{}.map(std::borrow::Cow::Owned)", p.name)
                        } else {
                            p.name.clone()
                        }
                    } else if p.is_ref {
                        format!("&{}", p.name)
                    } else if p.core_wrapper == CoreWrapper::Cow {
                        // Core takes Cow<str>: String implements Into<Cow<str>>.
                        format!("{}.into()", p.name)
                    } else {
                        p.name.clone()
                    }
                }
                TypeRef::Path => {
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_deref().map(std::path::Path::new)", p.name)
                        } else {
                            format!("{}.map(std::path::PathBuf::from)", p.name)
                        }
                    } else if p.is_ref {
                        format!("std::path::Path::new(&{})", p.name)
                    } else {
                        format!("std::path::PathBuf::from({})", p.name)
                    }
                }
                TypeRef::Bytes => {
                    // In NAPI, Bytes becomes napi::Buffer, which needs conversion to Vec<u8>
                    // The conversion happens in let bindings, so we use the parameter name as-is
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_deref()", p.name)
                        } else {
                            p.name.clone()
                        }
                    } else if p.is_ref {
                        format!("&{}", p.name)
                    } else {
                        p.name.clone()
                    }
                }
                TypeRef::Vec(inner) => {
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_deref()", p.name)
                        } else {
                            p.name.clone()
                        }
                    } else if p.is_ref
                        && p.vec_inner_is_ref
                        && matches!(inner.as_ref(), TypeRef::String | TypeRef::Char)
                    {
                        // Core expects &[&str]: use the pre-built _refs binding.
                        format!("&{}_refs", p.name)
                    } else if p.is_ref {
                        format!("&{}", p.name)
                    } else {
                        p.name.clone()
                    }
                }
                TypeRef::Map(_, _) => {
                    // When map_is_btree=true, the core expects BTreeMap but the NAPI binding
                    // receives HashMap. Convert inline — the temporary BTreeMap lives for
                    // the duration of the function call statement (Rust temp extension).
                    if p.optional {
                        if p.is_ref {
                            format!("{}.as_ref()", p.name)
                        } else if p.map_is_btree {
                            format!(
                                "{}.map(|m| m.into_iter().collect::<std::collections::BTreeMap<_, _>>())",
                                p.name
                            )
                        } else {
                            p.name.clone()
                        }
                    } else if p.is_ref && p.map_is_btree {
                        format!("&{}.into_iter().collect::<std::collections::BTreeMap<_, _>>()", p.name)
                    } else if p.is_ref {
                        format!("&{}", p.name)
                    } else if p.map_is_btree {
                        format!("{}.into_iter().collect::<std::collections::BTreeMap<_, _>>()", p.name)
                    } else {
                        p.name.clone()
                    }
                }
                _ => p.name.clone(),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn needs_vec_f32_conversion(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(crate::core::ir::PrimitiveType::F32)))
}

pub(in crate::backends::napi::gen_bindings) fn needs_napi_cast(p: &crate::core::ir::PrimitiveType) -> bool {
    // U32 maps to u32 in both NAPI and core, so no cast needed.
    // U64/Usize/Isize map to i64 in NAPI but u64/usize/isize in core.
    // F32 maps to f64 in NAPI but f32 in core.
    matches!(
        p,
        crate::core::ir::PrimitiveType::U64
            | crate::core::ir::PrimitiveType::Usize
            | crate::core::ir::PrimitiveType::Isize
            | crate::core::ir::PrimitiveType::F32
    )
}

pub(in crate::backends::napi::gen_bindings) fn core_prim_str(p: &crate::core::ir::PrimitiveType) -> &'static str {
    match p {
        crate::core::ir::PrimitiveType::U64 => "u64",
        crate::core::ir::PrimitiveType::Usize => "usize",
        crate::core::ir::PrimitiveType::Isize => "isize",
        crate::core::ir::PrimitiveType::F32 => "f32",
        _ => unreachable!(),
    }
}

/// Check if a type is Vec<u8> or Bytes (which becomes napi::Buffer).
pub(super) fn is_bytes_param(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Bytes)
        || matches!(ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Primitive(crate::core::ir::PrimitiveType::U8)))
}
