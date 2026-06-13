//! Phase C emission: lifecycle hooks, error types, WebSocket/SSE stubs for JNI backend.

use crate::backends::jni::template_env;
use crate::codegen::naming::to_class_name;
use crate::core::config::ResolvedCrateConfig;
use crate::core::ir::{ApiSurface, LifecycleHookDef};
use crate::core::jni::{bridge_method_name, jni_package, jni_symbol, service_bridge_class_name};
use minijinja::context;

// ──────────────────────── Phase-C emission (new IR sections) ──────────────

/// Emit JNI lifecycle-hook registration methods (Rust-side entry points).
///
/// For each `LifecycleHookDef`, generates a `#[no_mangle] extern "system"` JNI function
/// that registers a Java callback for that hook. The hook callback is stored in the
/// service owner and invoked at the appropriate pipeline phase by the native runtime.
pub(crate) fn emit_lifecycle_hooks(
    hooks: &[LifecycleHookDef],
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
) -> String {
    if hooks.is_empty() {
        return String::new();
    }

    let package = jni_package(config);
    let core_import = config.core_import_name();
    let mut out = String::new();

    // Each service may register callbacks for each hook
    for service in &api.services {
        let service_bridge_class = service_bridge_class_name(&service.name);
        for hook in hooks {
            gen_lifecycle_hook_registration(
                &mut out,
                service,
                hook,
                api,
                &core_import,
                &package,
                &service_bridge_class,
            );
        }
    }

    out
}

/// Emit JNI WebSocket route registration methods. Stub for now.
#[allow(dead_code)]
pub(crate) fn emit_websocket_routes(_routes: &[crate::core::ir::WebSocketRouteDef]) -> String {
    // WebSocket handlers use `impl Future` return types that are not object-safe;
    // deferring concrete wrapper monomorphization to later implementation.
    String::new()
}

/// Emit JNI SSE route registration methods. Stub for now.
#[allow(dead_code)]
pub(crate) fn emit_sse_routes(_routes: &[crate::core::ir::SseRouteDef]) -> String {
    // SSE producers use `impl AsyncIterator` return types that are not object-safe;
    // deferring concrete wrapper monomorphization to later implementation.
    String::new()
}

/// Emit JNI native error types (Rust exception class exports).
///
/// For each `ErrorTypeDef`, generates a `#[no_mangle] extern "system"` JNI function
/// that constructs a Java exception class on the host side. The exception carries
/// RFC 9457 ProblemDetails metadata and maps to the correct HTTP status.
pub(crate) fn emit_error_types(types: &[crate::core::ir::ErrorTypeDef], config: &ResolvedCrateConfig) -> String {
    if types.is_empty() {
        return String::new();
    }

    let package = jni_package(config);
    let mut out = String::new();

    for error_type in types {
        gen_error_type_constructor(&mut out, error_type, &package);
    }

    out
}

/// Aggregate emission — forwards all four new IR sections for the JNI backend.
pub(crate) fn emit_new_ir_sections(api: &ApiSurface, config: &ResolvedCrateConfig) -> String {
    let mut out = String::new();
    out.push_str(&emit_lifecycle_hooks(&api.lifecycle_hooks, api, config));
    out.push_str(&emit_websocket_routes(&api.websocket_routes));
    out.push_str(&emit_sse_routes(&api.sse_routes));
    out.push_str(&emit_error_types(&api.error_types, config));
    out
}

// ──────────────────────────────── Phase-C helpers ──────────────────────────

/// Generate a JNI entry point for registering a lifecycle hook callback.
///
/// Emits:
/// ```java,ignore
/// public native void register{Service}{HookName}(Object callback);
/// ```
/// And the corresponding Rust JNI glue that stores the callback handle for native
/// invocation during request processing.
fn gen_lifecycle_hook_registration(
    out: &mut String,
    service: &crate::core::ir::ServiceDef,
    hook: &LifecycleHookDef,
    api: &ApiSurface,
    core_import: &str,
    package: &str,
    service_bridge_class: &str,
) {
    let hook_pascal = to_class_name(&hook.name);
    let service_pascal = to_class_name(&service.name);
    let hook_method = bridge_method_name(&service.name, &format!("register_{}", &hook.name));
    let symbol = jni_symbol(package, service_bridge_class, &hook_method);

    // Find the hook's callback contract
    if let Some(_contract) = api
        .handler_contracts
        .iter()
        .find(|c| c.trait_name == hook.callback_contract)
    {
        let bridge_name = format!("Jni{}Bridge", to_class_name(&hook.callback_contract));
        let opaque_name = format!("{}Opaque", service.name);

        out.push_str(&template_env::render(
            "lifecycle_hook_registration.rs.jinja",
            context! {
                service_pascal => service_pascal,
                hook_pascal => hook_pascal,
                hook_name => hook.name,
                symbol => symbol,
                bridge_name => bridge_name,
                core_import => core_import.to_string(),
                contract_name => hook.callback_contract,
                opaque_name => opaque_name,
                is_async => hook.is_async,
            },
        ));
    }
}

/// Generate a JNI entry point for constructing a typed error instance.
///
/// Emits a `#[no_mangle] extern "system"` function that returns a JObject (the Java
/// exception class) with the error metadata pre-set. The function name follows the
/// pattern `nativeCreate{ErrorTypeName}` so Java code can call it to instantiate
/// the exception with status code and ProblemDetails.
fn gen_error_type_constructor(out: &mut String, error_type: &crate::core::ir::ErrorTypeDef, package: &str) {
    let error_pascal = &error_type.name;
    let status_code = error_type.http_status.as_u16();
    let problem_details_type = error_type.problem_details_type.as_deref().unwrap_or("");
    let method = format!("create{error_pascal}");
    let symbol = jni_symbol(package, "Errors", &method);

    out.push_str(&template_env::render(
        "error_type_constructor.rs.jinja",
        context! {
            error_pascal => error_pascal,
            error_name => error_type.name,
            symbol => symbol,
            status_code => status_code,
            problem_details_type => problem_details_type,
            doc => error_type.doc,
        },
    ));
}
