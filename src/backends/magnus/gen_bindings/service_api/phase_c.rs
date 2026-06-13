//! Phase-C service API emission hooks for the Magnus backend.

/// Emit Magnus/Ruby lifecycle-hook registration methods.
///
/// Stub: walks the collection, logs once when non-empty, returns `""`.
/// Replace this body with Jinja-driven generation in the Magnus Phase-C pass.
#[allow(dead_code)]
pub(super) fn emit_lifecycle_hooks(hooks: &[crate::core::ir::LifecycleHookDef]) -> String {
    if hooks.is_empty() {
        return String::new();
    }
    tracing::debug!(
        "lifecycle hook emission not implemented for magnus ({} hooks)",
        hooks.len()
    );
    for _hook in hooks {}
    String::new()
}

/// Emit Magnus/Ruby WebSocket route registration methods.
///
/// Stub: returns `""` until the Magnus Phase-C specialist implements
/// `app.websocket(path) { |socket| ... }` generation.
#[allow(dead_code)]
pub(super) fn emit_websocket_routes(routes: &[crate::core::ir::WebSocketRouteDef]) -> String {
    if routes.is_empty() {
        return String::new();
    }
    tracing::debug!(
        "WebSocket route emission not implemented for magnus ({} routes)",
        routes.len()
    );
    for _route in routes {}
    String::new()
}

/// Emit Magnus/Ruby SSE route registration methods.
///
/// Stub: returns `""` until the Magnus Phase-C specialist implements
/// `app.sse(path) { ... }` generation.
#[allow(dead_code)]
pub(super) fn emit_sse_routes(routes: &[crate::core::ir::SseRouteDef]) -> String {
    if routes.is_empty() {
        return String::new();
    }
    tracing::debug!(
        "SSE route emission not implemented for magnus ({} routes)",
        routes.len()
    );
    for _route in routes {}
    String::new()
}

/// Emit Magnus/Ruby native error classes.
///
/// Stub: returns `""` until the Magnus Phase-C specialist implements
/// Ruby `StandardError` subclass generation.
#[allow(dead_code)]
pub(super) fn emit_error_types(types: &[crate::core::ir::ErrorTypeDef]) -> String {
    if types.is_empty() {
        return String::new();
    }
    tracing::debug!("error type emission not implemented for magnus ({} types)", types.len());
    for _ty in types {}
    String::new()
}

/// Aggregate stub: forwards all four new IR sections for the Magnus backend.
#[allow(dead_code)]
pub(super) fn emit_new_ir_sections(api: &crate::core::ir::ApiSurface) -> String {
    let mut out = String::new();
    out.push_str(&emit_lifecycle_hooks(&api.lifecycle_hooks));
    out.push_str(&emit_websocket_routes(&api.websocket_routes));
    out.push_str(&emit_sse_routes(&api.sse_routes));
    out.push_str(&emit_error_types(&api.error_types));
    out
}
