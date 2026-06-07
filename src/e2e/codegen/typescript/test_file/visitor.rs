use super::*;

#[derive(Debug, Clone)]
pub(in crate::e2e::codegen::typescript::test_file) struct WasmVisitorBinding {
    pub(in crate::e2e::codegen::typescript::test_file) options_type: String,
    pub(in crate::e2e::codegen::typescript::test_file) options_field: String,
    pub(in crate::e2e::codegen::typescript::test_file) handle_type: String,
}

pub(in crate::e2e::codegen::typescript::test_file) fn wasm_visitor_binding(
    config: &crate::core::config::ResolvedCrateConfig,
    fallback_options_type: Option<&str>,
) -> Option<WasmVisitorBinding> {
    let bridge = config
        .trait_bridges
        .iter()
        .find(|bridge| bridge.options_type.is_some() && bridge.resolved_options_field().is_some())?;
    let wasm_prefix = config.wasm_type_prefix();
    let options_type = fallback_options_type
        .or(bridge.options_type.as_deref())
        .map(|name| wasm_class_name(name.strip_prefix(&wasm_prefix).unwrap_or(name), &wasm_prefix))?;
    let handle_type = bridge
        .type_alias
        .as_deref()
        .map(|name| wasm_class_name(name.strip_prefix(&wasm_prefix).unwrap_or(name), &wasm_prefix))
        .unwrap_or_else(|| format!("Wasm{}Bridge", bridge.trait_name));

    Some(WasmVisitorBinding {
        options_type,
        options_field: bridge.resolved_options_field()?.to_string(),
        handle_type,
    })
}

pub(in crate::e2e::codegen::typescript::test_file) fn apply_wasm_visitor_arg(
    args_str: &str,
    visitor_arg: &str,
    binding: &WasmVisitorBinding,
) -> String {
    let visitor_assignment = format!(
        "_u.{} = new {}({visitor_arg});",
        snake_to_camel(&binding.options_field),
        binding.handle_type
    );
    let iife = format!(
        "(() => {{ const _u = {}.default(); {visitor_assignment} return _u; }})()",
        binding.options_type
    );
    if args_str.is_empty() {
        iife
    } else if let Some(return_pos) = args_str.rfind("return _u;") {
        let (iife_body, ret_part) = args_str.split_at(return_pos);
        format!("{iife_body}{visitor_assignment} {ret_part}")
    } else if let Some(stripped) = args_str.strip_suffix(", undefined") {
        format!("{stripped}, {iife}")
    } else {
        format!("{args_str}, {iife}")
    }
}
