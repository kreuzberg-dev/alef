//! PHP (ext-php-rs) specific trait bridge code generation.
//!
//! Generates Rust wrapper structs that implement Rust traits by delegating
//! to PHP objects via ext-php-rs Zval method calls.

use minijinja::context;

use crate::codegen::generators::trait_bridge::{BridgeOutput, TraitBridgeGenerator, TraitBridgeSpec, gen_bridge_all};
use crate::core::config::TraitBridgeConfig;
use crate::core::ir::{ApiSurface, MethodDef, TypeDef, TypeRef};
use std::collections::HashMap;

use super::visitor::gen_visitor_bridge;

/// PHP-specific trait bridge generator.
/// Implements code generation for bridging PHP objects to Rust traits.
pub struct PhpBridgeGenerator {
    /// Core crate import path (e.g., `"sample_core"`).
    pub core_import: String,
    /// Map of type name → fully-qualified Rust path for type references.
    pub type_paths: HashMap<String, String>,
    /// Error type name (e.g., `"SampleCrateError"`).
    pub error_type: String,
}

impl TraitBridgeGenerator for PhpBridgeGenerator {
    fn foreign_object_type(&self) -> &str {
        "*mut ext_php_rs::types::ZendObject"
    }

    fn bridge_imports(&self) -> Vec<String> {
        vec!["std::sync::Arc".to_string(), "ext_php_rs::rc::PhpRc".to_string()]
    }

    fn gen_sync_method_body(&self, method: &MethodDef, spec: &TraitBridgeSpec) -> String {
        let name = &method.name;

        let has_args = !method.params.is_empty();
        let args_expr = if has_args {
            let mut args_parts = Vec::new();
            for p in &method.params {
                let arg_expr = match &p.ty {
                    TypeRef::String => format!("ext_php_rs::types::Zval::try_from({}).unwrap_or_default()", p.name),
                    TypeRef::Path => format!(
                        "ext_php_rs::types::Zval::try_from({}.to_string_lossy().to_string()).unwrap_or_default()",
                        p.name
                    ),
                    TypeRef::Bytes => format!(
                        "ext_php_rs::types::Zval::try_from(format!(\"{{:?}}\", {})).unwrap_or_default()",
                        p.name
                    ),
                    TypeRef::Named(_) => {
                        format!(
                            "ext_php_rs::types::Zval::try_from(serde_json::to_string(&{}).unwrap_or_default()).unwrap_or_default()",
                            p.name
                        )
                    }
                    TypeRef::Primitive(_) => {
                        format!("ext_php_rs::types::Zval::try_from({}).unwrap_or_default()", p.name)
                    }
                    _ => format!(
                        "ext_php_rs::types::Zval::try_from(format!(\"{{:?}}\", {})).unwrap_or_default()",
                        p.name
                    ),
                };
                args_parts.push(arg_expr);
            }
            let args_array = format!("[{}]", args_parts.join(", "));
            format!(
                "{}.iter().map(|z| z as &dyn ext_php_rs::convert::IntoZvalDyn).collect()",
                args_array
            )
        } else {
            "vec![]".to_string()
        };

        let is_result_type = method.error_type.is_some();
        let is_unit_return = matches!(method.return_type, TypeRef::Unit);
        let is_primitive_return = matches!(&method.return_type, TypeRef::Primitive(_));

        let return_type = match &method.return_type {
            TypeRef::Named(n) => self
                .type_paths
                .get(n.as_str())
                .map(|p| p.replace('-', "_"))
                .unwrap_or_else(|| n.clone()),
            other => crate::codegen::generators::trait_bridge::format_type_ref(other, &self.type_paths),
        };

        let deserialize_error_expr = spec.make_error("format!(\"Deserialize error: {}\", e)");
        let call_error_expr = spec.make_error("e.to_string()");

        crate::backends::php::template_env::render(
            "sync_method_body.jinja",
            context! {
                method_name => name,
                args_expr => args_expr,
                is_result_type => is_result_type,
                is_unit_return => is_unit_return,
                is_primitive_return => is_primitive_return,
                return_type => return_type,
                deserialize_error_expr => deserialize_error_expr,
                call_error_expr => call_error_expr,
            },
        )
    }

    fn gen_async_method_body(&self, method: &MethodDef, spec: &TraitBridgeSpec) -> String {
        let name = &method.name;

        let string_params: Vec<String> = method
            .params
            .iter()
            .filter(|p| matches!(&p.ty, TypeRef::String))
            .map(|p| p.name.clone())
            .collect();

        let has_args = !method.params.is_empty();
        let args_expr = if has_args {
            let mut args_parts = Vec::new();
            for p in &method.params {
                let arg_expr = match &p.ty {
                    TypeRef::String => format!("ext_php_rs::types::Zval::try_from({}).unwrap_or_default()", p.name),
                    TypeRef::Path => format!(
                        "ext_php_rs::types::Zval::try_from({}.to_string_lossy().to_string()).unwrap_or_default()",
                        p.name
                    ),
                    TypeRef::Bytes => format!(
                        "ext_php_rs::types::Zval::try_from(format!(\"{{:?}}\", {})).unwrap_or_default()",
                        p.name
                    ),
                    TypeRef::Named(_) => {
                        format!(
                            "ext_php_rs::types::Zval::try_from(serde_json::to_string(&{}).unwrap_or_default()).unwrap_or_default()",
                            p.name
                        )
                    }
                    TypeRef::Primitive(_) => {
                        format!("ext_php_rs::types::Zval::try_from({}).unwrap_or_default()", p.name)
                    }
                    _ => format!(
                        "ext_php_rs::types::Zval::try_from(format!(\"{{:?}}\", {})).unwrap_or_default()",
                        p.name
                    ),
                };
                args_parts.push(arg_expr);
            }
            let args_array = format!("[{}]", args_parts.join(", "));
            format!(
                "{}.iter().map(|z| z as &dyn ext_php_rs::convert::IntoZvalDyn).collect()",
                args_array
            )
        } else {
            "vec![]".to_string()
        };

        let is_result_type = method.error_type.is_some();
        let deserialize_error_expr = spec.make_error("format!(\"Deserialize error: {}\", e)");
        let call_error_expr = spec.make_error(&format!(
            "format!(\"Plugin '{{}}' method '{name}' failed: {{}}\", cached_name, e)"
        ));

        crate::backends::php::template_env::render(
            "async_method_body.jinja",
            context! {
                method_name => name,
                args_expr => args_expr,
                string_params => string_params,
                is_result_type => is_result_type,
                deserialize_error_expr => deserialize_error_expr,
                call_error_expr => call_error_expr,
            },
        )
    }

    fn gen_constructor(&self, spec: &TraitBridgeSpec) -> String {
        let wrapper = spec.wrapper_name();

        crate::backends::php::template_env::render(
            "bridge_constructor.jinja",
            context! {
                wrapper => &wrapper,
            },
        )
    }

    fn gen_unregistration_fn(&self, spec: &TraitBridgeSpec) -> String {
        let Some(unregister_fn) = spec.bridge_config.unregister_fn.as_deref() else {
            return String::new();
        };
        let host_path = crate::codegen::generators::trait_bridge::host_function_path(spec, unregister_fn);

        crate::backends::php::template_env::render(
            "bridge_unregister_fn.jinja",
            context! {
                unregister_fn => unregister_fn,
                host_path => &host_path,
            },
        )
    }

    fn gen_clear_fn(&self, spec: &TraitBridgeSpec) -> String {
        let Some(clear_fn) = spec.bridge_config.clear_fn.as_deref() else {
            return String::new();
        };
        let host_path = crate::codegen::generators::trait_bridge::host_function_path(spec, clear_fn);

        crate::backends::php::template_env::render(
            "bridge_clear_fn.jinja",
            context! {
                clear_fn => clear_fn,
                host_path => &host_path,
            },
        )
    }

    fn gen_registration_fn(&self, spec: &TraitBridgeSpec) -> String {
        let Some(register_fn) = spec.bridge_config.register_fn.as_deref() else {
            return String::new();
        };
        let Some(registry_getter) = spec.bridge_config.registry_getter.as_deref() else {
            return String::new();
        };
        let wrapper = spec.wrapper_name();
        let trait_path = spec.trait_path();

        let req_methods: Vec<&MethodDef> = spec.required_methods();
        let required_methods: Vec<minijinja::Value> = req_methods
            .iter()
            .map(|m| {
                minijinja::context! {
                    name => m.name.as_str(),
                }
            })
            .collect();

        let extra_args = spec
            .bridge_config
            .register_extra_args
            .as_deref()
            .map(|a| format!(", {a}"))
            .unwrap_or_default();

        crate::backends::php::template_env::render(
            "bridge_registration_fn.jinja",
            context! {
                register_fn => register_fn,
                required_methods => required_methods,
                wrapper => &wrapper,
                trait_path => &trait_path,
                registry_getter => registry_getter,
                extra_args => &extra_args,
            },
        )
    }
}

/// Generate all trait bridge code for a given trait type and bridge config.
pub fn gen_trait_bridge(
    trait_type: &TypeDef,
    bridge_cfg: &TraitBridgeConfig,
    core_import: &str,
    error_type: &str,
    error_constructor: &str,
    api: &ApiSurface,
) -> BridgeOutput {
    // Build type name → rust_path lookup as owned HashMap
    let type_paths: HashMap<String, String> = api
        .types
        .iter()
        .map(|t| (t.name.clone(), t.rust_path.replace('-', "_")))
        .chain(
            api.enums
                .iter()
                .map(|e| (e.name.clone(), e.rust_path.replace('-', "_"))),
        )
        // Include excluded types so trait methods referencing them (e.g. `&InternalDocument`)
        // are qualified with the full Rust path rather than emitting the bare type name.
        .chain(
            api.excluded_type_paths
                .iter()
                .map(|(name, path)| (name.clone(), path.replace('-', "_"))),
        )
        .collect();

    // Visitor-style bridge: all methods have defaults, no registry, no super-trait.
    let is_visitor_bridge = bridge_cfg.type_alias.is_some()
        && bridge_cfg.register_fn.is_none()
        && bridge_cfg.super_trait.is_none()
        && bridge_cfg.context_type.is_some()
        && bridge_cfg.result_type.is_some()
        && trait_type.methods.iter().all(|m| m.has_default_impl);

    if is_visitor_bridge {
        let struct_name = format!("Php{}Bridge", bridge_cfg.trait_name);
        let trait_path = trait_type.rust_path.replace('-', "_");
        let code = gen_visitor_bridge(trait_type, bridge_cfg, &struct_name, &trait_path, &type_paths, api);

        // Note: PHP interface file generation is handled separately by the PHP backend
        // in generate_bindings() to emit it as a standalone PHP file, not inline Rust code.
        //
        // The visitor-bridge struct uses `inc_count()`/`dec_count()` from the `PhpRc`
        // trait in its Clone/Drop/new impls (see `visitor_bridge_struct.jinja` and
        // `bridge_constructor.jinja`) — the trait must be in scope at the binding-crate
        // root or those calls fail with E0599 "no method named inc_count for _zend_object".
        BridgeOutput {
            imports: vec!["ext_php_rs::rc::PhpRc".to_string()],
            code,
        }
    } else {
        // Use the IR-driven TraitBridgeGenerator infrastructure
        let generator = PhpBridgeGenerator {
            core_import: core_import.to_string(),
            type_paths: type_paths.clone(),
            error_type: error_type.to_string(),
        };
        let lifetime_type_names: std::collections::HashSet<String> = api
            .types
            .iter()
            .filter(|t| t.has_lifetime_params)
            .map(|t| t.name.clone())
            .collect();
        let spec = TraitBridgeSpec {
            trait_def: trait_type,
            bridge_config: bridge_cfg,
            core_import,
            wrapper_prefix: "Php",
            type_paths,
            lifetime_type_names,
            error_type: error_type.to_string(),
            error_constructor: error_constructor.to_string(),
        };
        gen_bridge_all(&spec, &generator)
    }
}
