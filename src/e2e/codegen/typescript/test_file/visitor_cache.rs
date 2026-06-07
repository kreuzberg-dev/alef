use crate::e2e::config::E2eConfig;
use crate::e2e::fixture::Fixture;

use super::super::json::snake_to_camel;
use super::helpers::resolve_node_function_name;
use super::wasm::wasm_class_name;

pub(super) struct WasmVisitorBinding {
    pub(super) options_type: String,
    pub(super) options_field: String,
    pub(super) handle_type: String,
}

pub(super) fn wasm_visitor_binding(
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

pub(super) fn apply_wasm_visitor_arg(args_str: &str, visitor_arg: &str, binding: &WasmVisitorBinding) -> String {
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

/// Detect if cache isolation is needed: checks if any fixture calls `cleanCache`
/// and if a `configure` function is available.
/// Returns (has_clean_cache, has_configure).
pub(super) fn detect_cache_isolation_needs(fixtures: &[&Fixture], e2e_config: &E2eConfig) -> (bool, bool) {
    let has_clean_cache = fixtures.iter().any(|fixture| {
        let call_config = e2e_config.resolve_call_for_fixture(
            fixture.call.as_deref(),
            &fixture.id,
            &fixture.resolved_category(),
            &fixture.tags,
            &fixture.input,
        );
        resolve_node_function_name(call_config) == "cleanCache"
    });

    let has_configure = e2e_config
        .calls
        .iter()
        .any(|(_, call_config)| resolve_node_function_name(call_config) == "configure")
        || resolve_node_function_name(&e2e_config.call) == "configure";

    (has_clean_cache, has_configure)
}

/// Emit the cache isolation setup code (beforeAll/afterAll blocks).
pub(super) fn emit_cache_isolation_setup(out: &mut String) {
    let rendered = crate::e2e::template_env::render("typescript/cache_isolation_setup.jinja", minijinja::context! {});
    out.push_str(&rendered);
}
