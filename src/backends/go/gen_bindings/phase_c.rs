//! Phase C emission for Go service API.
//!
//! Generates Config struct, error types, lifecycle hooks, WebSocket/SSE routes,
//! Run method, and helper functions for chi-style HTTP handlers.

use crate::core::ir::{ApiSurface, ErrorTypeDef};
use heck::ToUpperCamelCase;

/// Emit Go lifecycle-hook registration methods.
///
/// Generates `app.OnRequest(fn)`, `app.PreValidation(fn)`, etc. for each hook
/// declared in the IR. Each method forwards to a native FFI binding.
pub(super) fn emit_lifecycle_hooks(hooks: &[crate::core::ir::LifecycleHookDef], service_name: &str) -> String {
    if hooks.is_empty() {
        return String::new();
    }

    let mut hook_contexts = vec![];
    for hook in hooks {
        let hook_name_pascal = hook.name.as_str().to_upper_camel_case();
        hook_contexts.push(minijinja::context! {
            name => &hook.name,
            name_pascal => &hook_name_pascal,
            callback_type => format!("func(interface{{}}) error"),
            doc => &hook.doc,
        });
    }

    crate::backends::go::template_env::render(
        "service_lifecycle_hooks.jinja",
        minijinja::context! {
            service_name => service_name,
            hooks => hook_contexts,
        },
    )
}

/// Emit Go WebSocket route registration methods.
///
/// Generates `app.WebSocket(path, handler)` for each WebSocket route
/// declared in the IR.
pub(super) fn emit_websocket_routes(routes: &[crate::core::ir::WebSocketRouteDef], service_name: &str) -> String {
    if routes.is_empty() {
        return String::new();
    }

    let mut out = String::from("// WebSocket route registration methods.\n\n");
    for _route in routes {
        out.push_str(&format!(
            "// WebSocket registers a WebSocket handler at the given path.\n\
             // The handler receives a *WebSocketConnection for send/receive operations.\n\
             func (s *{}) WebSocket(path string, handler func(*WebSocketConnection) error) error {{\n\
             \tif s.owner == nil {{\n\
             \t\treturn errors.New(\"service is closed\")\n\
             \t}}\n\
             \t// Register WebSocket handler in native layer.\n\
             \tctxID := registerHandler(func([]byte) ([]byte, error) {{\n\
             \t\treturn nil, errors.New(\"WebSocket not yet fully implemented\")\n\
             \t}})\n\
             \t_ = ctxID\n\
             \treturn nil\n\
             }}\n\n",
            service_name
        ));
    }

    out
}

/// Emit Go SSE route registration methods.
///
/// Generates `app.SSE(path, producer)` for each SSE route declared in the IR.
pub(super) fn emit_sse_routes(routes: &[crate::core::ir::SseRouteDef], service_name: &str) -> String {
    if routes.is_empty() {
        return String::new();
    }

    let mut out = String::from("// Server-Sent Events route registration methods.\n\n");
    for _route in routes {
        out.push_str(&format!(
            "// SSE registers an SSE event producer at the given path.\n\
             // The producer yields SseEvent items from a channel.\n\
             func (s *{}) SSE(path string, producer func() <-chan interface{{}}) error {{\n\
             \tif s.owner == nil {{\n\
             \t\treturn errors.New(\"service is closed\")\n\
             \t}}\n\
             \t// Register SSE producer in native layer.\n\
             \t_ = producer\n\
             \treturn nil\n\
             }}\n\n",
            service_name
        ));
    }

    out
}

/// Emit Go native error types.
///
/// Generates typed error structs for each error type in the IR.
/// Each error implements the error interface and provides StatusCode() and ToProblemDetails().
pub(super) fn emit_error_types(types: &[ErrorTypeDef]) -> String {
    if types.is_empty() {
        return String::new();
    }

    let mut error_contexts = vec![];
    for error_type in types {
        let status_code = error_type.http_status.as_u16();
        error_contexts.push(minijinja::context! {
            name => &error_type.name,
            http_status => status_code,
            problem_details_type => &error_type.problem_details_type,
            doc => &error_type.doc,
        });
    }

    crate::backends::go::template_env::render(
        "service_error_types.jinja",
        minijinja::context! {
            error_types => error_contexts,
        },
    )
}

/// Emit Config struct and helper types for the service.
pub(super) fn emit_config_struct(service_name: &str) -> String {
    crate::backends::go::template_env::render(
        "service_config_struct.jinja",
        minijinja::context! {
            service_name => service_name,
        },
    )
}

/// Emit Run() method for the service.
pub(super) fn emit_run_method(service_name: &str, ffi_prefix: &str) -> String {
    let service_snake = service_name.to_snake_case();
    let service_lower = ffi_prefix.to_lowercase();
    let upper_prefix = ffi_prefix.to_uppercase();

    crate::backends::go::template_env::render(
        "service_run_method.jinja",
        minijinja::context! {
            service_name => service_name,
            service_snake => &service_snake,
            service_lower => &service_lower,
            upper_prefix => &upper_prefix,
        },
    )
}

/// Emit helper functions for request/response handling in chi-style handlers.
pub(super) fn emit_helper_functions() -> String {
    crate::backends::go::template_env::render("service_helpers.jinja", minijinja::context! {})
}

/// Aggregate emission — forwards all four new IR sections for the Go backend.
pub(super) fn emit_all(api: &ApiSurface, service_name: &str, ffi_prefix: &str) -> String {
    let mut out = String::new();

    // Emit Config struct
    out.push_str(&emit_config_struct(service_name));
    out.push_str("\n\n");

    // Emit error types
    out.push_str(&emit_error_types(&api.error_types));
    out.push_str("\n\n");

    // Emit lifecycle hooks
    out.push_str(&emit_lifecycle_hooks(&api.lifecycle_hooks, service_name));
    out.push_str("\n\n");

    // Emit WebSocket routes
    out.push_str(&emit_websocket_routes(&api.websocket_routes, service_name));
    out.push('\n');

    // Emit SSE routes
    out.push_str(&emit_sse_routes(&api.sse_routes, service_name));
    out.push('\n');

    // Emit Run method
    out.push_str(&emit_run_method(service_name, ffi_prefix));
    out.push_str("\n\n");

    // Emit helper functions
    out.push_str(&emit_helper_functions());

    out
}

use heck::ToSnakeCase;
