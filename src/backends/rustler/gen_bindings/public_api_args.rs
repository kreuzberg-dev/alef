use crate::core::ir::{ParamDef, TypeRef};
use ahash::AHashSet;

pub(in crate::backends::rustler::gen_bindings) fn json_encode_param_indices(
    params: &[ParamDef],
    default_types: &AHashSet<String>,
) -> AHashSet<usize> {
    params
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| match &param.ty {
            TypeRef::Vec(inner) => match inner.as_ref() {
                TypeRef::Named(inner_name) if default_types.contains(inner_name) => Some(idx),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

pub(in crate::backends::rustler::gen_bindings) fn nif_arg(
    index: usize,
    param: &str,
    json_encode_params: &AHashSet<usize>,
) -> String {
    if json_encode_params.contains(&index) {
        format!("Jason.encode!({param})")
    } else {
        param.to_string()
    }
}

pub(in crate::backends::rustler::gen_bindings) fn keyword_nif_arg(
    index: usize,
    param: &str,
    json_encode_params: &AHashSet<usize>,
) -> String {
    if json_encode_params.contains(&index) {
        format!("case Keyword.get(opts, :{param}) do nil -> nil; v -> Jason.encode!(v) end")
    } else {
        format!("Keyword.get(opts, :{param})")
    }
}
