use ahash::AHashSet;
use skif_core::ir::{ApiSurface, EnumDef, TypeDef, TypeRef};
use std::fmt::Write;

/// Build the set of types that can have From/Into safely generated.
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

    // Start with all non-opaque types as candidates
    let mut convertible: AHashSet<String> = surface
        .types
        .iter()
        .filter(|t| !t.is_opaque)
        .map(|t| t.name.clone())
        .collect();

    // Iteratively remove types whose fields reference non-convertible Named types
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot: Vec<String> = convertible.iter().cloned().collect();
        for type_name in &snapshot {
            if let Some(typ) = surface.types.iter().find(|t| t.name == *type_name) {
                let ok = typ
                    .fields
                    .iter()
                    .all(|f| is_field_convertible_with_set(&f.ty, &convertible, &convertible_enums));
                if !ok && convertible.remove(type_name) {
                    changed = true;
                }
            }
        }
    }
    convertible
}

/// Check if a specific type is in the convertible set.
pub fn can_generate_conversion(typ: &TypeDef, convertible: &AHashSet<String>) -> bool {
    convertible.contains(&typ.name)
}

fn is_field_convertible_with_set(
    ty: &TypeRef,
    convertible: &AHashSet<String>,
    convertible_enums: &AHashSet<&str>,
) -> bool {
    match ty {
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Unit => true,
        TypeRef::Json => false,
        TypeRef::Optional(inner) | TypeRef::Vec(inner) => {
            is_field_convertible_with_set(inner, convertible, convertible_enums)
        }
        TypeRef::Map(k, v) => {
            is_field_convertible_with_set(k, convertible, convertible_enums)
                && is_field_convertible_with_set(v, convertible, convertible_enums)
        }
        // Only unit-variant enums are safe for auto-conversion in From/Into.
        TypeRef::Named(name) => convertible_enums.contains(name.as_str()),
    }
}

/// Check if an enum can have From/Into safely generated.
/// Only simple unit-variant enums are supported.
pub fn can_generate_enum_conversion(enum_def: &EnumDef) -> bool {
    enum_def.variants.iter().all(|v| v.fields.is_empty())
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

/// Generate `impl From<BindingType> for core::Type` (binding -> core).
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
pub fn gen_from_core_to_binding(typ: &TypeDef, core_import: &str) -> String {
    let core_path = core_type_path(typ, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {} {{", typ.name).ok();
    writeln!(out, "    fn from(val: {core_path}) -> Self {{").ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let conversion = field_conversion_from_core(&field.name, &field.ty, field.optional);
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
pub fn gen_enum_from_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_path} {{", enum_def.name).ok();
    writeln!(out, "    fn from(val: {}) -> Self {{", enum_def.name).ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {}::{} => Self::{},",
            enum_def.name, variant.name, variant.name
        )
        .ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Enum> for BindingEnum` (core -> binding).
pub fn gen_enum_from_core_to_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {} {{", enum_def.name).ok();
    writeln!(out, "    fn from(val: {core_path}) -> Self {{").ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {core_path}::{} => Self::{},",
            variant.name, variant.name
        )
        .ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Determine the field conversion expression for binding -> core.
pub fn field_conversion_to_core(name: &str, ty: &TypeRef, optional: bool) -> String {
    match ty {
        // Primitives, String, Bytes, Path, Unit, Json -- direct assignment
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Unit | TypeRef::Json => {
            format!("{name}: val.{name}")
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
            TypeRef::Named(_) => format!("{name}: val.{name}.map(Into::into)"),
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
pub fn field_conversion_from_core(name: &str, ty: &TypeRef, optional: bool) -> String {
    // Same logic -- From/Into is symmetric for our types
    field_conversion_to_core(name, ty, optional)
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
                },
                FieldDef {
                    name: "timeout".into(),
                    ty: TypeRef::Primitive(PrimitiveType::U64),
                    optional: true,
                    default: None,
                    doc: String::new(),
                },
                FieldDef {
                    name: "backend".into(),
                    ty: TypeRef::Named("Backend".into()),
                    optional: true,
                    default: None,
                    doc: String::new(),
                },
            ],
            methods: vec![],
            is_opaque: false,
            is_clone: true,
            doc: String::new(),
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
        let result = gen_from_core_to_binding(&typ, "my_crate");
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
