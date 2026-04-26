//! Ruby (Magnus) specific trait bridge code generation.
//!
//! Generates Rust wrapper structs that implement Rust traits by delegating
//! to Ruby objects via Magnus `respond_to` checks and `funcall`.

pub use alef_codegen::generators::trait_bridge::find_bridge_param;
use alef_codegen::generators::trait_bridge::{
    bridge_param_type as param_type, visitor_param_type, gen_bridge_all, TraitBridgeGenerator,
    TraitBridgeSpec,
};
use alef_core::config::TraitBridgeConfig;
use alef_core::ir::{ApiSurface, MethodDef, TypeDef, TypeRef};
use std::collections::HashMap;
use std::fmt::Write;

/// Generate all trait bridge code for a given trait type and bridge config.
pub fn gen_trait_bridge(
    trait_type: &TypeDef,
    bridge_cfg: &TraitBridgeConfig,
    core_import: &str,
    api: &ApiSurface,
) -> String {
    // Skip if explicitly excluded for Ruby
    if bridge_cfg.exclude_languages.contains(&"ruby".to_string()) {
        return String::new();
    }

    let trait_path = trait_type.rust_path.replace('-', "_");

    // Build type name → rust_path lookup
    let type_paths: HashMap<String, String> = api
        .types
        .iter()
        .map(|t| (t.name.clone(), t.rust_path.replace('-', "_")))
        .chain(
            api.enums
                .iter()
                .map(|e| (e.name.clone(), e.rust_path.replace('-', "_"))),
        )
        .collect();

    // Visitor-style bridge: all methods have defaults, no registry, no super-trait.
    let is_visitor_bridge = bridge_cfg.type_alias.is_some()
        && bridge_cfg.register_fn.is_none()
        && bridge_cfg.super_trait.is_none()
        && trait_type.methods.iter().all(|m| m.has_default_impl);

    if is_visitor_bridge {
        // Visitor pattern: use the old visitor bridge code
        let struct_name = format!("Rb{}Bridge", bridge_cfg.trait_name);
        let mut out = String::with_capacity(8192);
        gen_visitor_bridge(
            &mut out,
            trait_type,
            bridge_cfg,
            &struct_name,
            &trait_path,
            core_import,
            &type_paths,
        );
        out
    } else {
        // Plugin pattern: use the shared TraitBridgeGenerator infrastructure
        let generator = MagnusBridgeGenerator {
            core_import: core_import.to_string(),
            type_paths: type_paths.clone(),
        };
        let spec = TraitBridgeSpec {
            trait_def: trait_type,
            bridge_config: bridge_cfg,
            core_import,
            wrapper_prefix: "Rb",
            type_paths,
            error_type: "Box<dyn std::error::Error + Send + Sync>".to_string(),
            error_constructor: "Box::new({msg})".to_string(),
        };
        let output = gen_bridge_all(&spec, &generator);
        // Add imports to the output (they'll be collected by the caller)
        output.code
    }
}

/// Generate a visitor-style bridge wrapping a Magnus `magnus::Value`.
///
/// Every trait method checks if the Ruby object responds to a snake_case method,
/// then calls it via `funcall` and maps the return value to `VisitResult`.
fn gen_visitor_bridge(
    out: &mut String,
    trait_type: &TypeDef,
    _bridge_cfg: &TraitBridgeConfig,
    struct_name: &str,
    trait_path: &str,
    core_crate: &str,
    type_paths: &std::collections::HashMap<String, String>,
) {
    // Helper: convert NodeContext to a Ruby hash (magnus::RHash)
    writeln!(out, "fn nodecontext_to_rb_hash(").unwrap();
    writeln!(out, "    ctx: &{core_crate}::visitor::NodeContext,").unwrap();
    writeln!(out, ") -> magnus::RHash {{").unwrap();
    writeln!(out, "    let ruby = unsafe {{ magnus::Ruby::get_unchecked() }};").unwrap();
    writeln!(out, "    let h = ruby.hash_new();").unwrap();
    writeln!(
        out,
        "    h.aset(ruby.to_symbol(\"node_type\"), format!(\"{{:?}}\", ctx.node_type)).ok();"
    )
    .unwrap();
    writeln!(
        out,
        "    h.aset(ruby.to_symbol(\"tag_name\"), ctx.tag_name.as_str()).ok();"
    )
    .unwrap();
    writeln!(out, "    h.aset(ruby.to_symbol(\"depth\"), ctx.depth as i64).ok();").unwrap();
    writeln!(
        out,
        "    h.aset(ruby.to_symbol(\"index_in_parent\"), ctx.index_in_parent as i64).ok();"
    )
    .unwrap();
    writeln!(out, "    h.aset(ruby.to_symbol(\"is_inline\"), ctx.is_inline).ok();").unwrap();
    writeln!(
        out,
        "    h.aset(ruby.to_symbol(\"parent_tag\"), ctx.parent_tag.as_deref().map(|s| ruby.str_new(s).as_value())).ok();"
    )
    .unwrap();
    writeln!(out, "    let attrs = ruby.hash_new();").unwrap();
    writeln!(out, "    for (k, v) in &ctx.attributes {{").unwrap();
    writeln!(out, "        attrs.aset(ruby.str_new(k), ruby.str_new(v)).ok();").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    h.aset(ruby.to_symbol(\"attributes\"), attrs).ok();").unwrap();
    writeln!(out, "    h").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Bridge struct
    writeln!(out, "pub struct {struct_name} {{").unwrap();
    writeln!(out, "    rb_obj: magnus::Value,").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Manual Debug impl
    writeln!(out, "impl std::fmt::Debug for {struct_name} {{").unwrap();
    writeln!(
        out,
        "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{"
    )
    .unwrap();
    writeln!(out, "        write!(f, \"{struct_name}\")").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Constructor
    writeln!(out, "impl {struct_name} {{").unwrap();
    writeln!(out, "    pub fn new(rb_obj: magnus::Value) -> Self {{").unwrap();
    writeln!(out, "        Self {{ rb_obj }}").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Trait impl
    writeln!(out, "impl {trait_path} for {struct_name} {{").unwrap();
    for method in &trait_type.methods {
        if method.trait_source.is_some() {
            continue;
        }
        gen_visitor_method_magnus(out, method, type_paths);
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
}

/// Generate a single visitor method that checks Ruby respond_to and calls via funcall.
fn gen_visitor_method_magnus(
    out: &mut String,
    method: &MethodDef,
    type_paths: &std::collections::HashMap<String, String>,
) {
    let name = &method.name;
    // Ruby uses snake_case method names (same as Rust)

    let mut sig_parts = vec!["&mut self".to_string()];
    for p in &method.params {
        let ty_str = visitor_param_type(&p.ty, p.is_ref, p.optional, type_paths);
        sig_parts.push(format!("{}: {}", p.name, ty_str));
    }
    let sig = sig_parts.join(", ");

    let ret_ty = match &method.return_type {
        TypeRef::Named(n) => type_paths
            .get(n.as_str())
            .map(|p| p.replace('-', "_"))
            .unwrap_or_else(|| n.clone()),
        other => param_type(other, "", false, type_paths),
    };

    writeln!(out, "    fn {name}({sig}) -> {ret_ty} {{").unwrap();

    // Check if the Ruby object responds to this method
    writeln!(
        out,
        "        let responds = self.rb_obj.respond_to(\"{name}\", false).unwrap_or(false);"
    )
    .unwrap();
    writeln!(out, "        if !responds {{").unwrap();
    writeln!(out, "            return {ret_ty}::Continue;").unwrap();
    writeln!(out, "        }}").unwrap();

    // Build the funcall args tuple
    if method.params.is_empty() {
        writeln!(
            out,
            "        let result: Result<magnus::Value, magnus::Error> = self.rb_obj.funcall(\"{name}\", ());"
        )
        .unwrap();
    } else {
        // Build args as a tuple
        let args_exprs: Vec<String> = method.params.iter().map(build_magnus_arg).collect();
        let args_tuple = if args_exprs.len() == 1 {
            format!("({},)", args_exprs[0])
        } else {
            format!("({})", args_exprs.join(", "))
        };
        writeln!(
            out,
            "        let result: Result<magnus::Value, magnus::Error> = self.rb_obj.funcall(\"{name}\", {args_tuple});"
        )
        .unwrap();
    }

    // Parse result
    writeln!(out, "        match result {{").unwrap();
    writeln!(out, "            Err(_) => {ret_ty}::Continue,").unwrap();
    writeln!(out, "            Ok(val) => {{").unwrap();
    writeln!(out, "                let s: String = val.to_string();").unwrap();
    writeln!(out, "                match s.to_lowercase().as_str() {{").unwrap();
    writeln!(out, "                    \"continue\" => {ret_ty}::Continue,").unwrap();
    writeln!(out, "                    \"skip\" => {ret_ty}::Skip,").unwrap();
    writeln!(
        out,
        "                    \"preserve_html\" | \"preservehtml\" => {ret_ty}::PreserveHtml,"
    )
    .unwrap();
    writeln!(out, "                    other => {ret_ty}::Custom(other.to_string()),").unwrap();
    writeln!(out, "                }}").unwrap();
    writeln!(out, "            }}").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out).unwrap();
}

/// Build a single Magnus funcall arg expression for a visitor method parameter.
fn build_magnus_arg(p: &alef_core::ir::ParamDef) -> String {
    if let TypeRef::Named(n) = &p.ty {
        if n == "NodeContext" {
            return format!("nodecontext_to_rb_hash({}{})", if p.is_ref { "" } else { "&" }, p.name);
        }
    }
    if p.optional && matches!(&p.ty, TypeRef::String) {
        return format!(
            "match {} {{ Some(s) => magnus::RString::new(s).as_value(), None => magnus::value::qnil().as_value() }}",
            p.name
        );
    }
    if matches!(&p.ty, TypeRef::String) && p.is_ref {
        return format!("magnus::RString::new({})", p.name);
    }
    if matches!(&p.ty, TypeRef::String) {
        return format!("magnus::RString::new({}.as_str())", p.name);
    }
    // Vec/slice types: convert to Ruby array
    if matches!(&p.ty, TypeRef::Vec(_)) {
        let ruby = "unsafe { magnus::Ruby::get_unchecked() }";
        return format!(
            "{{ let arr = {ruby}.ary_new_capa({name}.len()); for item in {name} {{ let _ = arr.push(item.to_string()); }} arr }}",
            name = p.name,
        );
    }
    // For primitive types, pass directly — Magnus funcall handles i32, i64, u32, bool natively.
    p.name.to_string()
}

// ---------------------------------------------------------------------------
// Plugin-pattern bridge generator (shared TraitBridgeGenerator implementation)
// ---------------------------------------------------------------------------

/// Magnus-specific trait bridge generator.
/// Implements code generation for bridging Ruby objects to Rust traits.
struct MagnusBridgeGenerator {
    /// Core crate import path (e.g., `"kreuzberg"`).
    core_import: String,
    /// Map of type name → fully-qualified Rust path for type references.
    type_paths: HashMap<String, String>,
}

impl TraitBridgeGenerator for MagnusBridgeGenerator {
    fn foreign_object_type(&self) -> &str {
        "magnus::Opaque<magnus::Value>"
    }

    fn bridge_imports(&self) -> Vec<String> {
        vec![
            "magnus::prelude::*".to_string(),
            "std::sync::Arc".to_string(),
            "std::sync::Mutex".to_string(),
        ]
    }

    fn gen_sync_method_body(&self, method: &MethodDef, _spec: &TraitBridgeSpec) -> String {
        let name = &method.name;
        let has_error = method.error_type.is_some();
        let mut out = String::with_capacity(512);

        // Magnus requires holding the GVL (Global VM Lock) to call Ruby methods.
        // Use Ruby::get() to acquire it inside a closure.
        writeln!(out, "let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{").ok();
        writeln!(out, "    let ruby = unsafe {{ magnus::Ruby::get_unchecked() }};").ok();
        writeln!(out, "    let value = *self.inner;").ok();

        // Build funcall args
        let args: Vec<String> = method
            .params
            .iter()
            .map(|p| self.ruby_arg_expr(p))
            .collect();

        let call = if args.is_empty() {
            format!("value.funcall::<_, _, magnus::Value>(\"{name}\", ())")
        } else {
            let args_tuple = if args.len() == 1 {
                format!("({},)", args[0])
            } else {
                format!("({})", args.join(", "))
            };
            format!("value.funcall::<_, _, magnus::Value>(\"{name}\", {args_tuple})")
        };

        writeln!(out, "    {call}").ok();
        writeln!(out, "}});").ok();
        writeln!(out).ok();

        writeln!(out, "let val = match result {{").ok();
        writeln!(out, "    Ok(Ok(v)) => v,").ok();
        writeln!(out, "    Ok(Err(e)) => {{").ok();
        writeln!(out, "        let msg = format!(\"Ruby method '{{}}' failed: {{}}\", \"{name}\", e);").ok();
        if has_error {
            writeln!(out, "        return Err(Box::new(std::io::Error::new(").ok();
            writeln!(out, "            std::io::ErrorKind::Other,").ok();
            writeln!(out, "            msg,").ok();
            writeln!(out, "        )));").ok();
        } else {
            writeln!(out, "        return Default::default();").ok();
        }
        writeln!(out, "    }}").ok();
        writeln!(out, "    Err(_) => {{").ok();
        if has_error {
            writeln!(out, "        return Err(Box::new(std::io::Error::new(").ok();
            writeln!(out, "            std::io::ErrorKind::Other,").ok();
            writeln!(out, "            \"Ruby method panicked\",").ok();
            writeln!(out, "        )));").ok();
        } else {
            writeln!(out, "        return Default::default();").ok();
        }
        writeln!(out, "    }}").ok();
        writeln!(out, "}};").ok();

        // Extract and convert return value
        if matches!(method.return_type, TypeRef::Unit) {
            // Unit return type — just return ()
            writeln!(out, "Ok(())").ok();
        } else {
            let ret_type = self.extract_magnus_type(&method.return_type);
            writeln!(out, "val.try_convert::<{ret_type}>()").ok();
            if has_error {
                writeln!(out, "    .map_err(|e| Box::new(std::io::Error::new(").ok();
                writeln!(out, "        std::io::ErrorKind::InvalidData,").ok();
                writeln!(out, "        format!(\"Failed to convert return value: {{}}\", e),").ok();
                writeln!(out, "    )) as Box<dyn std::error::Error + Send + Sync>)").ok();
            } else {
                writeln!(out, "    .unwrap_or_default()").ok();
            }
        }

        out
    }

    fn gen_async_method_body(&self, method: &MethodDef, _spec: &TraitBridgeSpec) -> String {
        let name = &method.name;
        let mut out = String::with_capacity(1024);

        // Magnus is fundamentally sync-only (Ruby is single-threaded with GVL).
        // For async trait methods, we spawn a blocking task that acquires the GVL
        // and calls the Ruby method synchronously. This blocks the async executor
        // but is necessary because Ruby is not Send + Sync.

        writeln!(out, "Box::pin(async move {{").ok();
        writeln!(out, "    let ruby_value = *self.inner;").ok();
        writeln!(out, "    let cached_name = self.cached_name.clone();").ok();

        // Clone/convert params for the blocking closure
        for p in &method.params {
            match (&p.ty, p.is_ref) {
                (TypeRef::String, true) => {
                    writeln!(out, "    let {0}_owned = {0}.to_string();", p.name).ok();
                }
                _ => {
                    writeln!(out, "    let {0}_owned = {0}.clone();", p.name).ok();
                }
            }
        }
        writeln!(out).ok();

        writeln!(out, "    tokio::task::spawn_blocking(move || {{").ok();
        writeln!(out, "        let ruby = unsafe {{ magnus::Ruby::get_unchecked() }};").ok();

        // Build funcall args (using owned copies)
        let args: Vec<String> = method
            .params
            .iter()
            .map(|p| {
                let param_name = if matches!(&p.ty, TypeRef::String) && p.is_ref {
                    format!("{}_owned.as_str()", p.name)
                } else {
                    format!("{}_owned", p.name)
                };
                self.ruby_arg_expr_custom(&p.ty, &param_name)
            })
            .collect();

        let call = if args.is_empty() {
            format!("ruby_value.funcall::<_, _, magnus::Value>(\"{name}\", ())")
        } else {
            let args_tuple = if args.len() == 1 {
                format!("({},)", args[0])
            } else {
                format!("({})", args.join(", "))
            };
            format!("ruby_value.funcall::<_, _, magnus::Value>(\"{name}\", {args_tuple})")
        };

        writeln!(out, "        {call}").ok();
        writeln!(out, "            .map_err(|e| Box::new(std::io::Error::new(").ok();
        writeln!(out, "                std::io::ErrorKind::Other,").ok();
        writeln!(out, "                format!(\"Plugin '{{}}' method '{name}' failed: {{}}\", cached_name, e),").ok();
        writeln!(out, "            )) as Box<dyn std::error::Error + Send + Sync>)").ok();
        writeln!(out, "    }})").ok();
        writeln!(out, "    .await").ok();
        writeln!(out, "    .map_err(|e| Box::new(std::io::Error::new(").ok();
        writeln!(out, "        std::io::ErrorKind::Other,").ok();
        writeln!(out, "        format!(\"spawn_blocking failed: {{}}\", e),").ok();
        writeln!(out, "    )) as Box<dyn std::error::Error + Send + Sync>)?").ok();

        // Extract and convert return value
        if !matches!(method.return_type, TypeRef::Unit) {
            let ret_type = self.extract_magnus_type(&method.return_type);
            writeln!(out, "    .try_convert::<{ret_type}>()").ok();
            writeln!(out, "    .map_err(|e| Box::new(std::io::Error::new(").ok();
            writeln!(out, "        std::io::ErrorKind::InvalidData,").ok();
            writeln!(out, "        format!(\"Failed to convert return value: {{}}\", e),").ok();
            writeln!(out, "    )) as Box<dyn std::error::Error + Send + Sync>)").ok();
        } else {
            writeln!(out, "    Ok(())").ok();
        }

        writeln!(out, "}})").ok();
        out
    }

    fn gen_constructor(&self, spec: &TraitBridgeSpec) -> String {
        let wrapper = spec.wrapper_name();
        let mut out = String::with_capacity(512);

        writeln!(out, "impl {wrapper} {{").ok();
        writeln!(
            out,
            "    /// Create a new bridge wrapping a Ruby object."
        )
        .ok();
        writeln!(
            out,
            "    /// Validates that the Ruby object responds to all required methods."
        )
        .ok();
        writeln!(
            out,
            "    pub fn new(rb_obj: magnus::Value, name: String) -> Result<Self, magnus::Error> {{"
        )
        .ok();

        // Validate required methods respond_to?
        for req_method in spec.required_methods() {
            writeln!(
                out,
                "        if !rb_obj.respond_to(\"{}\", false).unwrap_or(false) {{",
                req_method.name
            )
            .ok();
            let ruby = "unsafe { magnus::Ruby::get_unchecked() }";
            writeln!(out, "            let ruby = {ruby};").ok();
            writeln!(
                out,
                "            return Err(magnus::Error::new("
            )
            .ok();
            writeln!(
                out,
                "                ruby.exception_runtime_error(),"
            )
            .ok();
            writeln!(
                out,
                "                format!(\"Ruby object missing required method: {{}}\", \"{}\"),",
                req_method.name
            )
            .ok();
            writeln!(out, "            ));").ok();
            writeln!(out, "        }}").ok();
        }

        writeln!(out).ok();
        writeln!(out, "        Ok(Self {{").ok();
        writeln!(out, "            inner: magnus::Opaque::new(rb_obj),").ok();
        writeln!(out, "            cached_name: name,").ok();
        writeln!(out, "        }})").ok();
        writeln!(out, "    }}").ok();
        writeln!(out, "}}").ok();
        out
    }

    fn gen_registration_fn(&self, spec: &TraitBridgeSpec) -> String {
        let Some(register_fn) = spec.bridge_config.register_fn.as_deref() else {
            return String::new();
        };
        let Some(_registry_getter) = spec.bridge_config.registry_getter.as_deref() else {
            return String::new();
        };
        let wrapper = spec.wrapper_name();
        let trait_path = spec.trait_path();

        let mut out = String::with_capacity(1024);

        // Magnus module init function: #[magnus::init]
        writeln!(out, "#[magnus::init]").ok();
        writeln!(
            out,
            "pub fn init() -> magnus::RModule {{"
        )
        .ok();
        writeln!(out, "    let module = magnus::define_module(\"Kreuzberg\").unwrap();").ok();
        writeln!(out).ok();

        writeln!(
            out,
            "    module.define_singleton_method(\"{register_fn}\", magnus::function!"
        )
        .ok();
        writeln!(out, "        fn register_bridge(rb_obj: magnus::Value, name: String) -> Result<(), magnus::Error> {{").ok();

        // Validate required methods
        let req_methods: Vec<_> = spec.required_methods();
        if !req_methods.is_empty() {
            writeln!(out, "            let required_methods = [{}];", req_methods
                .iter()
                .map(|m| format!("\"{}\"", m.name))
                .collect::<Vec<_>>()
                .join(", "))
                .ok();
            writeln!(out, "            for method in &required_methods {{").ok();
            writeln!(out, "                if !rb_obj.respond_to(*method, false).unwrap_or(false) {{").ok();
            writeln!(out, "                    let ruby = unsafe {{ magnus::Ruby::get_unchecked() }};").ok();
            writeln!(
                out,
                "                    return Err(magnus::Error::new("
            )
            .ok();
            writeln!(
                out,
                "                        ruby.exception_runtime_error(),"
            )
            .ok();
            writeln!(
                out,
                "                        format!(\"Backend missing required method: {{}}\", method),"
            )
            .ok();
            writeln!(out, "                    ));").ok();
            writeln!(out, "                }}").ok();
            writeln!(out, "            }}").ok();
        }

        writeln!(out).ok();
        writeln!(out, "            let wrapper = {wrapper}::new(rb_obj, name)?;").ok();
        writeln!(out, "            let arc: Arc<dyn {trait_path}> = Arc::new(wrapper);").ok();
        writeln!(out).ok();

        // Register in the plugin registry
        let _extra = spec
            .bridge_config
            .register_extra_args
            .as_deref()
            .map(|a| format!(", {a}"))
            .unwrap_or_default();
        writeln!(out, "            // Register in the backend registry").ok();
        writeln!(out, "            // Note: registry interaction is deferred to Rust code").ok();
        writeln!(out, "            Ok(())").ok();

        writeln!(out, "        }},").ok();
        writeln!(out, "    ).unwrap();").ok();
        writeln!(out).ok();
        writeln!(out, "    module").ok();
        writeln!(out, "}}").ok();
        out
    }
}

impl MagnusBridgeGenerator {
    /// Convert a Rust TypeRef to its Magnus type representation for extraction.
    fn extract_magnus_type(&self, ty: &TypeRef) -> String {
        match ty {
            TypeRef::Primitive(p) => {
                use alef_core::ir::PrimitiveType::*;
                match p {
                    Bool => "bool",
                    U8 | U16 | U32 => "u32",
                    U64 => "u64",
                    I8 | I16 | I32 => "i32",
                    I64 => "i64",
                    F32 => "f32",
                    F64 => "f64",
                    Usize => "usize",
                    Isize => "isize",
                }
                .to_string()
            }
            TypeRef::String => "String".to_string(),
            TypeRef::Bytes => "Vec<u8>".to_string(),
            TypeRef::Vec(inner) => format!("Vec<{}>", self.extract_magnus_type(inner)),
            TypeRef::Optional(inner) => format!("Option<{}>", self.extract_magnus_type(inner)),
            TypeRef::Named(name) => self
                .type_paths
                .get(name.as_str())
                .cloned()
                .unwrap_or_else(|| format!("{}::{}", self.core_import, name)),
            TypeRef::Unit => "()".to_string(),
            TypeRef::Map(k, v) => format!(
                "std::collections::HashMap<{}, {}>",
                self.extract_magnus_type(k),
                self.extract_magnus_type(v)
            ),
            TypeRef::Json => "serde_json::Value".to_string(),
            TypeRef::Duration => "std::time::Duration".to_string(),
            TypeRef::Char => "char".to_string(),
            TypeRef::Path => "std::path::PathBuf".to_string(),
        }
    }

    /// Build a Ruby arg expression for funcall given a Rust parameter.
    fn ruby_arg_expr(&self, p: &alef_core::ir::ParamDef) -> String {
        self.ruby_arg_expr_custom(&p.ty, &p.name)
    }

    /// Build a Ruby arg expression for funcall given a type and variable name.
    fn ruby_arg_expr_custom(&self, ty: &TypeRef, var: &str) -> String {
        match ty {
            TypeRef::String => format!("magnus::RString::new({}).as_value()", var),
            TypeRef::Bytes => format!("magnus::RString::new(String::from_utf8_lossy({}).as_ref()).as_value()", var),
            TypeRef::Named(_) => {
                format!("serde_json::to_string({}).ok().map(|s| magnus::RString::new(s).as_value()).unwrap_or_else(|| magnus::value::qnil().as_value())", var)
            }
            TypeRef::Vec(_) => {
                format!("{{ let ruby = unsafe {{ magnus::Ruby::get_unchecked() }}; let arr = ruby.ary_new(); for item in {} {{ let _ = arr.push(item); }} arr.as_value() }}", var)
            }
            _ => var.to_string(),
        }
    }
}

/// Generate a Magnus free function that has one parameter replaced by `magnus::Value` (a trait
/// bridge). The bridge is constructed before calling the core function.
#[allow(clippy::too_many_arguments)]
pub fn gen_bridge_function(
    func: &alef_core::ir::FunctionDef,
    bridge_param_idx: usize,
    bridge_cfg: &TraitBridgeConfig,
    mapper: &dyn alef_codegen::type_mapper::TypeMapper,
    opaque_types: &ahash::AHashSet<String>,
    default_types: &std::collections::HashSet<&str>,
    core_import: &str,
) -> String {
    use alef_core::ir::TypeRef;

    let struct_name = format!("Rb{}Bridge", bridge_cfg.trait_name);
    let handle_path = format!("{core_import}::visitor::VisitorHandle");
    let param_name = &func.params[bridge_param_idx].name;
    let bridge_param = &func.params[bridge_param_idx];
    let is_optional = bridge_param.optional || matches!(&bridge_param.ty, TypeRef::Optional(_));

    // Build parameter list
    let mut sig_parts = Vec::new();
    for (idx, p) in func.params.iter().enumerate() {
        if idx == bridge_param_idx {
            if is_optional {
                sig_parts.push(format!("{}: Option<magnus::Value>", p.name));
            } else {
                sig_parts.push(format!("{}: magnus::Value", p.name));
            }
        } else {
            let promoted = idx > bridge_param_idx || func.params[..idx].iter().any(|pp| pp.optional);
            // default_types are passed as JSON strings at the NIF boundary
            let is_default_type = match &p.ty {
                TypeRef::Named(n) => default_types.contains(n.as_str()),
                TypeRef::Optional(inner) => {
                    matches!(inner.as_ref(), TypeRef::Named(n) if default_types.contains(n.as_str()))
                }
                _ => false,
            };
            let ty = if is_default_type {
                if p.optional || promoted {
                    "Option<String>".to_string()
                } else {
                    "String".to_string()
                }
            } else if p.optional || promoted {
                format!("Option<{}>", mapper.map_type(&p.ty))
            } else {
                mapper.map_type(&p.ty)
            };
            sig_parts.push(format!("{}: {}", p.name, ty));
        }
    }

    let params_str = sig_parts.join(", ");
    let return_type = mapper.map_type(&func.return_type);
    // Magnus functions with errors always return Result
    let has_error = func.error_type.is_some();
    let ret = mapper.wrap_return(&return_type, has_error);

    let err_conv = ".map_err(|e| magnus::Error::new(unsafe { magnus::Ruby::get_unchecked() }.exception_runtime_error(), e.to_string()))";

    // Bridge wrapping code
    let bridge_wrap = if is_optional {
        format!(
            "let {param_name}: Option<{handle_path}> = match {param_name} {{\n        \
             Some(v) if !v.is_nil() => {{\n            \
             let bridge = {struct_name}::new(v);\n            \
             Some(std::rc::Rc::new(std::cell::RefCell::new(bridge)) as {handle_path})\n        \
             }},\n        \
             _ => None,\n    \
             }};"
        )
    } else {
        format!(
            "let {param_name} = {{\n        \
             let bridge = {struct_name}::new({param_name});\n        \
             std::rc::Rc::new(std::cell::RefCell::new(bridge)) as {handle_path}\n    \
             }};"
        )
    };

    // Serde let bindings for non-bridge Named params
    let serde_bindings: String = func
        .params
        .iter()
        .enumerate()
        .filter(|(idx, p)| {
            if *idx == bridge_param_idx {
                return false;
            }
            let named = match &p.ty {
                TypeRef::Named(n) => Some(n.as_str()),
                TypeRef::Optional(inner) => {
                    if let TypeRef::Named(n) = inner.as_ref() {
                        Some(n.as_str())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            named.is_some_and(|n| !opaque_types.contains(n))
        })
        .map(|(_, p)| {
            let name = &p.name;
            let core_path = format!(
                "{core_import}::{}",
                match &p.ty {
                    TypeRef::Named(n) => n.clone(),
                    TypeRef::Optional(inner) =>
                        if let TypeRef::Named(n) = inner.as_ref() {
                            n.clone()
                        } else {
                            String::new()
                        },
                    _ => String::new(),
                }
            );
            if p.optional || matches!(&p.ty, TypeRef::Optional(_)) {
                format!(
                    "let {name}_core: Option<{core_path}> = {name}.as_deref().filter(|s| *s != \"nil\").map(|s| serde_json::from_str(s){err_conv}).transpose()?;\n    "
                )
            } else {
                format!(
                    "let {name}_core: {core_path} = serde_json::from_str(&{name}){err_conv}?;\n    "
                )
            }
        })
        .collect();

    // Build call args
    let call_args: Vec<String> = func
        .params
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            if idx == bridge_param_idx {
                return p.name.clone();
            }
            match &p.ty {
                TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
                    if p.optional {
                        format!("{}.as_ref().map(|v| &v.inner)", p.name)
                    } else {
                        format!("&{}.inner", p.name)
                    }
                }
                TypeRef::Named(_) => format!("{}_core", p.name),
                TypeRef::Optional(inner) => {
                    if let TypeRef::Named(n) = inner.as_ref() {
                        if opaque_types.contains(n.as_str()) {
                            format!("{}.as_ref().map(|v| &v.inner)", p.name)
                        } else {
                            format!("{}_core", p.name)
                        }
                    } else {
                        p.name.clone()
                    }
                }
                TypeRef::String | TypeRef::Char => {
                    if p.is_ref {
                        format!("&{}", p.name)
                    } else {
                        p.name.clone()
                    }
                }
                _ => p.name.clone(),
            }
        })
        .collect();
    let call_args_str = call_args.join(", ");

    let core_fn_path = {
        let path = func.rust_path.replace('-', "_");
        if path.starts_with(core_import) {
            path
        } else {
            format!("{core_import}::{}", func.name)
        }
    };
    let core_call = format!("{core_fn_path}({call_args_str})");

    let return_wrap = match &func.return_type {
        TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
            format!("{name} {{ inner: std::sync::Arc::new(val) }}")
        }
        TypeRef::Named(_) => "val.into()".to_string(),
        TypeRef::String | TypeRef::Bytes => "val.into()".to_string(),
        _ => "val".to_string(),
    };

    let body = if func.error_type.is_some() {
        if return_wrap == "val" {
            format!("{bridge_wrap}\n    {serde_bindings}{core_call}{err_conv}")
        } else {
            format!("{bridge_wrap}\n    {serde_bindings}{core_call}.map(|val| {return_wrap}){err_conv}")
        }
    } else {
        format!("{bridge_wrap}\n    {serde_bindings}{core_call}")
    };

    let func_name = &func.name;
    let mut out = String::with_capacity(1024);
    if func.error_type.is_some() {
        writeln!(out, "#[allow(clippy::missing_errors_doc)]").ok();
    }
    writeln!(out, "pub fn {func_name}({params_str}) -> {ret} {{").ok();
    writeln!(out, "    {body}").ok();
    writeln!(out, "}}").ok();

    out
}
