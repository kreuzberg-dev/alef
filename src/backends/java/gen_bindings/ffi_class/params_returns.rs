use crate::codegen::naming::to_java_name;
use crate::core::ir::{FunctionDef, ParamDef, TypeRef};
use std::collections::HashSet;

use super::super::helpers::is_bridge_param_java;

pub(super) fn param_type_name(param: &ParamDef) -> Option<&str> {
    match &param.ty {
        TypeRef::Named(name) => Some(name.as_str()),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn public_arg_names(
    func: &FunctionDef,
    bridge_param_names: &HashSet<String>,
    bridge_type_aliases: &HashSet<String>,
) -> Vec<String> {
    func.params
        .iter()
        .filter(|p| !is_bridge_param_java(p, bridge_param_names, bridge_type_aliases))
        .map(|p| to_java_name(&p.name))
        .collect()
}

pub(super) fn return_type_name(return_type: &TypeRef) -> Option<&str> {
    match return_type {
        TypeRef::Named(name) => Some(name.as_str()),
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(name) => Some(name.as_str()),
            _ => None,
        },
        _ => None,
    }
}
