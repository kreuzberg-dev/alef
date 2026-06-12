use crate::core::config::TraitBridgeConfig;
use crate::core::ir::TypeDef;
use heck::ToSnakeCase;

use super::method_impls::inbound_return_type;
use super::{has_plugin_super, inbound_bridge_type};

/// Emit the `extern "Rust"` block declaring the `register_*`/`unregister_*` Swift-callable
/// entry points. swift-bridge generates Swift glue that converts `Swift{Trait}Box` instances
/// to retained ARC pointers and forwards them into Rust.
pub(crate) fn emit_extern_block_for_inbound_registration(
    trait_def: &TypeDef,
    bridge_config: &TraitBridgeConfig,
) -> String {
    let trait_name = &trait_def.name;
    let box_name = format!("Swift{trait_name}Box");

    let mut block = String::new();
    let mut has_any = false;
    block.push_str("    extern \"Rust\" {\n");
    if let Some(register_fn) = bridge_config.register_fn.as_deref() {
        let camel = heck::AsLowerCamelCase(register_fn).to_string();
        block.push_str(&crate::backends::swift::template_env::render(
            "inbound_registration_fn.rs.jinja",
            minijinja::context! {
                camel => &camel,
                fn_name => register_fn,
                params => format!("swift_box: {box_name}"),
            },
        ));
        has_any = true;
    }
    if let Some(unregister_fn) = bridge_config.unregister_fn.as_deref() {
        let camel = heck::AsLowerCamelCase(unregister_fn).to_string();
        block.push_str(&crate::backends::swift::template_env::render(
            "inbound_registration_fn.rs.jinja",
            minijinja::context! {
                camel => &camel,
                fn_name => unregister_fn,
                params => "name: String",
            },
        ));
        has_any = true;
    }
    if let Some(clear_fn) = bridge_config.clear_fn.as_deref() {
        let camel = heck::AsLowerCamelCase(clear_fn).to_string();
        block.push_str(&crate::backends::swift::template_env::render(
            "inbound_registration_fn.rs.jinja",
            minijinja::context! {
                camel => &camel,
                fn_name => clear_fn,
                params => "",
            },
        ));
        has_any = true;
    }
    block.push_str("    }\n\n");
    if has_any { block } else { String::new() }
}

/// Emit the `extern "Swift"` block declaring `Swift{Trait}Box` and per-method FFI shims.
///
/// Each shim signature is the JSON-bridged form of the trait method: complex types become
/// `String` (JSON), primitives and `String`/`Vec<u8>` pass through directly. Methods that
/// can fail return `Result<RetBridge, String>` so the Swift side can surface errors.
///
/// Also emits a phantom `Vec<Swift{Trait}Box>` function inside an `extern "Rust"` block
/// to force swift-bridge-build to generate the Vec accessor symbols (`__swift_bridge__$Vec_*`)
/// that the auto-generated Swift Vec extension references.
pub(crate) fn emit_extern_block_for_inbound(trait_def: &TypeDef, bridge_config: &TraitBridgeConfig) -> String {
    let trait_name = &trait_def.name;
    let box_name = format!("Swift{trait_name}Box");
    let _trait_snake = heck::AsSnakeCase(trait_name.as_str()).to_string();
    let emit_plugin_shims = has_plugin_super(bridge_config);

    let mut block = String::new();

    // No inbound (Swift-side) phantom Vec block: `Swift{Trait}Box` is an
    // `extern "Swift" type` with no Rust-side struct backing it at module scope,
    // so neither the `extern "Rust" { fn ... -> Vec<Swift{Trait}Box>; }` declaration
    // nor its matching `pub fn` impl can compile. swift-bridge's auto-generated
    // Vec accessors for inbound traits are not actually used by the bindings we
    // emit (Swift code consumes individual instances, not Vec<>), so omitting the
    // phantom does not break anything. The outbound Rust-side trait_bridge.rs
    // still emits its own phantom for `{Trait}Box`, which is a real `pub struct`
    // at module scope.

    block.push_str("    extern \"Swift\" {\n");
    block.push_str(&crate::backends::swift::template_env::render(
        "inbound_swift_type.rs.jinja",
        minijinja::context! {
            box_name => &box_name,
        },
    ));

    if emit_plugin_shims {
        // Plugin super-trait shims — only emitted when the trait has a Plugin super-trait.
        // We declare these as `&self` methods so swift-bridge treats them as instance methods
        // on `Swift{Trait}Box` and emits the proper `Unmanaged<T>.fromOpaque(this).takeUnretainedValue()`
        // dispatch on the Swift side. Free-fn declarations (with `this: &Box` as a regular param)
        // would force swift-bridge to FFI-encode the box as a value, which breaks for opaque
        // Swift handle types.
        block.push_str("        fn alef_name(&self) -> String;\n");
        block.push_str("        fn alef_version(&self) -> String;\n");
        // initialize/shutdown return a JSON envelope `{"ok":null}` / `{"err":"<msg>"}` —
        // swift-bridge 0.1.59 cannot bridge `Result<(), String>` from `extern "Swift"` (broken
        // codegen for `Result<RustString, RustString>` shape). The wrapper decodes the envelope.
        block.push_str("        fn alef_initialize(&self) -> String;\n");
        block.push_str("        fn alef_shutdown(&self) -> String;\n");
    }

    for method in &trait_def.methods {
        let method_snake = method.name.to_snake_case();

        let mut params = vec!["&self".to_string()];
        for p in &method.params {
            let bridge_ty = if p.optional {
                format!("Option<{}>", inbound_bridge_type(&p.ty))
            } else {
                inbound_bridge_type(&p.ty)
            };
            let name = p.name.to_snake_case();
            params.push(format!("{name}: {bridge_ty}"));
        }

        let return_ty = inbound_return_type(method);
        let params_str = params.join(", ");
        block.push_str(&crate::backends::swift::template_env::render(
            "inbound_swift_method.rs.jinja",
            minijinja::context! {
                method_snake => &method_snake,
                params => &params_str,
                return_ty => &return_ty,
            },
        ));
        let _ = box_name; // silence unused if no methods iter has it
    }

    block.push_str("    }\n\n");
    block
}
