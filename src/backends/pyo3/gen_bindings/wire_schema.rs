//! Generation of `__ALEF_WIRE_*` rename-schema consts for data-enum payload DTO coercion.
//!
//! A data-enum per-variant constructor accepts a public payload (a `@dataclass` or a `dict`) for a
//! config-DTO field and coerces it into the core type via the `__alef_coerce_dto` runtime helper
//! (see [`crate::codegen::generators::PYO3_DTO_COERCE_HELPER`]). A `@dataclass` carries Rust field
//! names, while serde deserializes by wire name, so the helper rewrites the keys using a per-DTO
//! `&[__AlefAlias]` schema emitted here. The schema honors both `#[serde(rename)]` and
//! `#[serde(rename_all)]` (sourced from the centralized [`crate::codegen::naming`] transforms — the
//! same single source of truth the Python `_to_rust_*` converters use) and recurses through nested
//! DTOs, sequences, maps, and optionals so renamed fields at any depth survive the round-trip.

use crate::codegen::generators::{
    coercible_payload, collect_variant_constructors, enum_has_data_variants, pyo3_wire_schema_const_name,
};
use crate::codegen::naming::wire_field_name;
use crate::codegen::shared::binding_fields;
use crate::core::ir::{ApiSurface, TypeDef};
use ahash::{AHashMap, AHashSet};

/// One emitted `__AlefAlias` row.
struct AliasEntry {
    rust: String,
    wire: String,
    kind: &'static str,
    /// Nested schema const name, or `&[]` for a leaf (rename-only) field.
    nested: String,
}

/// Emit the `__ALEF_WIRE_*` rename-schema consts for every coercible DTO reachable from a data-enum
/// variant-constructor payload field (transitively through nested coercible DTO fields). Returns an
/// empty string when no coercion is in play. Cyclic type graphs are broken at back-edges (the deeper
/// occurrence references `&[]`) so the emitted consts never form a const-evaluation cycle.
pub(super) fn gen_wire_schema_consts(api: &ApiSurface, coercible_dto_names: &AHashSet<&str>) -> String {
    if coercible_dto_names.is_empty() {
        return String::new();
    }
    let types: AHashMap<&str, &TypeDef> = api.types.iter().map(|t| (t.name.as_str(), t)).collect();

    // Seed from every coercible Named/Optional(Named) payload field of a generated variant ctor.
    let mut seeds: Vec<String> = Vec::new();
    for e in &api.enums {
        if !enum_has_data_variants(e) {
            continue;
        }
        for ctor in collect_variant_constructors(e) {
            for p in &ctor.params {
                // Any coercible payload shape (a DTO, or a list/map of DTOs) needs its schema const.
                if let Some((dto, _)) = coercible_payload(&p.ty, coercible_dto_names) {
                    if !seeds.iter().any(|s| s == dto) {
                        seeds.push(dto.to_string());
                    }
                }
            }
        }
    }
    if seeds.is_empty() {
        return String::new();
    }

    // DFS building one const per reachable type, breaking cycles at back-edges.
    let mut built: Vec<(String, Vec<AliasEntry>)> = Vec::new();
    let mut done: AHashSet<String> = AHashSet::new();
    for seed in &seeds {
        build_type(
            seed,
            &types,
            coercible_dto_names,
            &mut Vec::new(),
            &mut done,
            &mut built,
        );
    }

    // Deterministic output order.
    built.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (const_name, entries) in &built {
        let rendered_entries: Vec<minijinja::Value> = entries
            .iter()
            .map(|e| {
                minijinja::context! {
                    rust => &e.rust,
                    wire => &e.wire,
                    kind => e.kind,
                    nested => &e.nested,
                }
            })
            .collect();
        out.push_str(&crate::backends::pyo3::template_env::render(
            "pyo3_wire_schema_const.jinja",
            minijinja::context! {
                const_name => const_name,
                entries => rendered_entries,
            },
        ));
        out.push('\n');
    }
    out
}

/// Build (and memoize) the schema const for `type_name`, recursing into nested coercible DTOs.
/// `path` tracks the current DFS ancestry so a back-edge to a type still being built is broken
/// (`&[]`) — guaranteeing the const-reference graph stays acyclic.
fn build_type(
    type_name: &str,
    types: &AHashMap<&str, &TypeDef>,
    coercible: &AHashSet<&str>,
    path: &mut Vec<String>,
    done: &mut AHashSet<String>,
    built: &mut Vec<(String, Vec<AliasEntry>)>,
) {
    if done.contains(type_name) {
        return;
    }
    let Some(typ) = types.get(type_name) else {
        // No TypeDef (e.g. an opaque or externally-defined type): emit an empty schema so the
        // referencing const still resolves.
        done.insert(type_name.to_string());
        built.push((pyo3_wire_schema_const_name(type_name), Vec::new()));
        return;
    };
    done.insert(type_name.to_string());
    path.push(type_name.to_string());

    let rename_all = typ.serde_rename_all.as_deref();
    let mut entries: Vec<AliasEntry> = Vec::new();
    for field in binding_fields(&typ.fields) {
        let wire = wire_field_name(&field.name, field.serde_rename.as_deref(), rename_all);
        match coercible_payload(&field.ty, coercible) {
            Some((dto_name, shape)) => {
                // Break cycles: a back-edge to a type currently on the DFS path references `&[]`.
                let nested = if path.iter().any(|p| p == dto_name) {
                    "&[]".to_string()
                } else {
                    build_type(dto_name, types, coercible, path, done, built);
                    pyo3_wire_schema_const_name(dto_name)
                };
                entries.push(AliasEntry {
                    rust: field.name.clone(),
                    wire,
                    kind: shape.wire_kind(),
                    nested,
                });
            }
            None => {
                // Leaf field: only needs an entry when its wire name differs from the Rust name.
                if wire != field.name {
                    entries.push(AliasEntry {
                        rust: field.name.clone(),
                        wire,
                        kind: "Leaf",
                        nested: "&[]".to_string(),
                    });
                }
            }
        }
    }

    path.pop();
    built.push((pyo3_wire_schema_const_name(type_name), entries));
}

#[cfg(test)]
mod tests;
