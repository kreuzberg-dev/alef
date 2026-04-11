use ahash::AHashSet;
use alef_core::ir::{ApiSurface, EnumDef, FieldDef, PrimitiveType, TypeDef, TypeRef};

/// Returns true if a primitive type needs i64 casting (NAPI/PHP — JS/PHP lack native u64).
pub(crate) fn needs_i64_cast(p: &PrimitiveType) -> bool {
    matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)
}

/// Returns the core primitive type string for cast primitives.
pub(crate) fn core_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 => "u64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
        PrimitiveType::F32 => "f32",
        _ => unreachable!(),
    }
}

/// Returns the binding primitive type string for cast primitives (core→binding direction).
pub(crate) fn binding_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize => "i64",
        _ => unreachable!(),
    }
}

/// Build the set of types that can have core→binding From safely generated.
/// More permissive than binding→core: allows sanitized fields (uses format!("{:?}"))
/// and accepts data enums (data discarded with `..` in match arms).
pub fn core_to_binding_convertible_types(surface: &ApiSurface) -> AHashSet<String> {
    let convertible_enums: AHashSet<&str> = surface
        .enums
        .iter()
        .filter(|e| can_generate_enum_conversion_from_core(e))
        .map(|e| e.name.as_str())
        .collect();

    let opaque_type_names: AHashSet<&str> = surface
        .types
        .iter()
        .filter(|t| t.is_opaque)
        .map(|t| t.name.as_str())
        .collect();

    // All non-opaque types are candidates (sanitized fields use format!("{:?}"))
    let mut convertible: AHashSet<String> = surface
        .types
        .iter()
        .filter(|t| !t.is_opaque)
        .map(|t| t.name.clone())
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<String> = convertible.iter().cloned().collect();
        let mut known: AHashSet<&str> = convertible.iter().map(|s| s.as_str()).collect();
        known.extend(&opaque_type_names);
        let mut to_remove = Vec::new();
        for type_name in &snapshot {
            if let Some(typ) = surface.types.iter().find(|t| t.name == *type_name) {
                let ok = typ
                    .fields
                    .iter()
                    .all(|f| f.sanitized || is_field_convertible(&f.ty, &convertible_enums, &known));
                if !ok {
                    to_remove.push(type_name.clone());
                }
            }
        }
        for name in to_remove {
            if convertible.remove(&name) {
                changed = true;
            }
        }
    }
    convertible
}

/// Build the set of types that can have binding→core From safely generated.
/// Strict: excludes types with sanitized fields (lossy conversion).
/// This is transitive: a type is convertible only if all its Named field types
/// are also convertible (or are enums with From/Into support).
pub fn convertible_types(surface: &ApiSurface) -> AHashSet<String> {
    // Build set of enums that have From/Into impls (unit-variant enums only)
    let convertible_enums: AHashSet<&str> = surface
        .enums
        .iter()
        .filter(|e| can_generate_enum_conversion(e))
        .map(|e| e.name.as_str())
        .collect();

    // Build set of all known type names (including opaques) — opaque Named fields
    // are convertible because we wrap/unwrap them via Arc.
    let _all_type_names: AHashSet<&str> = surface.types.iter().map(|t| t.name.as_str()).collect();

    // Start with all non-opaque types as candidates.
    // Types with sanitized fields use Default::default() for the sanitized field
    // in the binding→core direction (lossy but functional).
    let mut convertible: AHashSet<String> = surface
        .types
        .iter()
        .filter(|t| !t.is_opaque)
        .map(|t| t.name.clone())
        .collect();

    // Set of opaque type names — Named fields referencing opaques are always convertible
    // (they use Arc wrap/unwrap), so include them in the known-types check.
    let opaque_type_names: AHashSet<&str> = surface
        .types
        .iter()
        .filter(|t| t.is_opaque)
        .map(|t| t.name.as_str())
        .collect();

    // Iteratively remove types whose fields reference non-convertible Named types.
    // We check against `convertible ∪ opaque_types` so that types referencing
    // excluded types (e.g. types with sanitized fields) are transitively removed,
    // while opaque Named fields remain valid.
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<String> = convertible.iter().cloned().collect();
        let mut known: AHashSet<&str> = convertible.iter().map(|s| s.as_str()).collect();
        known.extend(&opaque_type_names);
        let mut to_remove = Vec::new();
        for type_name in &snapshot {
            if let Some(typ) = surface.types.iter().find(|t| t.name == *type_name) {
                let ok = typ
                    .fields
                    .iter()
                    .all(|f| is_field_convertible(&f.ty, &convertible_enums, &known));
                if !ok {
                    to_remove.push(type_name.clone());
                }
            }
        }
        for name in to_remove {
            if convertible.remove(&name) {
                changed = true;
            }
        }
    }
    convertible
}

/// Check if a specific type is in the convertible set.
pub fn can_generate_conversion(typ: &TypeDef, convertible: &AHashSet<String>) -> bool {
    convertible.contains(&typ.name)
}

pub(crate) fn is_field_convertible(
    ty: &TypeRef,
    convertible_enums: &AHashSet<&str>,
    known_types: &AHashSet<&str>,
) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Char
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
        TypeRef::Json => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_field_convertible(inner, convertible_enums, known_types),
        TypeRef::Map(k, v) => {
            is_field_convertible(k, convertible_enums, known_types)
                && is_field_convertible(v, convertible_enums, known_types)
        }
        // Tuple types are passthrough — always convertible
        TypeRef::Named(name) if is_tuple_type_name(name) => true,
        // Unit-variant enums and known types (including opaques, which use Arc wrap/unwrap) are convertible.
        TypeRef::Named(name) => convertible_enums.contains(name.as_str()) || known_types.contains(name.as_str()),
    }
}

/// Check if an enum can have From/Into safely generated (both directions).
/// All enums are allowed — data variants use Default::default() for non-simple fields
/// in the binding→core direction.
pub fn can_generate_enum_conversion(enum_def: &EnumDef) -> bool {
    !enum_def.variants.is_empty()
}

/// Check if an enum can have core→binding From safely generated.
/// This is always possible: unit variants map 1:1, data variants discard data with `..`.
pub fn can_generate_enum_conversion_from_core(enum_def: &EnumDef) -> bool {
    // Always possible — data variants are handled by pattern matching with `..`
    !enum_def.variants.is_empty()
}

/// Returns true if fields represent a tuple variant (positional: _0, _1, ...).
pub fn is_tuple_variant(fields: &[FieldDef]) -> bool {
    !fields.is_empty()
        && fields[0]
            .name
            .strip_prefix('_')
            .is_some_and(|rest: &str| rest.chars().all(|c: char| c.is_ascii_digit()))
}

/// Returns true if a TypeDef represents a newtype struct (single unnamed field `_0`).
pub fn is_newtype(typ: &TypeDef) -> bool {
    typ.fields.len() == 1 && typ.fields[0].name == "_0"
}

/// Returns true if a type name looks like a tuple (starts with `(`).
/// Tuple types are passthrough — no conversion needed.
pub(crate) fn is_tuple_type_name(name: &str) -> bool {
    name.starts_with('(')
}

/// Derive the Rust import path from rust_path, replacing hyphens with underscores.
pub fn core_type_path(typ: &TypeDef, core_import: &str) -> String {
    // rust_path is like "liter-llm::tower::RateLimitConfig"
    // We need "liter_llm::tower::RateLimitConfig"
    let path = typ.rust_path.replace('-', "_");
    // If the path starts with the core_import, use it directly
    if path.starts_with(core_import) {
        path
    } else {
        // Fallback: just use core_import::name
        format!("{core_import}::{}", typ.name)
    }
}

/// Check if a type has any sanitized fields (binding→core conversion is lossy).
pub fn has_sanitized_fields(typ: &TypeDef) -> bool {
    typ.fields.iter().any(|f| f.sanitized)
}

/// Derive the Rust import path for an enum, replacing hyphens with underscores.
pub fn core_enum_path(enum_def: &EnumDef, core_import: &str) -> String {
    let path = enum_def.rust_path.replace('-', "_");
    if path.starts_with(core_import) {
        path
    } else {
        format!("{core_import}::{}", enum_def.name)
    }
}

/// Generate a match arm for binding -> core direction.
/// Binding enums are always unit-variant-only. Core enums may have data variants.
/// For data variants: `BindingEnum::Variant => CoreEnum::Variant(Default::default(), ...)`
pub fn binding_to_core_match_arm(binding_prefix: &str, variant_name: &str, fields: &[FieldDef]) -> String {
    if fields.is_empty() {
        format!("{binding_prefix}::{variant_name} => Self::{variant_name},")
    } else if is_tuple_variant(fields) {
        let defaults: Vec<&str> = fields.iter().map(|_| "Default::default()").collect();
        format!(
            "{binding_prefix}::{variant_name} => Self::{variant_name}({}),",
            defaults.join(", ")
        )
    } else {
        let defaults: Vec<String> = fields
            .iter()
            .map(|f| format!("{}: Default::default()", f.name))
            .collect();
        format!(
            "{binding_prefix}::{variant_name} => Self::{variant_name} {{ {} }},",
            defaults.join(", ")
        )
    }
}

/// Generate a match arm for core -> binding direction.
/// Core enums may have data variants; binding enums are always unit-variant-only.
/// For data variants: `CoreEnum::Variant(..) => Self::Variant`
pub fn core_to_binding_match_arm(core_prefix: &str, variant_name: &str, fields: &[FieldDef]) -> String {
    if fields.is_empty() {
        format!("{core_prefix}::{variant_name} => Self::{variant_name},")
    } else if is_tuple_variant(fields) {
        format!("{core_prefix}::{variant_name}(..) => Self::{variant_name},")
    } else {
        format!("{core_prefix}::{variant_name} {{ .. }} => Self::{variant_name},")
    }
}
