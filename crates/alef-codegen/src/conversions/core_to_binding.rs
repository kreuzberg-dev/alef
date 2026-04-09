use ahash::AHashSet;
use alef_core::ir::{PrimitiveType, TypeDef, TypeRef};
use std::fmt::Write;

use super::ConversionConfig;
use super::binding_to_core::field_conversion_to_core;
use super::helpers::{binding_prim_str, core_type_path, needs_i64_cast};

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
        // Box<T> fields: dereference before conversion.
        let base_conversion = if field.is_boxed && matches!(&field.ty, TypeRef::Named(_)) {
            if field.optional {
                // Optional<Box<T>>: replace .map(Into::into) with .map(|v| (*v).into())
                let src = format!("{}: val.{}.map(Into::into)", field.name, field.name);
                let dst = format!("{}: val.{}.map(|v| (*v).into())", field.name, field.name);
                if base_conversion == src { dst } else { base_conversion }
            } else {
                // Box<T>: replace `val.{name}` with `(*val.{name})`
                base_conversion.replace(&format!("val.{}", field.name), &format!("(*val.{})", field.name))
            }
        } else {
            base_conversion
        };
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
        // Skip cfg-gated fields — they don't exist in the binding struct
        if field.cfg.is_some() {
            continue;
        }
        writeln!(out, "            {conversion},").ok();
    }

    // Synthetic field conversion for cfg-gated metadata field (NAPI only).
    if config.include_cfg_metadata && typ.has_stripped_cfg_fields && typ.name == "ConversionResult" {
        writeln!(out, "            metadata: Some(val.metadata.into()),").ok();
    }

    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
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
                if optional {
                    return format!(
                        "{name}: val.{name}.as_ref().map(|v| v.iter().map(|i| format!(\"{{:?}}\", i)).collect())"
                    );
                }
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
        // Char: core uses char, binding uses String — convert char to string
        TypeRef::Char => {
            if optional {
                format!("{name}: val.{name}.map(|c| c.to_string())")
            } else {
                format!("{name}: val.{name}.to_string()")
            }
        }
        // Bytes: core uses bytes::Bytes, binding uses Vec<u8>
        TypeRef::Bytes => {
            if optional {
                format!("{name}: val.{name}.map(|v| v.to_vec())")
            } else {
                format!("{name}: val.{name}.to_vec()")
            }
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

    // WASM JsValue: use serde_wasm_bindgen for Map and nested Vec types
    if config.map_uses_jsvalue {
        let is_nested_vec = matches!(ty, TypeRef::Vec(inner) if matches!(inner.as_ref(), TypeRef::Vec(_)));
        let is_map = matches!(ty, TypeRef::Map(_, _));
        if is_nested_vec || is_map {
            if optional {
                return format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::to_value(v).ok())");
            }
            return format!("{name}: serde_wasm_bindgen::to_value(&val.{name}).unwrap_or(JsValue::NULL)");
        }
        if let TypeRef::Optional(inner) = ty {
            let is_inner_nested = matches!(inner.as_ref(), TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Vec(_)));
            let is_inner_map = matches!(inner.as_ref(), TypeRef::Map(_, _));
            if is_inner_nested || is_inner_map {
                return format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::to_value(v).ok())");
            }
        }
    }

    let prefix = config.type_name_prefix;
    let is_enum_string = |n: &str| -> bool { config.enum_string_names.as_ref().is_some_and(|names| names.contains(n)) };

    match ty {
        // i64 casting for large int primitives
        TypeRef::Primitive(p) if config.cast_large_ints_to_i64 && needs_i64_cast(p) => {
            let cast_to = binding_prim_str(p);
            if optional {
                format!("{name}: val.{name}.map(|v| v as {cast_to})")
            } else {
                format!("{name}: val.{name} as {cast_to}")
            }
        }
        // f32→f64 casting (NAPI only)
        TypeRef::Primitive(PrimitiveType::F32) if config.cast_f32_to_f64 => {
            if optional {
                format!("{name}: val.{name}.map(|v| v as f64)")
            } else {
                format!("{name}: val.{name} as f64")
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
        // Vec<f32> needs element-wise cast to f64 when f32→f64 mapping is active
        TypeRef::Vec(inner)
            if config.cast_f32_to_f64 && matches!(inner.as_ref(), TypeRef::Primitive(PrimitiveType::F32)) =>
        {
            if optional {
                format!("{name}: val.{name}.as_ref().map(|v| v.iter().map(|&x| x as f64).collect())")
            } else {
                format!("{name}: val.{name}.iter().map(|&v| v as f64).collect()")
            }
        }
        // Optional(Vec(f32)) needs element-wise cast to f64
        TypeRef::Optional(inner)
            if config.cast_f32_to_f64
                && matches!(inner.as_ref(), TypeRef::Vec(vi) if matches!(vi.as_ref(), TypeRef::Primitive(PrimitiveType::F32))) =>
        {
            format!("{name}: val.{name}.as_ref().map(|v| v.iter().map(|&x| x as f64).collect())")
        }
        // Optional with i64-cast inner
        TypeRef::Optional(inner)
            if config.cast_large_ints_to_i64
                && matches!(inner.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) =>
        {
            if let TypeRef::Primitive(p) = inner.as_ref() {
                let cast_to = binding_prim_str(p);
                format!("{name}: val.{name}.map(|v| v as {cast_to})")
            } else {
                field_conversion_from_core(name, ty, optional, sanitized, opaque_types)
            }
        }
        // Vec<u64/usize/isize> needs element-wise i64 casting (core→binding)
        TypeRef::Vec(inner)
            if config.cast_large_ints_to_i64
                && matches!(inner.as_ref(), TypeRef::Primitive(p) if needs_i64_cast(p)) =>
        {
            if let TypeRef::Primitive(p) = inner.as_ref() {
                let cast_to = binding_prim_str(p);
                if optional {
                    format!("{name}: val.{name}.as_ref().map(|v| v.iter().map(|&x| x as {cast_to}).collect())")
                } else {
                    format!("{name}: val.{name}.iter().map(|&v| v as {cast_to}).collect()")
                }
            } else {
                field_conversion_from_core(name, ty, optional, sanitized, opaque_types)
            }
        }
        // Json→String: core uses serde_json::Value, binding uses String (PHP)
        TypeRef::Json if config.json_to_string => {
            if optional {
                format!("{name}: val.{name}.as_ref().map(|v| v.to_string())")
            } else {
                format!("{name}: val.{name}.to_string()")
            }
        }
        // Json→JsValue: core uses serde_json::Value, binding uses JsValue (WASM)
        TypeRef::Json if config.map_uses_jsvalue => {
            if optional {
                format!("{name}: val.{name}.as_ref().and_then(|v| serde_wasm_bindgen::to_value(v).ok())")
            } else {
                format!("{name}: serde_wasm_bindgen::to_value(&val.{name}).unwrap_or(JsValue::NULL)")
            }
        }
        // Fall through to default (handles paths, opaque without prefix, etc.)
        _ => field_conversion_from_core(name, ty, optional, sanitized, opaque_types),
    }
}
