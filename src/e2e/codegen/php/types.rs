//! PHP e2e helper types and type-classification helpers.
//! These utilities were previously defined in `php.rs` during extractor phase.

use crate::core::config::e2e::CallConfig;
use crate::core::ir::TypeRef;
use crate::e2e::field_access::PhpGetterMap;
use std::collections::{HashMap, HashSet};

/// Compute the getter mapping for PHP result/fixture field traversal.
///
/// Returns true when a field is scalar-compatible for ext-php-rs `#[php(prop)]` — that
/// is, the mapped Rust type implements `IntoZval` + `FromZval` automatically without
/// a manual getter. Mirrors `is_php_prop_scalar_with_enums` from
/// `alef-backend-php/src/gen_bindings/types.rs`.
///
/// Scalar-compatible: primitives, String, Char, Duration (→ u64), Path (→ String),
/// Option<scalar>, Vec<primitive|String|Char>, unit-variant enums (mapped to String).
/// Non-scalar: Named struct, Map, nested Vec<Named>, Json, Bytes.
/// Build a per-`(owner_type, field_name)` PHP getter classification plus chain-resolution
/// metadata from the IR's `TypeDef`s.
///
/// For each type, marks fields as needing getter syntax when their mapped Rust type is
/// non-scalar in PHP (`Named` struct, `Vec<Named>`, `Map`, `Json`, `Bytes`).
/// Also records each field's referenced `Named` inner type so the resolver can advance
/// the current-type cursor as it walks multi-segment paths like `outer.inner.content`.
///
/// `root_type` is derived (best-effort) from a `result_type` override on any backend
/// (`c`, `csharp`, `java`, `kotlin`, `go`, `php`) and otherwise inferred by matching
/// `result_fields` against `TypeDef.fields`. When no root can be determined, chain
/// resolution falls back to the legacy bare-name union (sound only when no field names
/// collide across types).
pub(super) fn build_php_getter_map(
    type_defs: &[crate::core::ir::TypeDef],
    enum_names: &HashSet<String>,
    call: &CallConfig,
    result_fields: &HashSet<String>,
) -> PhpGetterMap {
    let mut getters: HashMap<String, HashSet<String>> = HashMap::new();
    let mut field_types: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut all_fields: HashMap<String, HashSet<String>> = HashMap::new();
    for td in type_defs {
        let mut getter_fields: HashSet<String> = HashSet::new();
        let mut field_type_map: HashMap<String, String> = HashMap::new();
        let mut td_all_fields: HashSet<String> = HashSet::new();
        for f in &td.fields {
            td_all_fields.insert(f.name.clone());
            if !is_php_scalar(&f.ty, enum_names) {
                getter_fields.insert(f.name.clone());
            }
            if let Some(named) = inner_named(&f.ty) {
                field_type_map.insert(f.name.clone(), named);
            }
        }
        getters.insert(td.name.clone(), getter_fields);
        all_fields.insert(td.name.clone(), td_all_fields);
        if !field_type_map.is_empty() {
            field_types.insert(td.name.clone(), field_type_map);
        }
    }
    let root_type = derive_root_type(call, type_defs, result_fields);
    PhpGetterMap {
        getters,
        field_types,
        root_type,
        all_fields,
    }
}

/// Unwrap `Option<T>` / `Vec<T>` to the innermost `Named` type name, if any.
/// Returns `None` for primitives, scalars, `Map`, `Json`, `Bytes`, and `Unit`.
pub(super) fn inner_named(ty: &TypeRef) -> Option<String> {
    match ty {
        TypeRef::Named(n) => Some(n.clone()),
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => inner_named(inner),
        _ => None,
    }
}

/// Derive the IR type name backing the result variable in PHP-generated assertions.
///
/// Lookup order:
/// 1. `call.overrides[<lang>]`.result_type for any of `php`, `c`, `csharp`,
///    `java`, `kotlin`, `go` (first non-empty wins).
/// 2. Type-defs whose field names form a superset of `result_fields` (when exactly
///    one matches).
///
/// Returns `None` when neither yields a definitive answer; callers fall back to the
/// legacy bare-name union behaviour.
pub(super) fn derive_root_type(
    call: &CallConfig,
    type_defs: &[crate::core::ir::TypeDef],
    result_fields: &HashSet<String>,
) -> Option<String> {
    const LOOKUP_LANGS: &[&str] = &["php", "c", "csharp", "java", "kotlin", "go"];
    for lang in LOOKUP_LANGS {
        if let Some(o) = call.overrides.get(*lang)
            && let Some(rt) = o.result_type.as_deref()
            && !rt.is_empty()
            && type_defs.iter().any(|td| td.name == rt)
        {
            return Some(rt.to_string());
        }
    }
    if result_fields.is_empty() {
        return None;
    }
    let matches: Vec<&crate::core::ir::TypeDef> = type_defs
        .iter()
        .filter(|td| {
            let names: HashSet<&str> = td.fields.iter().map(|f| f.name.as_str()).collect();
            result_fields.iter().all(|rf| names.contains(rf.as_str()))
        })
        .collect();
    if matches.len() == 1 {
        return Some(matches[0].name.clone());
    }
    None
}

pub(super) fn is_php_scalar(ty: &TypeRef, enum_names: &HashSet<String>) -> bool {
    match ty {
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Char | TypeRef::Duration | TypeRef::Path => true,
        TypeRef::Optional(inner) => is_php_scalar(inner, enum_names),
        TypeRef::Vec(inner) => {
            matches!(inner.as_ref(), TypeRef::Primitive(_) | TypeRef::String | TypeRef::Char)
                || matches!(inner.as_ref(), TypeRef::Named(n) if enum_names.contains(n))
        }
        TypeRef::Named(n) if enum_names.contains(n) => true,
        TypeRef::Named(_) | TypeRef::Map(_, _) | TypeRef::Json | TypeRef::Bytes | TypeRef::Unit => false,
    }
}
