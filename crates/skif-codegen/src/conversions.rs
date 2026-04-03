use ahash::AHashSet;
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, TypeDef, TypeRef};
use std::fmt::Write;

/// Build the set of types that can have core→binding From safely generated.
/// More permissive than binding→core: allows sanitized fields (uses format!("{:?}")).
pub fn core_to_binding_convertible_types(surface: &ApiSurface) -> AHashSet<String> {
    let convertible_enums: AHashSet<&str> = surface
        .enums
        .iter()
        .filter(|e| can_generate_enum_conversion(e))
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

    // Start with all non-opaque types that don't have sanitized fields as candidates.
    // Types with sanitized fields can't have binding→core From (lossy conversion),
    // and excluding them here ensures the transitive closure also removes types
    // containing Vec<SanitizedType> or Named(SanitizedType) fields.
    let mut convertible: AHashSet<String> = surface
        .types
        .iter()
        .filter(|t| !t.is_opaque && !has_sanitized_fields(t))
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

fn is_field_convertible(ty: &TypeRef, convertible_enums: &AHashSet<&str>, known_types: &AHashSet<&str>) -> bool {
    match ty {
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Unit => true,
        TypeRef::Json => false,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_field_convertible(inner, convertible_enums, known_types),
        TypeRef::Map(k, v) => {
            is_field_convertible(k, convertible_enums, known_types)
                && is_field_convertible(v, convertible_enums, known_types)
        }
        // Unit-variant enums and known types (including opaques, which use Arc wrap/unwrap) are convertible.
        TypeRef::Named(name) => convertible_enums.contains(name.as_str()) || known_types.contains(name.as_str()),
    }
}

/// Check if an enum can have From/Into safely generated.
/// Supports unit-variant enums and enums whose data variants contain only
/// simple convertible field types (primitives, String, Bytes, Path, Unit).
pub fn can_generate_enum_conversion(enum_def: &EnumDef) -> bool {
    enum_def
        .variants
        .iter()
        .all(|v| v.fields.iter().all(|f| is_simple_type(&f.ty)))
}

/// Returns true for types that are trivially convertible without needing
/// to consult the convertible_enums/known_types sets.
fn is_simple_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Unit => true,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => is_simple_type(inner),
        TypeRef::Map(k, v) => is_simple_type(k) && is_simple_type(v),
        TypeRef::Named(_) | TypeRef::Json => false,
    }
}

/// Returns true if fields represent a tuple variant (positional: _0, _1, ...).
pub fn is_tuple_variant(fields: &[FieldDef]) -> bool {
    !fields.is_empty()
        && fields[0]
            .name
            .strip_prefix('_')
            .is_some_and(|rest: &str| rest.chars().all(|c: char| c.is_ascii_digit()))
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

/// Derive the Rust import path from rust_path, replacing hyphens with underscores.
fn core_type_path(typ: &TypeDef, core_import: &str) -> String {
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

/// Generate `impl From<BindingType> for core::Type` (binding -> core).
/// Only valid for types WITHOUT sanitized fields — sanitized fields can't be converted back.
pub fn gen_from_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    let core_path = core_type_path(typ, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_path} {{", typ.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", typ.name).ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = field_conversion_to_core(&field.name, &field.ty, field.optional);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Type> for BindingType` (core -> binding).
pub fn gen_from_core_to_binding(typ: &TypeDef, core_import: &str, opaque_types: &AHashSet<String>) -> String {
    let core_path = core_type_path(typ, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {} {{", typ.name).ok();
    writeln!(out, "    fn from(val: {core_path}) -> Self {{").ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion =
            field_conversion_from_core(&field.name, &field.ty, field.optional, field.sanitized, opaque_types);
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

fn core_enum_path(enum_def: &EnumDef, core_import: &str) -> String {
    let path = enum_def.rust_path.replace('-', "_");
    if path.starts_with(core_import) {
        path
    } else {
        format!("{core_import}::{}", enum_def.name)
    }
}

/// Generate `impl From<BindingEnum> for core::Enum` (binding -> core).
/// Binding enums are always unit-variant-only. Core enums may have data variants,
/// in which case Default::default() is used for fields.
pub fn gen_enum_from_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_path} {{", enum_def.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", enum_def.name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        let arm = binding_to_core_match_arm(&enum_def.name, &variant.name, &variant.fields);
        writeln!(out, "            {arm}").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Enum> for BindingEnum` (core -> binding).
/// Core enums may have data variants; binding enums are always unit-variant-only,
/// so data fields are discarded.
pub fn gen_enum_from_core_to_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {} {{", enum_def.name).ok();
    writeln!(out, "    fn from(val: {core_path}) -> Self {{").ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        let arm = core_to_binding_match_arm(&core_path, &variant.name, &variant.fields);
        writeln!(out, "            {arm}").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Determine the field conversion expression for binding -> core.
pub fn field_conversion_to_core(name: &str, ty: &TypeRef, optional: bool) -> String {
    match ty {
        // Primitives, String, Bytes, Unit, Json -- direct assignment
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Unit | TypeRef::Json => {
            format!("{name}: val.{name}")
        }
        // Path needs .into() — binding uses String, core uses PathBuf
        TypeRef::Path => {
            if optional {
                format!("{name}: val.{name}.map(Into::into)")
            } else {
                format!("{name}: val.{name}.into()")
            }
        }
        // Named type -- needs .into() to convert between binding and core types
        TypeRef::Named(_) => {
            if optional {
                format!("{name}: val.{name}.map(Into::into)")
            } else {
                format!("{name}: val.{name}.into()")
            }
        }
        // Optional with inner
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Named(_) | TypeRef::Path => format!("{name}: val.{name}.map(Into::into)"),
            _ => format!("{name}: val.{name}"),
        },
        // Vec of named types -- map each element
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(_) => {
                format!("{name}: val.{name}.into_iter().map(Into::into).collect()")
            }
            _ => format!("{name}: val.{name}"),
        },
        // Map -- for now direct (both key and value might need conversion)
        TypeRef::Map(_, _) => format!("{name}: val.{name}"),
    }
}

/// Same but for core -> binding direction.
/// Some types are asymmetric (PathBuf→String, sanitized fields need .to_string()).
pub fn field_conversion_from_core(
    name: &str,
    ty: &TypeRef,
    optional: bool,
    sanitized: bool,
    opaque_types: &AHashSet<String>,
) -> String {
    // Sanitized fields: the binding type differs from core. Use format!("{:?}") to convert.
    // When the binding type is Vec<String> (sanitized from Vec<Unknown>), map each element.
    if sanitized {
        // Check if binding type is Vec<String> (inner was sanitized from Named→String)
        if let TypeRef::Vec(inner) = ty {
            if matches!(inner.as_ref(), TypeRef::String) {
                return format!("{name}: val.{name}.iter().map(|v| format!(\"{{:?}}\", v)).collect()");
            }
        }
        if optional {
            return format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))");
        }
        return format!("{name}: format!(\"{{:?}}\", val.{name})");
    }
    match ty {
        // Path: core uses PathBuf, binding uses String — PathBuf→String needs special handling
        TypeRef::Path => {
            if optional {
                format!("{name}: val.{name}.map(|p| p.to_string_lossy().to_string())")
            } else {
                format!("{name}: val.{name}.to_string_lossy().to_string()")
            }
        }
        TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Path) => {
            format!("{name}: val.{name}.map(|p| p.to_string_lossy().to_string())")
        }
        // Opaque Named types: wrap in Arc to create the binding wrapper
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            if optional {
                format!("{name}: val.{name}.map(|v| {n} {{ inner: std::sync::Arc::new(v) }})")
            } else {
                format!("{name}: {n} {{ inner: std::sync::Arc::new(val.{name}) }}")
            }
        }
        // Everything else is symmetric
        _ => field_conversion_to_core(name, ty, optional),
    }
}

// Suppress dead_code warning for field_conversion_from_core's `_optional` usage
// through the delegation to field_conversion_to_core.

#[cfg(test)]
mod tests {
    use super::*;
    use skif_core::ir::*;

    fn simple_type() -> TypeDef {
        TypeDef {
            name: "Config".to_string(),
            rust_path: "my_crate::Config".to_string(),
            fields: vec![
                FieldDef {
                    name: "name".into(),
                    ty: TypeRef::String,
                    optional: false,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "timeout".into(),
                    ty: TypeRef::Primitive(PrimitiveType::U64),
                    optional: true,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
                FieldDef {
                    name: "backend".into(),
                    ty: TypeRef::Named("Backend".into()),
                    optional: true,
                    default: None,
                    doc: String::new(),
                    sanitized: false,
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            is_trait: false,
            doc: String::new(),
            cfg: None,
        }
    }

    fn simple_enum() -> EnumDef {
        EnumDef {
            name: "Backend".to_string(),
            rust_path: "my_crate::Backend".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Cpu".into(),
                    fields: vec![],
                    doc: String::new(),
                },
                EnumVariant {
                    name: "Gpu".into(),
                    fields: vec![],
                    doc: String::new(),
                },
            ],
            doc: String::new(),
            cfg: None,
        }
    }

    #[test]
    fn test_from_binding_to_core() {
        let typ = simple_type();
        let result = gen_from_binding_to_core(&typ, "my_crate");
        assert!(result.contains("impl From<Config> for my_crate::Config"));
        assert!(result.contains("name: val.name"));
        assert!(result.contains("timeout: val.timeout"));
        assert!(result.contains("backend: val.backend.map(Into::into)"));
    }

    #[test]
    fn test_from_core_to_binding() {
        let typ = simple_type();
        let result = gen_from_core_to_binding(&typ, "my_crate", &AHashSet::new());
        assert!(result.contains("impl From<my_crate::Config> for Config"));
    }

    #[test]
    fn test_enum_from_binding_to_core() {
        let enum_def = simple_enum();
        let result = gen_enum_from_binding_to_core(&enum_def, "my_crate");
        assert!(result.contains("impl From<Backend> for my_crate::Backend"));
        assert!(result.contains("Backend::Cpu => Self::Cpu"));
        assert!(result.contains("Backend::Gpu => Self::Gpu"));
    }

    #[test]
    fn test_enum_from_core_to_binding() {
        let enum_def = simple_enum();
        let result = gen_enum_from_core_to_binding(&enum_def, "my_crate");
        assert!(result.contains("impl From<my_crate::Backend> for Backend"));
        assert!(result.contains("my_crate::Backend::Cpu => Self::Cpu"));
        assert!(result.contains("my_crate::Backend::Gpu => Self::Gpu"));
    }
}
