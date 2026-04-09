use alef_core::ir::{PrimitiveType, TypeDef, TypeRef};
use std::fmt::Write;

use super::ConversionConfig;
use super::helpers::{core_prim_str, core_type_path, needs_i64_cast};

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
        // Box<T> fields: wrap the converted value in Box::new()
        let conversion = if field.is_boxed && matches!(&field.ty, TypeRef::Named(_)) {
            if let Some(expr) = conversion.strip_prefix(&format!("{}: ", field.name)) {
                format!("{}: Box::new({})", field.name, expr)
            } else {
                conversion
            }
        } else {
            conversion
        };
        writeln!(out, "            {conversion},").ok();
    }
    // Use ..Default::default() to fill cfg-gated fields stripped from the IR
    if typ.has_stripped_cfg_fields {
        writeln!(out, "            ..Default::default()").ok();
    }
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

/// Generate field conversion for a non-optional field that was optionalized
/// (wrapped in Option<T>) in the binding struct for JS ergonomics.
pub(super) fn gen_optionalized_field_to_core(name: &str, ty: &TypeRef, config: &ConversionConfig) -> String {
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
        // Char: binding uses Option<String>, core uses char
        TypeRef::Char => {
            format!("{name}: val.{name}.and_then(|s| s.chars().next()).unwrap_or('*')")
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Named(_) => {
                format!("{name}: val.{name}.map(|v| v.into_iter().map(Into::into).collect()).unwrap_or_default()")
            }
            TypeRef::Primitive(p) if config.cast_large_ints_to_i64 && needs_i64_cast(p) => {
                let core_ty = core_prim_str(p);
                format!(
                    "{name}: val.{name}.map(|v| v.into_iter().map(|x| x as {core_ty}).collect()).unwrap_or_default()"
                )
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

/// Determine the field conversion expression for binding -> core.
pub fn field_conversion_to_core(name: &str, ty: &TypeRef, optional: bool) -> String {
    match ty {
        // Primitives, String, Bytes, Unit, Json -- direct assignment
        TypeRef::Primitive(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Unit | TypeRef::Json => {
            format!("{name}: val.{name}")
        }
        // Char: binding uses String, core uses char — convert first character
        TypeRef::Char => {
            if optional {
                format!("{name}: val.{name}.and_then(|s| s.chars().next())")
            } else {
                format!("{name}: val.{name}.chars().next().unwrap_or('*')")
            }
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

/// Binding→core field conversion with backend-specific config (i64 casts, etc.).
pub fn field_conversion_to_core_cfg(name: &str, ty: &TypeRef, optional: bool, config: &ConversionConfig) -> String {
    // WASM JsValue: use serde_wasm_bindgen for Map and nested Vec types
    if config.map_uses_jsvalue {
        let is_nested_vec = matches!(ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Vec(_)));
        let is_map = matches!(ty, TypeRef::Map(_, _));
        if is_nested_vec || is_map {
            if optional {
                return format!(
                    "{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::from_value(v.clone()).ok())"
                );
            }
            return format!("{name}: serde_wasm_bindgen::from_value(val.{name}.clone()).unwrap_or_default()");
        }
        if let TypeRef::Optional(inner) = ty {
            let is_inner_nested = matches!(inner.as_ref(), TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Vec(_)));
            let is_inner_map = matches!(inner.as_ref(), TypeRef::Map(_, _));
            if is_inner_nested || is_inner_map {
                return format!(
                    "{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::from_value(v.clone()).ok())"
                );
            }
        }
    }

    // Json→String binding→core: use Default::default() (lossy — can't parse String back)
    if config.json_to_string && matches!(ty, TypeRef::Json) {
        return format!("{name}: Default::default()");
    }
    // Json→JsValue binding→core: use serde_wasm_bindgen to convert (WASM)
    if config.map_uses_jsvalue && matches!(ty, TypeRef::Json) {
        if optional {
            return format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::from_value(v.clone()).ok())");
        }
        return format!("{name}: serde_wasm_bindgen::from_value(val.{name}.clone()).unwrap_or_default()");
    }
    if !config.cast_large_ints_to_i64 && !config.cast_f32_to_f64 && !config.json_to_string {
        return field_conversion_to_core(name, ty, optional);
    }
    // Cast mode: handle primitives and Duration differently
    match ty {
        TypeRef::Primitive(p) if config.cast_large_ints_to_i64 && needs_i64_cast(p) => {
            let core_ty = core_prim_str(p);
            if optional {
                format!("{name}: val.{name}.map(|v| v as {core_ty})")
            } else {
                format!("{name}: val.{name} as {core_ty}")
            }
        }
        // f64→f32 cast (NAPI binding f64 → core f32)
        TypeRef::Primitive(PrimitiveType::F32) if config.cast_f32_to_f64 => {
            if optional {
                format!("{name}: val.{name}.map(|v| v as f32)")
            } else {
                format!("{name}: val.{name} as f32")
            }
        }
        TypeRef::Duration if config.cast_large_ints_to_i64 => {
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
        // Vec<u64/usize/isize> needs element-wise i64→core casting
        TypeRef::Vec(inner)
            if config.cast_large_ints_to_i64
                && matches!(inner.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) =>
        {
            if let TypeRef::Primitive(p) = inner.as_ref() {
                let core_ty = core_prim_str(p);
                if optional {
                    format!("{name}: val.{name}.map(|v| v.into_iter().map(|x| x as {core_ty}).collect())")
                } else {
                    format!("{name}: val.{name}.into_iter().map(|v| v as {core_ty}).collect()")
                }
            } else {
                field_conversion_to_core(name, ty, optional)
            }
        }
        // Vec<f32> needs element-wise cast when f32→f64 mapping is active (NAPI)
        TypeRef::Vec(inner)
            if config.cast_f32_to_f64 && matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::F32)) =>
        {
            if optional {
                format!("{name}: val.{name}.map(|v| v.into_iter().map(|x| x as f32).collect())")
            } else {
                format!("{name}: val.{name}.into_iter().map(|v| v as f32).collect()")
            }
        }
        // Optional(Vec(f32)) needs element-wise cast (NAPI only)
        TypeRef::Optional(inner)
            if config.cast_f32_to_f64
                && matches!(inner.as_ref(), TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Primitive(PrimitiveType::F32))) =>
        {
            format!("{name}: val.{name}.map(|v| v.into_iter().map(|x| x as f32).collect())")
        }
        // Fall through to default for everything else
        _ => field_conversion_to_core(name, ty, optional),
    }
}
