//! Emits service App struct wrappers for the swift-bridge crate.
//!
//! Each service with registrations gets a wrapper struct that exposes:
//! - `App { inner: tokio::sync::Mutex<Option<source_crate::App>> }`
//! - `pub fn new() -> Self`
//! - `pub fn config(&mut self) -> ()`
//! - `pub fn run(self) -> Result<(), String>`
//!
//! Also emits wrapper-constructor free functions (e.g. `route_builder_new`) for registration variants
//! that need to construct a wrapper metadata param (e.g. `RouteBuilder::new(method, path)`) in Swift.
//! These are declared in the bridge `extern "Rust"` block and implemented here.

use crate::core::ir::{ApiSurface, WrapperConstructorArg};
use heck::ToSnakeCase;

/// Generate App wrapper struct and impl for all services with registrations.
pub fn emit_service_app_wrappers(api: &ApiSurface, source_crate: &str) -> String {
    let mut out = String::new();

    if api.services.is_empty() {
        return out;
    }

    for service in &api.services {
        if service.registrations.is_empty() {
            continue;
        }

        let service_name = &service.name;
        let service_path = if service.rust_path.is_empty() {
            format!("{source_crate}::{service_name}")
        } else {
            service.rust_path.clone()
        };
        let constructor = &service.constructor.name;

        out.push_str(&format!(
            "/// Wrapper for {service_name} service instance.\n\
             /// Holds the inner service in a blocking mutex to allow \
             mutable access\n\
             /// across FFI boundaries.\n\
             pub struct {service_name} {{\n\
             \x20\x20\x20\x20pub inner: tokio::sync::Mutex<Option<{service_path}>>,\n\
             }}\n\n"
        ));

        out.push_str(&format!(
            "impl {service_name} {{\n\
             \x20\x20\x20\x20/// Create a new service instance.\n\
             \x20\x20\x20\x20pub fn new() -> Self {{\n\
             \x20\x20\x20\x20\x20\x20\x20\x20Self {{\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20inner: tokio::sync::Mutex::new(Some({service_path}::{constructor}())),\n\
             \x20\x20\x20\x20\x20\x20\x20\x20}}\n\
             \x20\x20\x20\x20}}\n\n"
        ));

        out.push_str(
            "    /// Configure the service.\n\
             \x20\x20\x20\x20pub fn config(&mut self) {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20// Placeholder for future configuration.\n\
             \x20\x20\x20\x20}\n\n",
        );

        // The bridge declares `fn app_run(self: &mut App) -> String;` because swift-bridge 0.1.59
        // does not parse by-value `self: App` consume-self in `extern "Rust"` blocks. The
        // wrapper's `run` therefore takes `&mut self`, `take()`s the inner App out of the
        // Mutex (single-shot consume), and returns a String envelope describing success or
        // the error (Result<T, E> is not bridgeable across this swift-bridge version).
        out.push_str(
            "    /// Run the service (blocking, drives the Tokio runtime).\n\
             \x20\x20\x20\x20///\n\
             \x20\x20\x20\x20/// Returns an empty string on success or the error message.\n\
             \x20\x20\x20\x20pub fn run(&mut self) -> String {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20let rt = match tokio::runtime::Runtime::new() {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20Ok(rt) => rt,\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20Err(e) => return format!(\"runtime error: {:?}\", e),\n\
             \x20\x20\x20\x20\x20\x20\x20\x20};\n\
             \x20\x20\x20\x20\x20\x20\x20\x20rt.block_on(async {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20let mut guard = self.inner.lock().await;\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20if let Some(app) = guard.take() {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20match app.run().await {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20Ok(()) => String::new(),\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20Err(e) => format!(\"{:?}\", e),\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20}\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20} else {\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\"service already consumed\".to_string()\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20}\n\
             \x20\x20\x20\x20\x20\x20\x20\x20})\n\
             \x20\x20\x20\x20}\n\
             }\n\n",
        );

        // swift-bridge extern blocks declare these as free fns taking `client: &mut Foo`
        // (see rust_extern_service_methods.rs.jinja). The bridge then expects matching
        // free fns at the parent module scope, which delegate to the wrapper's
        // inherent methods. The `_raw_ptr` shim exposes the wrapper's raw address as
        // a `usize` so the Swift side can reconstitute it into an `OpaquePointer`
        // for the @_silgen_name'd extern "C" callback registration shim — swift-bridge's
        // generated `ptr` field is `internal` and unreachable from consumer modules.
        let service_snake_local = service_name.to_lowercase();
        out.push_str(&format!(
            "/// Free-function shim so the bridge declaration resolves.\n\
             pub fn config(client: &mut {service_name}) {{ client.config() }}\n\n\
             /// Free-function shim so the bridge declaration resolves.\n\
             pub fn run(client: &mut {service_name}) -> String {{ client.run() }}\n\n\
             /// Expose the wrapper's address as a usize for cross-bridge ptr handoff.\n\
             pub fn {service_snake_local}_raw_ptr(client: &mut {service_name}) -> usize {{ client as *mut {service_name} as usize }}\n\n"
        ));

        // Emit `route_builder_new`-style free functions for each unique WrapperConstructorCall.
        // These are called from Swift variant shorthand methods (e.g. `app.get(handler, path:)`)
        // to construct the wrapper metadata param before invoking the base callback-registration
        // C function. Each function uses serde to parse the opaque enum argument (via its
        // `to_string()` which returns the serde variant wire name) into the Rust core type,
        // then forwards to the wrapper type's static constructor.
        let mut seen_wrapper_fns = std::collections::HashSet::new();
        for reg in &service.registrations {
            for variant in &reg.variants {
                let Some(wc) = &variant.wrapper_call else { continue };
                let fn_name = format!("{}_new", wc.wrapper_type_name.to_snake_case());
                if !seen_wrapper_fns.insert(fn_name.clone()) {
                    continue;
                }
                // Build parameter list: Fixed args use the opaque enum type by reference,
                // Free args use their declared Rust type.
                let mut param_defs: Vec<String> = Vec::new();
                let mut call_args: Vec<String> = Vec::new();
                for arg in &wc.args {
                    match arg {
                        WrapperConstructorArg::Fixed { param_name, value_expr } => {
                            // value_expr is e.g. "source_crate::Method::Get". Extract the type name.
                            let type_name = if let Some(last_colon) = value_expr.rfind("::") {
                                if let Some(second_colon) = value_expr[..last_colon].rfind("::") {
                                    &value_expr[second_colon + 2..last_colon]
                                } else {
                                    &value_expr[..last_colon]
                                }
                            } else {
                                value_expr.as_str()
                            };
                            param_defs.push(format!("{param_name}: &{type_name}"));
                            // Use the opaque type's to_string() (returns serde variant name like
                            // "Get", "Post") to reconstruct the core Rust enum via serde_json.
                            // The wrapper_type_path has the full Rust path for the core enum
                            // (extracted from value_expr: "source_crate::Method::Get" → "source_crate::Method").
                            let core_enum_path = if let Some(last_colon) = value_expr.rfind("::") {
                                &value_expr[..last_colon]
                            } else {
                                value_expr.as_str()
                            };
                            call_args.push(format!(
                                "serde_json::from_str::<{core_enum_path}>(&format!(\"\\\"{{}}\\\"\\n\", {param_name}.to_string())).unwrap_or_default()"
                            ));
                        }
                        WrapperConstructorArg::Free { param } => {
                            param_defs.push(format!("{}: {}", param.name, rust_type_for_param(&param.ty)));
                            call_args.push(param.name.clone());
                        }
                    }
                }
                let params_str = param_defs.join(", ");
                let call_args_str = call_args.join(", ");
                let wrapper_type = &wc.wrapper_type_name;
                let wrapper_path = &wc.wrapper_type_path;
                let ctor = &wc.constructor_method;
                out.push_str(&format!(
                    "/// Construct a [`{wrapper_type}`] from its constructor args for use by Swift variant\n\
                     /// registration shortcuts. Fixed args (e.g. Method) are passed as opaque references;\n\
                     /// their `to_string()` returns the serde wire name used to reconstruct the Rust core type.\n\
                     pub fn {fn_name}({params_str}) -> {wrapper_type} {{\n\
                     \x20\x20\x20\x20{wrapper_type}({wrapper_path}::{ctor}({call_args_str}))\n\
                     }}\n\n"
                ));
            }
        }
    }

    out
}

/// Map a TypeRef to a Rust type string for function parameter declaration.
fn rust_type_for_param(ty: &crate::core::ir::TypeRef) -> &'static str {
    use crate::core::ir::TypeRef;
    match ty {
        TypeRef::String => "String",
        TypeRef::Primitive(_) => "i64", // conservative fallback; callers usually use String for paths
        _ => "String",
    }
}
