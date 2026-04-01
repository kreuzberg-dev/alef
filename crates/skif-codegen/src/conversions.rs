use skif_core::ir::{EnumDef, TypeDef, TypeRef};
use std::fmt::Write;

/// Generate `impl From<BindingType> for core::Type` (binding -> core).
pub fn gen_from_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{}> for {core_import}::{} {{", typ.name, typ.name).unwrap();
    writeln!(out, "    fn from(val: {}) -> Self {{", typ.name).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in &typ.fields {
        let conversion = field_conversion_to_core(&field.name, &field.ty, field.optional);
        writeln!(out, "            {conversion},").unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<core::Type> for BindingType` (core -> binding).
pub fn gen_from_core_to_binding(typ: &TypeDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_import}::{}> for {} {{", typ.name, typ.name).unwrap();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", typ.name).unwrap();
    writeln!(out, "        Self {{").unwrap();
    for field in &typ.fields {
        let conversion = field_conversion_from_core(&field.name, &field.ty, field.optional);
        writeln!(out, "            {conversion},").unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<BindingEnum> for core::Enum` (binding -> core).
pub fn gen_enum_from_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    writeln!(
        out,
        "impl From<{}> for {core_import}::{} {{",
        enum_def.name, enum_def.name
    )
    .unwrap();
    writeln!(out, "    fn from(val: {}) -> Self {{", enum_def.name).unwrap();
    writeln!(out, "        match val {{").unwrap();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {}::{} => Self::{},",
            enum_def.name, variant.name, variant.name
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Generate `impl From<core::Enum> for BindingEnum` (core -> binding).
pub fn gen_enum_from_core_to_binding(enum_def: &EnumDef, core_import: &str) -> String {
    let mut out = String::with_capacity(256);
    writeln!(
        out,
        "impl From<{core_import}::{}> for {} {{",
        enum_def.name, enum_def.name
    )
    .unwrap();
    writeln!(out, "    fn from(val: {core_import}::{}) -> Self {{", enum_def.name).unwrap();
    writeln!(out, "        match val {{").unwrap();
    for variant in &enum_def.variants {
        writeln!(
            out,
            "            {core_import}::{}::{} => Self::{},",
            enum_def.name, variant.name, variant.name
        )
        .unwrap();
    }
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

/// Determine the field conversion expression for binding -> core.
pub fn field_conversion_to_core(name: &str, ty: &TypeRef, optional: bool) -> String {
    match ty {
        // Primitives, String, Bytes, Path, Unit, Json -- direct assignment
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path | TypeRef::Unit | TypeRef::Json => {
            format!("{name}: val.{name}")
        }
        // Named type -- needs .into()
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
