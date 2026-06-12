use crate::core::config::TraitBridgeConfig;
use crate::core::ir::{ParamDef, TypeRef};

/// Build a serde_json::Value expression for a visitor method parameter (for the args JSON object).
pub(super) fn build_json_arg(p: &ParamDef, bridge_cfg: &TraitBridgeConfig) -> String {
    // context_type param: serialize as a JSON object via serde_json.
    if let TypeRef::Named(n) = &p.ty {
        if Some(n.as_str()) == bridge_cfg.context_type.as_deref() {
            let ref_expr = if p.is_ref {
                p.name.clone()
            } else {
                format!("&{}", p.name)
            };
            return format!("serde_json::to_value({ref_expr}).unwrap_or(serde_json::Value::Null)");
        }
    }
    // Optional string params (must check before non-optional: Option<&str> also has is_ref=true)
    if p.optional && matches!(&p.ty, TypeRef::String) {
        return format!(
            "match {0} {{ Some(s) => serde_json::Value::String(s.to_string()), None => serde_json::Value::Null }}",
            p.name
        );
    }
    // String params
    if matches!(&p.ty, TypeRef::String) && p.is_ref {
        return format!("serde_json::Value::String({}.to_string())", p.name);
    }
    if matches!(&p.ty, TypeRef::String) {
        return format!("serde_json::Value::String({}.clone())", p.name);
    }
    // Bool params
    if matches!(&p.ty, TypeRef::Primitive(crate::core::ir::PrimitiveType::Bool)) {
        return format!("serde_json::Value::Bool({})", p.name);
    }
    // Slice params (e.g. &[String])
    if matches!(&p.ty, TypeRef::Vec(_)) && p.is_ref {
        return format!("serde_json::to_value({}).unwrap_or(serde_json::Value::Null)", p.name);
    }
    // usize / u32 numeric params
    if matches!(
        &p.ty,
        TypeRef::Primitive(
            crate::core::ir::PrimitiveType::Usize
                | crate::core::ir::PrimitiveType::U8
                | crate::core::ir::PrimitiveType::U16
                | crate::core::ir::PrimitiveType::U32
                | crate::core::ir::PrimitiveType::U64
        )
    ) {
        return format!("serde_json::Value::Number(serde_json::Number::from({} as u64))", p.name);
    }
    // i64 / isize numeric params
    if matches!(
        &p.ty,
        TypeRef::Primitive(
            crate::core::ir::PrimitiveType::I8
                | crate::core::ir::PrimitiveType::I16
                | crate::core::ir::PrimitiveType::I32
                | crate::core::ir::PrimitiveType::I64
                | crate::core::ir::PrimitiveType::Isize
        )
    ) {
        return format!("serde_json::Value::Number(serde_json::Number::from({} as i64))", p.name);
    }
    // Fallback: debug-print as string
    format!("serde_json::Value::String(format!(\"{{:?}}\", {}))", p.name)
}
