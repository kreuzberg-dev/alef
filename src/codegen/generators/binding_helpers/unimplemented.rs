use crate::codegen::generators::RustBindingConfig;
use crate::core::ir::{ParamDef, TypeRef};
use ahash::AHashSet;

/// Generate a compile-time diagnostic for functions that can't be auto-delegated.
///
/// Unsupported production bindings must be excluded or backed by an adapter instead
/// of returning placeholder values at runtime.
pub fn gen_unimplemented_body(
    _return_type: &TypeRef,
    fn_name: &str,
    _has_error: bool,
    cfg: &RustBindingConfig,
    params: &[ParamDef],
    _opaque_types: &AHashSet<String>,
) -> String {
    let suppress = if params.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        if names.len() == 1 {
            format!("let _ = {};\n        ", names[0])
        } else {
            format!("let _ = ({});\n        ", names.join(", "))
        }
    };
    let config_hint = if cfg.type_name_prefix.is_empty() {
        "configure an adapter body or exclude this item from the backend"
    } else {
        "configure an adapter body or add this item to the backend exclude list"
    };
    let body = format!("compile_error!(\"alef cannot auto-delegate `{fn_name}`; {config_hint}\")");
    format!("{suppress}{body}")
}
