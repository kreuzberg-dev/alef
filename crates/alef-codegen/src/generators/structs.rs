use crate::builder::StructBuilder;
use crate::generators::RustBindingConfig;
use crate::type_mapper::TypeMapper;
use alef_core::ir::{TypeDef, TypeRef};
use std::fmt::Write;

/// Check if any two field names are similar enough to trigger clippy::similar_names.
/// This detects patterns like "sub_symbol" and "sup_symbol" (differ by 1-2 chars).
fn has_similar_names(names: &[&String]) -> bool {
    for (i, &name1) in names.iter().enumerate() {
        for &name2 in &names[i + 1..] {
            // Simple heuristic: if names differ by <= 2 characters and have same length, flag it
            if name1.len() == name2.len() && diff_count(name1, name2) <= 2 {
                return true;
            }
        }
    }
    false
}

/// Count how many characters differ between two strings of equal length.
fn diff_count(s1: &str, s2: &str) -> usize {
    s1.chars().zip(s2.chars()).filter(|(c1, c2)| c1 != c2).count()
}

/// Generate a struct definition using the builder, with a per-field attribute callback.
///
/// `extra_field_attrs` is called for each field and returns additional `#[...]` attributes to
/// prepend (beyond `cfg.field_attrs`). Pass `|_| vec![]` to use the default behaviour.
pub fn gen_struct_with_per_field_attrs(
    typ: &TypeDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    extra_field_attrs: impl Fn(&alef_core::ir::FieldDef) -> Vec<String>,
) -> String {
    let mut sb = StructBuilder::new(&typ.name);
    for attr in cfg.struct_attrs {
        sb.add_attr(attr);
    }

    // Check if struct has similar field names (e.g., sub_symbol and sup_symbol)
    let field_names: Vec<_> = typ.fields.iter().filter(|f| f.cfg.is_none()).map(|f| &f.name).collect();
    if has_similar_names(&field_names) {
        sb.add_attr("allow(clippy::similar_names)");
    }

    for d in cfg.struct_derives {
        sb.add_derive(d);
    }
    if typ.has_default {
        sb.add_derive("Default");
    }
    if cfg.has_serde {
        sb.add_derive("serde::Serialize");
    }
    for field in &typ.fields {
        if field.cfg.is_some() {
            continue;
        }
        let force_optional = cfg.option_duration_on_defaults
            && typ.has_default
            && !field.optional
            && matches!(field.ty, TypeRef::Duration);
        let ty = if field.optional || force_optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        let mut attrs: Vec<String> = cfg.field_attrs.iter().map(|a| a.to_string()).collect();
        attrs.extend(extra_field_attrs(field));
        sb.add_field_with_doc(&field.name, &ty, attrs, &field.doc);
    }
    sb.build()
}

/// Generate a struct definition using the builder.
pub fn gen_struct(typ: &TypeDef, mapper: &dyn TypeMapper, cfg: &RustBindingConfig) -> String {
    let mut sb = StructBuilder::new(&typ.name);
    for attr in cfg.struct_attrs {
        sb.add_attr(attr);
    }

    // Check if struct has similar field names (e.g., sub_symbol and sup_symbol)
    let field_names: Vec<_> = typ.fields.iter().filter(|f| f.cfg.is_none()).map(|f| &f.name).collect();
    if has_similar_names(&field_names) {
        sb.add_attr("allow(clippy::similar_names)");
    }

    for d in cfg.struct_derives {
        sb.add_derive(d);
    }
    // Types with has_default get #[derive(Default)] — all fields use Default::default()
    // so the manual impl is unnecessary and triggers clippy::derivable_impls.
    if typ.has_default {
        sb.add_derive("Default");
    }
    if cfg.has_serde {
        sb.add_derive("serde::Serialize");
    }
    for field in &typ.fields {
        // Skip cfg-gated fields — they depend on features that may not be enabled
        // for this binding crate. Including them would require the binding struct to
        // handle conditional compilation which struct literal initializers can't express.
        if field.cfg.is_some() {
            continue;
        }
        // When option_duration_on_defaults is set, wrap non-optional Duration fields in
        // Option<u64> for has_default types so the binding constructor can accept None
        // and the From conversion falls back to the core type's Default.
        let force_optional = cfg.option_duration_on_defaults
            && typ.has_default
            && !field.optional
            && matches!(field.ty, TypeRef::Duration);
        let ty = if field.optional || force_optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        let attrs: Vec<String> = cfg.field_attrs.iter().map(|a| a.to_string()).collect();
        sb.add_field_with_doc(&field.name, &ty, attrs, &field.doc);
    }
    sb.build()
}

/// Generate a `Default` impl for a non-opaque binding struct with `has_default`.
/// All fields use their type's Default::default().
/// Optional fields use None instead of Default::default().
/// This enables the struct to be used with `unwrap_or_default()` in config constructors.
pub fn gen_struct_default_impl(typ: &TypeDef, name_prefix: &str) -> String {
    let full_name = format!("{}{}", name_prefix, typ.name);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl Default for {} {{", full_name).ok();
    writeln!(out, "    fn default() -> Self {{").ok();
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        if field.cfg.is_some() {
            continue;
        }
        let default_val = match &field.ty {
            TypeRef::Optional(_) => "None".to_string(),
            _ => "Default::default()".to_string(),
        };
        writeln!(out, "            {}: {},", field.name, default_val).ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate an opaque wrapper struct with `inner: Arc<core::Type>`.
/// For trait types, uses `Arc<dyn Type + Send + Sync>`.
pub fn gen_opaque_struct(typ: &TypeDef, cfg: &RustBindingConfig) -> String {
    let mut out = String::with_capacity(512);
    if !cfg.struct_derives.is_empty() {
        writeln!(out, "#[derive(Clone)]").ok();
    }
    for attr in cfg.struct_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    writeln!(out, "pub struct {} {{", typ.name).ok();
    let core_path = typ.rust_path.replace('-', "_");
    if typ.is_trait {
        writeln!(out, "    inner: Arc<dyn {core_path} + Send + Sync>,").ok();
    } else {
        writeln!(out, "    inner: Arc<{core_path}>,").ok();
    }
    write!(out, "}}").ok();
    out
}

/// Generate an opaque wrapper struct with `inner: Arc<core::Type>` and a `Js` prefix.
pub fn gen_opaque_struct_prefixed(typ: &TypeDef, cfg: &RustBindingConfig, prefix: &str) -> String {
    let mut out = String::with_capacity(512);
    if !cfg.struct_derives.is_empty() {
        writeln!(out, "#[derive(Clone)]").ok();
    }
    for attr in cfg.struct_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    let core_path = typ.rust_path.replace('-', "_");
    writeln!(out, "pub struct {}{} {{", prefix, typ.name).ok();
    if typ.is_trait {
        writeln!(out, "    inner: Arc<dyn {core_path} + Send + Sync>,").ok();
    } else {
        writeln!(out, "    inner: Arc<{core_path}>,").ok();
    }
    write!(out, "}}").ok();
    out
}
