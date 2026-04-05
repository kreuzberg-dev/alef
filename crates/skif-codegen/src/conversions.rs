use ahash::AHashSet;
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, PrimitiveType, TypeDef, TypeRef};
use std::fmt::Write;

/// Backend-specific configuration for From/field conversion generation.
/// Enables shared code to handle all backend differences via parameters.
#[derive(Default, Clone)]
pub struct ConversionConfig<'a> {
    /// Prefix for binding type names ("Js" for NAPI/WASM, "" for others).
    pub type_name_prefix: &'a str,
    /// U64/Usize/Isize need `as i64` casts (NAPI, PHP — JS/PHP lack native u64).
    pub cast_large_ints_to_i64: bool,
    /// Enum names mapped to String in the binding layer (PHP only).
    /// Named fields referencing these use `format!("{:?}")` in core→binding.
    pub enum_string_names: Option<&'a AHashSet<String>>,
    /// Map types use JsValue in the binding layer (WASM only).
    /// When true, Map fields use `serde_wasm_bindgen` for conversion instead of
    /// iterator-based collect patterns (JsValue is not iterable).
    pub map_uses_jsvalue: bool,
    /// When true, non-optional fields on defaultable types are wrapped in Option<T>
    /// in the binding struct and need `.unwrap_or_default()` in binding→core From.
    /// Used by NAPI to make JS-facing structs fully optional.
    pub optionalize_defaults: bool,
}

/// Returns true if a primitive type needs i64 casting in NAPI/PHP.
fn needs_i64_cast(p: &PrimitiveType) -> bool {
    matches!(p, PrimitiveType::U64 | PrimitiveType::Usize | PrimitiveType::Isize)
}

/// Returns the core primitive type string for i64-cast primitives.
fn core_prim_str(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U64 => "u64",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
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

fn is_field_convertible(ty: &TypeRef, convertible_enums: &AHashSet<&str>, known_types: &AHashSet<&str>) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
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

/// Check if an enum can have From/Into safely generated (both directions).
/// Supports unit-variant enums and enums whose data variants contain only
/// simple convertible field types (primitives, String, Bytes, Path, Unit).
pub fn can_generate_enum_conversion(enum_def: &EnumDef) -> bool {
    enum_def
        .variants
        .iter()
        .all(|v| v.fields.iter().all(|f| is_simple_type(&f.ty)))
}

/// Check if an enum can have core→binding From safely generated.
/// This is always possible: unit variants map 1:1, data variants discard data with `..`.
pub fn can_generate_enum_conversion_from_core(enum_def: &EnumDef) -> bool {
    // Always possible — data variants are handled by pattern matching with `..`
    !enum_def.variants.is_empty()
}

/// Returns true for types that are trivially convertible without needing
/// to consult the convertible_enums/known_types sets.
fn is_simple_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Primitive(_)
        | TypeRef::String
        | TypeRef::Bytes
        | TypeRef::Path
        | TypeRef::Unit
        | TypeRef::Duration => true,
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

/// Generate `impl From<BindingType> for core::Type` (binding -> core).
/// Sanitized fields use `Default::default()` (lossy but functional).
pub fn gen_from_binding_to_core(typ: &TypeDef, core_import: &str) -> String {
    gen_from_binding_to_core_cfg(typ, core_import, &ConversionConfig::default())
}

/// Generate `impl From<BindingType> for core::Type` with backend-specific config.
pub fn gen_from_binding_to_core_cfg(typ: &TypeDef, core_import: &str, config: &ConversionConfig) -> String {
    let core_path = core_type_path(typ, core_import);
    let binding_name = format!("{}{}", config.type_name_prefix, typ.name);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{binding_name}> for {core_path} {{").ok();
    writeln!(out, "    fn from(val: {binding_name}) -> Self {{").ok();
    writeln!(out, "        Self {{").ok();
    let optionalized = config.optionalize_defaults && typ.has_default;
    for field in &typ.fields {
        let conversion = if field.sanitized {
            format!("{}: Default::default()", field.name)
        } else if optionalized && !field.optional {
            // Field was wrapped in Option<T> for JS ergonomics but core expects T.
            // Use unwrap_or_default() for simple types, unwrap_or_default() + into for Named.
            gen_optionalized_field_to_core(&field.name, &field.ty, config)
        } else {
            field_conversion_to_core_cfg(&field.name, &field.ty, field.optional, config)
        };
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate field conversion for a non-optional field that was optionalized
/// (wrapped in Option<T>) in the binding struct for JS ergonomics.
fn gen_optionalized_field_to_core(name: &str, ty: &TypeRef, config: &ConversionConfig) -> String {
    match ty {
        TypeRef::Named(_) => {
            // Named type: unwrap Option, convert via .into(), or use Default
            format!("{name}: val.{name}.map(Into::into).unwrap_or_default()")
        }
        TypeRef::Primitive(p) if config.cast_large_ints_to_i64 && needs_i64_cast(p) => {
            let core_ty = core_prim_str(p);
            format!("{name}: val.{name}.map(|v| v as {core_ty}).unwrap_or_default()")
        }
        TypeRef::Duration if config.cast_large_ints_to_i64 => {
            format!("{name}: val.{name}.map(|v| std::time::Duration::from_secs(v as u64)).unwrap_or_default()")
        }
        TypeRef::Duration => {
            format!("{name}: val.{name}.map(std::time::Duration::from_secs).unwrap_or_default()")
        }
        TypeRef::Path => {
            format!("{name}: val.{name}.map(Into::into).unwrap_or_default()")
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(_) => {
                format!("{name}: val.{name}.map(|v| v.into_iter().map(Into::into).collect()).unwrap_or_default()")
            }
            _ => format!("{name}: val.{name}.unwrap_or_default()"),
        },
        TypeRef::Map(_, _) => {
            // Collect to handle HashMap↔BTreeMap conversion
            format!("{name}: val.{name}.unwrap_or_default().into_iter().collect()")
        }
        _ => {
            // Simple types (primitives, String, etc): unwrap_or_default()
            format!("{name}: val.{name}.unwrap_or_default()")
        }
    }
}

/// Generate `impl From<core::Type> for BindingType` (core -> binding).
pub fn gen_from_core_to_binding(typ: &TypeDef, core_import: &str, opaque_types: &AHashSet<String>) -> String {
    gen_from_core_to_binding_cfg(typ, core_import, opaque_types, &ConversionConfig::default())
}

/// Generate `impl From<core::Type> for BindingType` with backend-specific config.
pub fn gen_from_core_to_binding_cfg(
    typ: &TypeDef,
    core_import: &str,
    opaque_types: &AHashSet<String>,
    config: &ConversionConfig,
) -> String {
    let core_path = core_type_path(typ, core_import);
    let binding_name = format!("{}{}", config.type_name_prefix, typ.name);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {binding_name} {{").ok();
    writeln!(out, "    fn from(val: {core_path}) -> Self {{").ok();
    let optionalized = config.optionalize_defaults && typ.has_default;
    writeln!(out, "        Self {{").ok();
    for field in &typ.fields {
        let base_conversion = field_conversion_from_core_cfg(
            &field.name,
            &field.ty,
            field.optional,
            field.sanitized,
            opaque_types,
            config,
        );
        // Optionalized non-optional fields need Some() wrapping in core→binding direction
        let conversion = if optionalized && !field.optional {
            // Extract the value expression after "name: " and wrap in Some()
            if let Some(expr) = base_conversion.strip_prefix(&format!("{}: ", field.name)) {
                format!("{}: Some({})", field.name, expr)
            } else {
                base_conversion
            }
        } else {
            base_conversion
        };
        writeln!(out, "            {conversion},").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

pub fn core_enum_path(enum_def: &EnumDef, core_import: &str) -> String {
    let path = enum_def.rust_path.replace('-', "_");
    if path.starts_with(core_import) {
        path
    } else {
        format!("{core_import}::{}", enum_def.name)
    }
}

/// Generate `impl From<BindingEnum> for core::Enum` (binding -> core).
pub fn gen_enum_from_binding_to_core(enum_def: &EnumDef, core_import: &str) -> String {
    gen_enum_from_binding_to_core_cfg(enum_def, core_import, &ConversionConfig::default())
}

/// Generate `impl From<BindingEnum> for core::Enum` with backend-specific config.
pub fn gen_enum_from_binding_to_core_cfg(enum_def: &EnumDef, core_import: &str, config: &ConversionConfig) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let binding_name = format!("{}{}", config.type_name_prefix, enum_def.name);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{binding_name}> for {core_path} {{").ok();
    writeln!(out, "    fn from(val: {binding_name}) -> Self {{").ok();
    writeln!(out, "        match val {{").ok();
    for variant in &enum_def.variants {
        let arm = binding_to_core_match_arm(&binding_name, &variant.name, &variant.fields);
        writeln!(out, "            {arm}").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate `impl From<core::Enum> for BindingEnum` (core -> binding).
pub fn gen_enum_from_core_to_binding(enum_def: &EnumDef, core_import: &str) -> String {
    gen_enum_from_core_to_binding_cfg(enum_def, core_import, &ConversionConfig::default())
}

/// Generate `impl From<core::Enum> for BindingEnum` with backend-specific config.
pub fn gen_enum_from_core_to_binding_cfg(enum_def: &EnumDef, core_import: &str, config: &ConversionConfig) -> String {
    let core_path = core_enum_path(enum_def, core_import);
    let binding_name = format!("{}{}", config.type_name_prefix, enum_def.name);
    let mut out = String::with_capacity(256);
    writeln!(out, "impl From<{core_path}> for {binding_name} {{").ok();
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
        // Duration: binding uses u64 (secs), core uses std::time::Duration
        TypeRef::Duration => {
            if optional {
                format!("{name}: val.{name}.map(std::time::Duration::from_secs)")
            } else {
                format!("{name}: std::time::Duration::from_secs(val.{name})")
            }
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
            TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Named(_)) => {
                format!("{name}: val.{name}.map(|v| v.into_iter().map(Into::into).collect())")
            }
            _ => format!("{name}: val.{name}"),
        },
        // Vec of named types -- map each element
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(_) => {
                if optional {
                    format!("{name}: val.{name}.map(|v| v.into_iter().map(Into::into).collect())")
                } else {
                    format!("{name}: val.{name}.into_iter().map(Into::into).collect()")
                }
            }
            _ => format!("{name}: val.{name}"),
        },
        // Map -- always collect to handle HashMap↔BTreeMap conversion;
        // additionally convert Named keys/values via Into.
        TypeRef::Map(k, v) => {
            let has_named_key = matches!(k.as_ref(), TypeRef::Named(_));
            let has_named_val = matches!(v.as_ref(), TypeRef::Named(_));
            let k_expr = if has_named_key { "k.into()" } else { "k" };
            let v_expr = if has_named_val { "v.into()" } else { "v" };
            if optional {
                format!("{name}: val.{name}.map(|m| m.into_iter().map(|(k, v)| ({k_expr}, {v_expr})).collect())")
            } else {
                format!("{name}: val.{name}.into_iter().map(|(k, v)| ({k_expr}, {v_expr})).collect()")
            }
        }
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
        // Check if binding type is Optional<Vec<String>> (sanitized from Optional<Vec<Unknown>>)
        if let TypeRef::Optional(opt_inner) = ty {
            if let TypeRef::Vec(vec_inner) = opt_inner.as_ref() {
                if matches!(vec_inner.as_ref(), TypeRef::String) {
                    return format!(
                        "{name}: val.{name}.as_ref().map(|v| v.iter().map(|i| format!(\"{{:?}}\", i)).collect())"
                    );
                }
            }
        }
        if optional {
            return format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))");
        }
        return format!("{name}: format!(\"{{:?}}\", val.{name})");
    }
    match ty {
        // Duration: core uses std::time::Duration, binding uses u64 (secs)
        TypeRef::Duration => {
            if optional {
                return format!("{name}: val.{name}.map(|d| d.as_secs())");
            }
            format!("{name}: val.{name}.as_secs()")
        }
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
                format!("{name}: val.{name}.map(|v| {n} {{ inner: Arc::new(v) }})")
            } else {
                format!("{name}: {n} {{ inner: Arc::new(val.{name}) }}")
            }
        }
        // Everything else is symmetric
        _ => field_conversion_to_core(name, ty, optional),
    }
}

/// Binding→core field conversion with backend-specific config (i64 casts, etc.).
pub fn field_conversion_to_core_cfg(name: &str, ty: &TypeRef, optional: bool, config: &ConversionConfig) -> String {
    // WASM JsValue maps: use serde_wasm_bindgen for deserialization
    if config.map_uses_jsvalue {
        if let TypeRef::Map(_, _) = ty {
            if optional {
                return format!(
                    "{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::from_value(v.clone()).ok())"
                );
            }
            return format!("{name}: serde_wasm_bindgen::from_value(val.{name}.clone()).unwrap_or_default()");
        }
        // Optional<Map> handled here too
        if let TypeRef::Optional(inner) = ty {
            if matches!(inner.as_ref(), TypeRef::Map(_, _)) {
                return format!(
                    "{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::from_value(v.clone()).ok())"
                );
            }
        }
    }

    if !config.cast_large_ints_to_i64 {
        return field_conversion_to_core(name, ty, optional);
    }
    // i64-cast mode: handle primitives and Duration differently
    match ty {
        TypeRef::Primitive(p) if needs_i64_cast(p) => {
            let core_ty = core_prim_str(p);
            if optional {
                format!("{name}: val.{name}.map(|v| v as {core_ty})")
            } else {
                format!("{name}: val.{name} as {core_ty}")
            }
        }
        TypeRef::Duration => {
            if optional {
                format!("{name}: val.{name}.map(|v| std::time::Duration::from_secs(v as u64))")
            } else {
                format!("{name}: std::time::Duration::from_secs(val.{name} as u64)")
            }
        }
        TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) => {
            if let TypeRef::Primitive(p) = inner.as_ref() {
                let core_ty = core_prim_str(p);
                format!("{name}: val.{name}.map(|v| v as {core_ty})")
            } else {
                field_conversion_to_core(name, ty, optional)
            }
        }
        // Fall through to default for everything else
        _ => field_conversion_to_core(name, ty, optional),
    }
}

/// Core→binding field conversion with backend-specific config.
pub fn field_conversion_from_core_cfg(
    name: &str,
    ty: &TypeRef,
    optional: bool,
    sanitized: bool,
    opaque_types: &AHashSet<String>,
    config: &ConversionConfig,
) -> String {
    // Sanitized fields handled the same regardless of config
    if sanitized {
        return field_conversion_from_core(name, ty, optional, sanitized, opaque_types);
    }

    // WASM JsValue maps: use serde_wasm_bindgen for serialization
    if config.map_uses_jsvalue {
        if let TypeRef::Map(_, _) = ty {
            if optional {
                return format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::to_value(v).ok())");
            }
            return format!("{name}: serde_wasm_bindgen::to_value(&val.{name}).unwrap_or(JsValue::NULL)");
        }
        if let TypeRef::Optional(inner) = ty {
            if matches!(inner.as_ref(), TypeRef::Map(_, _)) {
                return format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::to_value(v).ok())");
            }
        }
    }

    let prefix = config.type_name_prefix;
    let is_enum_string = |n: &str| -> bool { config.enum_string_names.as_ref().is_some_and(|names| names.contains(n)) };

    match ty {
        // i64 casting for large int primitives
        TypeRef::Primitive(p) if config.cast_large_ints_to_i64 && needs_i64_cast(p) => {
            if optional {
                format!("{name}: val.{name}.map(|v| v as i64)")
            } else {
                format!("{name}: val.{name} as i64")
            }
        }
        // Duration with i64 casting
        TypeRef::Duration if config.cast_large_ints_to_i64 => {
            if optional {
                format!("{name}: val.{name}.map(|d| d.as_secs() as i64)")
            } else {
                format!("{name}: val.{name}.as_secs() as i64")
            }
        }
        // Opaque Named types with prefix: wrap in Arc with prefixed binding name
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) && !prefix.is_empty() => {
            let prefixed = format!("{prefix}{n}");
            if optional {
                format!("{name}: val.{name}.map(|v| {prefixed} {{ inner: Arc::new(v) }})")
            } else {
                format!("{name}: {prefixed} {{ inner: Arc::new(val.{name}) }}")
            }
        }
        // Enum-to-String Named types (PHP pattern)
        TypeRef::Named(n) if is_enum_string(n) => {
            if optional {
                format!("{name}: val.{name}.as_ref().map(|v| format!(\"{{:?}}\", v))")
            } else {
                format!("{name}: format!(\"{{:?}}\", val.{name})")
            }
        }
        // Optional with i64-cast inner
        TypeRef::Optional(inner)
            if config.cast_large_ints_to_i64
                && matches!(inner.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) =>
        {
            if let TypeRef::Primitive(_p) = inner.as_ref() {
                format!("{name}: val.{name}.map(|v| v as i64)")
            } else {
                field_conversion_from_core(name, ty, optional, sanitized, opaque_types)
            }
        }
        // Fall through to default (handles paths, opaque without prefix, etc.)
        _ => field_conversion_from_core(name, ty, optional, sanitized, opaque_types),
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
            has_default: false,
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
