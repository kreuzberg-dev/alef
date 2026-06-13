//! Stub emission hooks for the new IR sections (Phase C seams) — Rustler/Elixir backend.

use crate::core::ir::{ApiSurface, ErrorTypeDef, LifecycleHookDef, SseRouteDef, WebSocketRouteDef};

/// Emit Rustler lifecycle-hook registration methods.
///
/// Generates `App.on_request(app, fn)`, `App.pre_validation(app, fn)`, etc.
/// for each hook type in the IR.
pub(super) fn emit_lifecycle_hooks(hooks: &[LifecycleHookDef]) -> String {
    if hooks.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    for hook in hooks {
        let method_name = hook.name.clone();
        let doc = if hook.doc.is_empty() {
            format!(
                "Register a {} lifecycle hook.\n\nThe hook receives a request context and must return the context (possibly modified).",
                hook.name.to_lowercase()
            )
        } else {
            hook.doc.clone()
        };
        let doc = doc.replace("\"", "\\\"");
        out.push_str(&format!(
            "  @doc \"{doc}\"\n  def {method_name}(app, handler_fn) when is_function(handler_fn, 1) do\n    %__MODULE__{{app | {method_name}: handler_fn}}\n  end\n\n"
        ));
    }
    out
}

/// Emit Rustler WebSocket route registration methods. Stub.
#[allow(dead_code)]
pub(super) fn emit_websocket_routes(routes: &[WebSocketRouteDef]) -> String {
    if routes.is_empty() {
        return String::new();
    }
    tracing::debug!(
        "WebSocket route emission not implemented for rustler ({} routes)",
        routes.len()
    );
    for _route in routes {}
    String::new()
}

/// Emit Rustler SSE route registration methods. Stub.
#[allow(dead_code)]
pub(super) fn emit_sse_routes(routes: &[SseRouteDef]) -> String {
    if routes.is_empty() {
        return String::new();
    }
    tracing::debug!(
        "SSE route emission not implemented for rustler ({} routes)",
        routes.len()
    );
    for _route in routes {}
    String::new()
}

/// Emit Rustler native error types.
///
/// Generates exception defexception definitions under the configured error module
/// for each ErrorTypeDef in the IR (NotFoundError, ValidationError, etc.).
pub(super) fn emit_error_types(types: &[ErrorTypeDef], module_prefix: &str) -> String {
    if types.is_empty() {
        return String::new();
    }

    let error_module = prefixed_module(module_prefix, "Errors");
    let mut out = String::new();
    out.push_str(&format!("defmodule {error_module} do\n"));
    out.push_str("  @moduledoc \"\"\"\n");
    out.push_str("  Generated exception types.\n");
    out.push_str("  \"\"\"\n\n");

    for error_type in types {
        let exception_name = &error_type.name;
        let status_code = error_type.http_status.as_u16();
        let doc = if error_type.doc.is_empty() {
            format!("Exception for {}.", error_type.name.to_lowercase())
        } else {
            error_type.doc.clone()
        };

        out.push_str(&format!("  @doc \"{}\"\n", doc.replace("\"", "\\\"")));
        out.push_str(&format!("  defmodule {} do\n", exception_name));
        out.push_str("    defexception [:message, :status_code, :problem_details]\n\n");
        out.push_str(&format!(
            "    def new(message, status_code \\\\ {}, problem_details \\\\ nil) do\n",
            status_code
        ));
        out.push_str("      %__MODULE__{\n");
        out.push_str("        message: message,\n");
        out.push_str("        status_code: status_code,\n");
        out.push_str("        problem_details: problem_details\n");
        out.push_str("      }\n");
        out.push_str("    end\n");
        out.push_str("  end\n\n");
    }

    out.push_str("end\n\n");
    out
}

/// Aggregate stub — forwards all four new IR sections for the Rustler/Elixir backend.
#[allow(dead_code)]
pub(super) fn emit_new_ir_sections(api: &ApiSurface) -> String {
    let mut out = String::new();
    out.push_str(&emit_lifecycle_hooks(&api.lifecycle_hooks));
    out.push_str(&emit_websocket_routes(&api.websocket_routes));
    out.push_str(&emit_sse_routes(&api.sse_routes));
    out.push_str(&emit_error_types(&api.error_types, ""));
    out
}

fn prefixed_module(module_prefix: &str, module: &str) -> String {
    if module_prefix.is_empty() {
        module.to_owned()
    } else {
        format!("{module_prefix}.{module}")
    }
}
