use crate::builder::StructBuilder;
use crate::shared::{constructor_parts, function_params, function_sig_defaults, partition_methods};
use crate::type_mapper::TypeMapper;
use ahash::{AHashMap, AHashSet};
use skif_core::ir::{EnumDef, FunctionDef, MethodDef, ParamDef, TypeDef, TypeRef};
use std::fmt::Write;

/// Map of adapter-generated method/function bodies.
/// Key: "TypeName.method_name" for methods, "function_name" for free functions.
pub type AdapterBodies = AHashMap<String, String>;

/// Async support pattern for the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncPattern {
    /// No async support
    None,
    /// PyO3: pyo3_async_runtimes::tokio::future_into_py
    Pyo3FutureIntoPy,
    /// NAPI-RS: native async fn → auto-Promise
    NapiNativeAsync,
    /// wasm-bindgen: native async fn → auto-Promise
    WasmNativeAsync,
    /// Block on Tokio runtime (Ruby, PHP)
    TokioBlockOn,
}

/// Configuration for Rust binding code generation.
pub struct RustBindingConfig<'a> {
    /// Attrs applied to generated structs, e.g. `["pyclass(frozen)"]`.
    pub struct_attrs: &'a [&'a str],
    /// Attrs applied to each field, e.g. `["pyo3(get)"]`.
    pub field_attrs: &'a [&'a str],
    /// Derives applied to generated structs, e.g. `["Clone"]`.
    pub struct_derives: &'a [&'a str],
    /// Attr wrapping the impl block, e.g. `Some("pymethods")`.
    pub method_block_attr: Option<&'a str>,
    /// Attr placed on the constructor, e.g. `"#[new]"`.
    pub constructor_attr: &'a str,
    /// Attr placed on static methods, e.g. `Some("staticmethod")`.
    pub static_attr: Option<&'a str>,
    /// Attr placed on free functions, e.g. `"#[pyfunction]"`.
    pub function_attr: &'a str,
    /// Attrs applied to generated enums, e.g. `["pyclass(eq, eq_int)"]`.
    pub enum_attrs: &'a [&'a str],
    /// Derives applied to generated enums, e.g. `["Clone", "PartialEq"]`.
    pub enum_derives: &'a [&'a str],
    /// Whether the backend requires `#[pyo3(signature = (...))]`-style annotations.
    pub needs_signature: bool,
    /// Prefix for the signature annotation, e.g. `"#[pyo3(signature = ("`.
    pub signature_prefix: &'a str,
    /// Suffix for the signature annotation, e.g. `"))]"`.
    pub signature_suffix: &'a str,
    /// Core crate import path, e.g. `"liter_llm"`. Used to generate calls into core.
    pub core_import: &'a str,
    /// Async pattern supported by this backend.
    pub async_pattern: AsyncPattern,
}

/// Wrap a core-call result for opaque delegation methods.
///
/// - `TypeRef::Named(n)` where `n == type_name` → re-wrap in `Self { inner: Arc::new(...) }`
/// - `TypeRef::Named(n)` where `n` is another opaque type → wrap in `{n} { inner: Arc::new(...) }`
/// - `TypeRef::Named(n)` where `n` is a non-opaque type → `todo!()` placeholder (From may not exist)
/// - Everything else (primitives, String, Vec, etc.) → pass through unchanged
/// - `TypeRef::Unit` → pass through unchanged
fn wrap_opaque_return(expr: &str, return_type: &TypeRef, type_name: &str, opaque_types: &AHashSet<String>) -> String {
    match return_type {
        TypeRef::Named(n) if n == type_name => {
            format!("Self {{ inner: std::sync::Arc::new({expr}) }}")
        }
        TypeRef::Named(n) if opaque_types.contains(n.as_str()) => {
            format!("{n} {{ inner: std::sync::Arc::new({expr}) }}")
        }
        TypeRef::Named(n) => {
            // Non-opaque Named return type — From impl may not exist, use todo!()
            format!("todo!(\"convert return type {n} from core\")")
        }
        // String/Bytes: .into() handles &str→String, &[u8]→Vec<u8>
        TypeRef::String | TypeRef::Bytes => format!("{expr}.into()"),
        // Path: PathBuf→String needs to_string_lossy, &Path→String too
        TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
        _ => expr.to_string(),
    }
}

/// Generate the body for an async call, unified across methods, static methods, and free functions.
///
/// - `core_call`: the expression to await, e.g. `inner.method(args)` or `CoreType::fn(args)`.
///   For Pyo3FutureIntoPy opaque methods this should reference `inner` (the Arc clone);
///   for all other patterns it may reference `self.inner` or a static call expression.
/// - `cfg`: binding configuration (determines which async pattern to emit)
/// - `has_error`: whether the core call returns a `Result`
/// - `return_wrap`: expression to produce the binding return value from `result`,
///   e.g. `"result"` or `"TypeName::from(result)"`
/// - `is_opaque`: whether the binding type is Arc-wrapped (affects TokioBlockOn wrapping)
/// - `inner_clone_line`: optional statement emitted before the pattern-specific body,
///   e.g. `"let inner = self.inner.clone();\n        "` for opaque instance methods, or `""`.
///   Required when `core_call` references `inner` (Pyo3FutureIntoPy opaque case).
fn gen_async_body(
    core_call: &str,
    cfg: &RustBindingConfig,
    has_error: bool,
    return_wrap: &str,
    is_opaque: bool,
    inner_clone_line: &str,
) -> String {
    let pattern_body = match cfg.async_pattern {
        AsyncPattern::Pyo3FutureIntoPy => {
            let result_handling = if has_error {
                format!(
                    "let result = {core_call}.await\n            \
                     .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;"
                )
            } else {
                format!("let result = {core_call}.await;")
            };
            format!(
                "pyo3_async_runtimes::tokio::future_into_py(py, async move {{\n            \
                 {result_handling}\n            \
                 Ok({return_wrap})\n        }})"
            )
        }
        AsyncPattern::WasmNativeAsync => {
            let result_handling = if has_error {
                format!(
                    "let result = {core_call}.await\n        \
                     .map_err(|e| JsValue::from_str(&e.to_string()))?;"
                )
            } else {
                format!("let result = {core_call}.await;")
            };
            format!(
                "{result_handling}\n        \
                 Ok({return_wrap})"
            )
        }
        AsyncPattern::NapiNativeAsync => {
            let result_handling = if has_error {
                format!(
                    "let result = {core_call}.await\n            \
                     .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;"
                )
            } else {
                format!("let result = {core_call}.await;")
            };
            format!(
                "{result_handling}\n            \
                 Ok({return_wrap})"
            )
        }
        AsyncPattern::TokioBlockOn => {
            if has_error {
                if is_opaque {
                    format!(
                        "let rt = tokio::runtime::Runtime::new()?;\n        \
                         let result = rt.block_on(async {{ {core_call}.await.map_err(|e| e.into()) }})?;\n        \
                         {return_wrap}"
                    )
                } else {
                    format!(
                        "let rt = tokio::runtime::Runtime::new()?;\n        \
                         rt.block_on(async {{ {core_call}.await.map_err(|e| e.into()) }})"
                    )
                }
            } else if is_opaque {
                format!(
                    "let rt = tokio::runtime::Runtime::new()?;\n        \
                     let result = rt.block_on(async {{ {core_call}.await }});\n        \
                     {return_wrap}"
                )
            } else {
                format!(
                    "let rt = tokio::runtime::Runtime::new()?;\n        \
                     rt.block_on(async {{ {core_call}.await }})"
                )
            }
        }
        AsyncPattern::None => "todo!(\"async not supported by backend\")".to_string(),
    };
    if inner_clone_line.is_empty() {
        pattern_body
    } else {
        format!("{inner_clone_line}{pattern_body}")
    }
}

/// Build call argument expressions from parameters.
/// - Opaque Named types: unwrap Arc wrapper via `(*param.inner).clone()`
/// - Non-opaque Named types: `.into()` for From conversion
/// - String/Path/Bytes: `&param` since core functions typically take `&str`/`&Path`/`&[u8]`
fn gen_call_args(params: &[ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                // Opaque type: borrow through Arc to get &CoreType
                if p.optional {
                    format!("{}.as_ref().map(|v| &*v.inner)", p.name)
                } else {
                    format!("&*{}.inner", p.name)
                }
            }
            TypeRef::Named(_) => {
                if p.optional {
                    format!("{}.map(Into::into)", p.name)
                } else {
                    format!("{}.into()", p.name)
                }
            }
            // String → &str for core function calls
            TypeRef::String => format!("&{}", p.name),
            // Path → PathBuf for core function calls (core expects PathBuf, binding has String)
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build call argument expressions using pre-bound let bindings for non-opaque Named params.
/// Non-opaque Named params use `&{name}_core` references instead of `.into()`.
fn gen_call_args_with_let_bindings(params: &[ParamDef], opaque_types: &AHashSet<String>) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                if p.optional {
                    format!("{}.as_ref().map(|v| &*v.inner)", p.name)
                } else {
                    format!("&*{}.inner", p.name)
                }
            }
            TypeRef::Named(_) => {
                if p.optional {
                    format!("{}_core.as_ref()", p.name)
                } else {
                    format!("&{}_core", p.name)
                }
            }
            TypeRef::String => format!("&{}", p.name),
            TypeRef::Path => format!("std::path::PathBuf::from({})", p.name),
            TypeRef::Bytes => format!("&{}", p.name),
            _ => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate let bindings for non-opaque Named params, converting them to core types.
fn gen_named_let_bindings(params: &[ParamDef], opaque_types: &AHashSet<String>) -> String {
    let mut bindings = String::new();
    for p in params {
        if let TypeRef::Named(name) = &p.ty {
            if !opaque_types.contains(name.as_str()) {
                if p.optional {
                    write!(bindings, "let {}_core = {}.map(Into::into);\n    ", p.name, p.name).ok();
                } else {
                    write!(bindings, "let {}_core = {}.into();\n    ", p.name, p.name).ok();
                }
            }
        }
    }
    bindings
}

/// Check if params contain any non-opaque Named types that need let bindings.
fn has_named_params(params: &[ParamDef], opaque_types: &AHashSet<String>) -> bool {
    params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(name) if !opaque_types.contains(name.as_str())))
}

/// Generate a struct definition using the builder.
pub fn gen_struct(typ: &TypeDef, mapper: &dyn TypeMapper, cfg: &RustBindingConfig) -> String {
    let mut sb = StructBuilder::new(&typ.name);
    for attr in cfg.struct_attrs {
        sb.add_attr(attr);
    }
    for d in cfg.struct_derives {
        sb.add_derive(d);
    }
    for field in &typ.fields {
        let ty = if field.optional {
            mapper.optional(&mapper.map_type(&field.ty))
        } else {
            mapper.map_type(&field.ty)
        };
        let attrs: Vec<String> = cfg.field_attrs.iter().map(|a| a.to_string()).collect();
        sb.add_field(&field.name, &ty, attrs);
    }
    sb.build()
}

/// Generate an opaque wrapper struct with `inner: Arc<core::Type>`.
/// For trait types, uses `Arc<dyn Type + Send + Sync>`.
pub fn gen_opaque_struct(typ: &TypeDef, cfg: &RustBindingConfig) -> String {
    let mut out = String::with_capacity(512);
    if !cfg.struct_derives.is_empty() {
        writeln!(out, "#[derive(Clone)]").ok();
    }
    for attr in cfg.struct_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    writeln!(out, "pub struct {} {{", typ.name).ok();
    let core_path = typ.rust_path.replace('-', "_");
    if typ.is_trait {
        writeln!(out, "    inner: std::sync::Arc<dyn {core_path} + Send + Sync>,").ok();
    } else {
        writeln!(out, "    inner: std::sync::Arc<{core_path}>,").ok();
    }
    write!(out, "}}").ok();
    out
}

/// Generate an opaque wrapper struct with `inner: Arc<core::Type>` and a `Js` prefix.
pub fn gen_opaque_struct_prefixed(typ: &TypeDef, cfg: &RustBindingConfig, prefix: &str) -> String {
    let mut out = String::with_capacity(512);
    if !cfg.struct_derives.is_empty() {
        writeln!(out, "#[derive(Clone)]").ok();
    }
    for attr in cfg.struct_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    let core_path = typ.rust_path.replace('-', "_");
    writeln!(out, "pub struct {}{} {{", prefix, typ.name).ok();
    if typ.is_trait {
        writeln!(out, "    inner: std::sync::Arc<dyn {core_path} + Send + Sync>,").ok();
    } else {
        writeln!(out, "    inner: std::sync::Arc<{core_path}>,").ok();
    }
    write!(out, "}}").ok();
    out
}

/// Generate a full impl block for an opaque type, delegating methods to `self.inner`.
///
/// `opaque_types` is the set of type names that are opaque wrappers (use `Arc<inner>`).
/// This is needed so that return-type wrapping uses the correct pattern for cross-type returns.
pub fn gen_opaque_impl_block(
    typ: &TypeDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    opaque_types: &AHashSet<String>,
    adapter_bodies: &AdapterBodies,
) -> String {
    let (instance, statics) = partition_methods(&typ.methods);
    if instance.is_empty() && statics.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(2048);
    if let Some(block_attr) = cfg.method_block_attr {
        writeln!(out, "#[{block_attr}]").ok();
    }
    writeln!(out, "impl {} {{", typ.name).ok();

    // Instance methods — delegate to self.inner
    for m in &instance {
        out.push_str(&gen_method(m, mapper, cfg, typ, true, opaque_types, adapter_bodies));
        out.push_str("\n\n");
    }

    // Static methods
    for m in &statics {
        out.push_str(&gen_static_method(m, mapper, cfg, typ, adapter_bodies, opaque_types));
        out.push_str("\n\n");
    }

    let trimmed = out.trim_end();
    let mut result = trimmed.to_string();
    result.push_str("\n}");
    result
}

/// Generate a constructor method.
pub fn gen_constructor(typ: &TypeDef, mapper: &dyn TypeMapper, cfg: &RustBindingConfig) -> String {
    let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
    let (param_list, sig_defaults, assignments) = constructor_parts(&typ.fields, &map_fn);

    let mut out = String::with_capacity(512);
    if cfg.needs_signature {
        writeln!(
            out,
            "    {}{}{}",
            cfg.signature_prefix, sig_defaults, cfg.signature_suffix
        )
        .ok();
    }
    write!(
        out,
        "    {}\n    pub fn new({param_list}) -> Self {{\n        Self {{ {assignments} }}\n    }}",
        cfg.constructor_attr
    )
    .ok();
    out
}

/// Generate an instance method.
///
/// When `is_opaque` is true, generates delegation to `self.inner` via Arc clone
/// instead of converting self to core type.
///
/// `opaque_types` is the set of opaque type names, used for correct return wrapping.
pub fn gen_method(
    method: &MethodDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    typ: &TypeDef,
    is_opaque: bool,
    opaque_types: &AHashSet<String>,
    adapter_bodies: &AdapterBodies,
) -> String {
    let type_name = &typ.name;
    // Use the full rust_path (with hyphens replaced by underscores) for core type references
    let core_type_path = typ.rust_path.replace('-', "_");

    let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
    let params = function_params(&method.params, &map_fn);
    let return_type = mapper.map_type(&method.return_type);
    let ret = mapper.wrap_return(&return_type, method.error_type.is_some());

    let call_args = gen_call_args(&method.params, opaque_types);

    // Auto-delegate opaque methods: unwrap Arc for params, wrap Arc for returns.
    // Allows Named params/returns (opaque types use Arc unwrap/wrap, non-opaque use .into()).
    let opaque_can_delegate = is_opaque
        && !method.sanitized
        && !method.is_async
        && method.error_type.is_none()
        && method
            .params
            .iter()
            .all(|p| !p.sanitized && crate::shared::is_opaque_delegatable_type(&p.ty))
        && crate::shared::is_opaque_delegatable_type(&method.return_type);

    // Build the core call expression: opaque types delegate to self.inner directly,
    // non-opaque types convert self to core type first.
    let make_core_call = |method_name: &str| -> String {
        if is_opaque {
            format!("self.inner.{method_name}({call_args})")
        } else {
            format!("{core_type_path}::from(self.clone()).{method_name}({call_args})")
        }
    };

    // For async opaque methods, we clone the Arc before moving into the future.
    let make_async_core_call = |method_name: &str| -> String {
        if is_opaque {
            format!("inner.{method_name}({call_args})")
        } else {
            format!("{core_type_path}::from(self.clone()).{method_name}({call_args})")
        }
    };

    // Generate the body: convert self to core type, call method, convert result back
    //
    // For opaque types, wrap the return value appropriately:
    //   - Named(self) → Self { inner: Arc::new(result) }
    //   - Named(other) → OtherType::from(result)
    //   - primitives/String/Vec/Unit → pass through
    let async_result_wrap = if is_opaque {
        wrap_opaque_return("result", &method.return_type, type_name, opaque_types)
    } else {
        // For non-opaque types, only use From conversion if the return type is simple
        // enough. Named return types may not have a From impl.
        match &method.return_type {
            TypeRef::Named(_) | TypeRef::Json => {
                format!("todo!(\"convert return of {}.{}\")", type_name, method.name)
            }
            _ => "result".to_string(),
        }
    };

    let body = if !opaque_can_delegate {
        // Check if an adapter provides the body
        let adapter_key = format!("{}.{}", type_name, method.name);
        if let Some(adapter_body) = adapter_bodies.get(&adapter_key) {
            adapter_body.clone()
        } else {
            format!("todo!(\"wire up {}.{}\")", type_name, method.name)
        }
    } else if method.is_async {
        let inner_clone_line = if is_opaque {
            "let inner = self.inner.clone();\n        "
        } else {
            ""
        };
        let core_call_str = make_async_core_call(&method.name);
        gen_async_body(
            &core_call_str,
            cfg,
            method.error_type.is_some(),
            &async_result_wrap,
            is_opaque,
            inner_clone_line,
        )
    } else {
        let core_call = make_core_call(&method.name);
        if method.error_type.is_some() {
            if is_opaque {
                let wrap = wrap_opaque_return("result", &method.return_type, type_name, opaque_types);
                format!("let result = {core_call}.map_err(|e| e.into())?;\n        {wrap}")
            } else {
                format!("{core_call}.map_err(|e| e.into())")
            }
        } else if is_opaque {
            wrap_opaque_return(&core_call, &method.return_type, type_name, opaque_types)
        } else {
            core_call
        }
    };

    let needs_py = method.is_async && cfg.async_pattern == AsyncPattern::Pyo3FutureIntoPy;
    let self_param = match (needs_py, params.is_empty()) {
        (true, true) => "&self, py: Python<'_>",
        (true, false) => "&self, py: Python<'_>, ",
        (false, true) => "&self",
        (false, false) => "&self, ",
    };

    // Wrap long signature if necessary
    let (sig_start, sig_params, sig_end) = if self_param.len() + params.len() > 100 {
        let wrapped_params = method
            .params
            .iter()
            .map(|p| {
                let ty = if p.optional {
                    format!("Option<{}>", mapper.map_type(&p.ty))
                } else {
                    mapper.map_type(&p.ty)
                };
                format!("{}: {}", p.name, ty)
            })
            .collect::<Vec<_>>()
            .join(",\n        ");
        let py_param = if needs_py { "\n        py: Python<'_>," } else { "" };
        (
            format!("pub fn {}(\n        &self,{}\n        ", method.name, py_param),
            wrapped_params,
            "\n    ) -> ".to_string(),
        )
    } else {
        (
            format!("pub fn {}({}", method.name, self_param),
            params,
            ") -> ".to_string(),
        )
    };

    let mut out = String::with_capacity(1024);
    if cfg.needs_signature {
        let sig = function_sig_defaults(&method.params);
        writeln!(out, "    {}{}{}", cfg.signature_prefix, sig, cfg.signature_suffix).ok();
    }
    write!(
        out,
        "    {}{}{}{} {{\n        \
         {body}\n    }}",
        sig_start, sig_params, sig_end, ret,
    )
    .ok();
    out
}

/// Generate a static method.
pub fn gen_static_method(
    method: &MethodDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    typ: &TypeDef,
    adapter_bodies: &AdapterBodies,
    opaque_types: &AHashSet<String>,
) -> String {
    let type_name = &typ.name;
    // Use the full rust_path (with hyphens replaced by underscores) for core type references
    let core_type_path = typ.rust_path.replace('-', "_");
    let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
    let params = function_params(&method.params, &map_fn);
    let return_type = mapper.map_type(&method.return_type);
    let ret = mapper.wrap_return(&return_type, method.error_type.is_some());

    let call_args = gen_call_args(&method.params, opaque_types);

    let can_delegate = crate::shared::can_auto_delegate(method);

    let body = if !can_delegate {
        // Check if an adapter provides the body
        let adapter_key = format!("{}.{}", type_name, method.name);
        if let Some(adapter_body) = adapter_bodies.get(&adapter_key) {
            adapter_body.clone()
        } else {
            format!("todo!(\"wire up {type_name}::{}\")", method.name)
        }
    } else if method.is_async {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        let return_wrap = format!("{return_type}::from(result)");
        gen_async_body(&core_call, cfg, method.error_type.is_some(), &return_wrap, false, "")
    } else {
        let core_call = format!("{core_type_path}::{}({call_args})", method.name);
        if method.error_type.is_some() {
            // Backend-specific error conversion
            let err_conv = match cfg.async_pattern {
                AsyncPattern::Pyo3FutureIntoPy => {
                    ".map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))"
                }
                AsyncPattern::NapiNativeAsync => {
                    ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))"
                }
                AsyncPattern::WasmNativeAsync => ".map_err(|e| JsValue::from_str(&e.to_string()))",
                _ => ".map_err(|e| e.to_string())",
            };
            // Wrap the Ok value if the return type needs conversion (e.g. PathBuf→String)
            let wrapped = wrap_opaque_return("val", &method.return_type, type_name, opaque_types);
            if wrapped == "val" {
                format!("{core_call}{err_conv}")
            } else {
                format!("{core_call}.map(|val| {wrapped}){err_conv}")
            }
        } else {
            // Wrap return value for non-error case too (e.g. PathBuf→String)
            wrap_opaque_return(&core_call, &method.return_type, type_name, opaque_types)
        }
    };

    // Wrap long signature if necessary
    let (sig_start, sig_params, sig_end) = if params.len() > 100 {
        let wrapped_params = method
            .params
            .iter()
            .map(|p| {
                let ty = if p.optional {
                    format!("Option<{}>", mapper.map_type(&p.ty))
                } else {
                    mapper.map_type(&p.ty)
                };
                format!("{}: {}", p.name, ty)
            })
            .collect::<Vec<_>>()
            .join(",\n        ");
        // For async PyO3, add py parameter
        if method.is_async && cfg.async_pattern == AsyncPattern::Pyo3FutureIntoPy {
            (
                format!("pub fn {}(py: Python<'_>,\n        ", method.name),
                wrapped_params,
                "\n    ) -> ".to_string(),
            )
        } else {
            (
                format!("pub fn {}(\n        ", method.name),
                wrapped_params,
                "\n    ) -> ".to_string(),
            )
        }
    } else {
        if method.is_async && cfg.async_pattern == AsyncPattern::Pyo3FutureIntoPy {
            (
                format!("pub fn {}(py: Python<'_>, ", method.name),
                params,
                ") -> ".to_string(),
            )
        } else {
            (format!("pub fn {}(", method.name), params, ") -> ".to_string())
        }
    };

    let mut out = String::with_capacity(1024);
    if let Some(attr) = cfg.static_attr {
        writeln!(out, "    #[{attr}]").ok();
    }
    if cfg.needs_signature {
        let sig = function_sig_defaults(&method.params);
        writeln!(out, "    {}{}{}", cfg.signature_prefix, sig, cfg.signature_suffix).ok();
    }
    write!(
        out,
        "    {}{}{}{} {{\n        \
         {body}\n    }}",
        sig_start, sig_params, sig_end, ret,
    )
    .ok();
    out
}

/// Generate an enum.
pub fn gen_enum(enum_def: &EnumDef, cfg: &RustBindingConfig) -> String {
    // All enums are generated as unit-variant-only in the binding layer.
    // Data variants are flattened to unit variants; the From/Into conversions
    // handle the lossy mapping (discarding / providing defaults for field data).
    let mut out = String::with_capacity(512);
    if !cfg.enum_derives.is_empty() {
        writeln!(out, "#[derive({})]", cfg.enum_derives.join(", ")).ok();
    }
    for attr in cfg.enum_attrs {
        writeln!(out, "#[{attr}]").ok();
    }
    writeln!(out, "pub enum {} {{", enum_def.name).ok();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "    {} = {idx},", variant.name).ok();
    }
    write!(out, "}}").ok();
    out
}

/// Generate a free function.
pub fn gen_function(
    func: &FunctionDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    adapter_bodies: &AdapterBodies,
    opaque_types: &AHashSet<String>,
) -> String {
    let map_fn = |ty: &skif_core::ir::TypeRef| mapper.map_type(ty);
    let params = function_params(&func.params, &map_fn);
    let return_type = mapper.map_type(&func.return_type);
    let ret = mapper.wrap_return(&return_type, func.error_type.is_some());

    // Use let-binding pattern for non-opaque Named params so core fns can take &CoreType
    let use_let_bindings = has_named_params(&func.params, opaque_types);
    let call_args = if use_let_bindings {
        gen_call_args_with_let_bindings(&func.params, opaque_types)
    } else {
        gen_call_args(&func.params, opaque_types)
    };
    let let_bindings = if use_let_bindings {
        gen_named_let_bindings(&func.params, opaque_types)
    } else {
        String::new()
    };
    let core_import = cfg.core_import;

    // Use the function's rust_path for correct module path resolution
    let core_fn_path = {
        let path = func.rust_path.replace('-', "_");
        if path.starts_with(core_import) {
            path
        } else {
            format!("{core_import}::{}", func.name)
        }
    };

    let can_delegate = crate::shared::can_auto_delegate_function(func);

    // Generate the body based on async pattern
    let body = if !can_delegate {
        // Check if an adapter provides the body
        if let Some(adapter_body) = adapter_bodies.get(&func.name) {
            adapter_body.clone()
        } else {
            format!("todo!(\"wire up {}\")", func.name)
        }
    } else if func.is_async {
        let core_call = format!("{core_fn_path}({call_args})");
        let return_wrap = format!("{return_type}::from(result)");
        let async_body = gen_async_body(&core_call, cfg, func.error_type.is_some(), &return_wrap, false, "");
        format!("{let_bindings}{async_body}")
    } else {
        let core_call = format!("{core_fn_path}({call_args})");

        // Determine return wrapping strategy
        let wrap_return = |expr: &str| -> String {
            match &func.return_type {
                // Opaque type return: wrap in Arc
                TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                    format!("{name} {{ inner: std::sync::Arc::new({expr}) }}")
                }
                // Non-opaque Named: use .into() if From impl exists
                TypeRef::Named(_name) => {
                    // Check if this type has a From impl (is convertible)
                    // For now, attempt .into() — compilation will catch missing impls
                    format!("{expr}.into()")
                }
                // String/Bytes: .into() handles &str→String etc.
                TypeRef::String | TypeRef::Bytes => format!("{expr}.into()"),
                // Path: PathBuf→String needs to_string_lossy
                TypeRef::Path => format!("{expr}.to_string_lossy().to_string()"),
                // Optional with opaque inner
                TypeRef::Optional(inner) => match inner.as_ref() {
                    TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                        format!("{expr}.map(|v| {name} {{ inner: std::sync::Arc::new(v) }})")
                    }
                    TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => {
                        format!("{expr}.map(Into::into)")
                    }
                    _ => expr.to_string(),
                },
                // Vec<Named>: map each element through Into
                TypeRef::Vec(inner) => match inner.as_ref() {
                    TypeRef::Named(name) if opaque_types.contains(name.as_str()) => {
                        format!("{expr}.into_iter().map(|v| {name} {{ inner: std::sync::Arc::new(v) }}).collect()")
                    }
                    TypeRef::Named(_) | TypeRef::String | TypeRef::Bytes | TypeRef::Path => {
                        format!("{expr}.into_iter().map(Into::into).collect()")
                    }
                    _ => expr.to_string(),
                },
                _ => expr.to_string(),
            }
        };

        if func.error_type.is_some() {
            // Backend-specific error conversion
            let err_conv = match cfg.async_pattern {
                AsyncPattern::Pyo3FutureIntoPy => {
                    ".map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))"
                }
                AsyncPattern::NapiNativeAsync => {
                    ".map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))"
                }
                AsyncPattern::WasmNativeAsync => ".map_err(|e| JsValue::from_str(&e.to_string()))",
                _ => ".map_err(|e| e.to_string())",
            };
            let wrapped = wrap_return("val");
            if wrapped == "val" {
                format!("{core_call}{err_conv}")
            } else {
                format!("{core_call}.map(|val| {wrapped}){err_conv}")
            }
        } else {
            wrap_return(&core_call)
        }
    };

    // Prepend let bindings for non-opaque Named params (sync non-adapter case)
    let body = if !let_bindings.is_empty() && can_delegate && !func.is_async {
        format!("{let_bindings}{body}")
    } else {
        body
    };

    // Wrap long signature if necessary
    let async_kw = if func.is_async { "async " } else { "" };
    let (func_sig, _params_formatted) = if params.len() > 100 {
        let wrapped_params = func
            .params
            .iter()
            .map(|p| {
                let ty = if p.optional {
                    format!("Option<{}>", mapper.map_type(&p.ty))
                } else {
                    mapper.map_type(&p.ty)
                };
                format!("{}: {}", p.name, ty)
            })
            .collect::<Vec<_>>()
            .join(",\n    ");

        // For async PyO3, we need special signature handling
        if func.is_async && cfg.async_pattern == AsyncPattern::Pyo3FutureIntoPy {
            (
                format!(
                    "pub fn {}(py: Python<'_>,\n    {}\n) -> {ret}",
                    func.name,
                    wrapped_params,
                    ret = ret
                ),
                "",
            )
        } else {
            (
                format!(
                    "pub {async_kw}fn {}(\n    {}\n) -> {ret}",
                    func.name,
                    wrapped_params,
                    ret = ret
                ),
                "",
            )
        }
    } else {
        if func.is_async && cfg.async_pattern == AsyncPattern::Pyo3FutureIntoPy {
            (format!("pub fn {}(py: Python<'_>, {params}) -> {ret}", func.name), "")
        } else {
            (format!("pub {async_kw}fn {}({params}) -> {ret}", func.name), "")
        }
    };

    let mut out = String::with_capacity(1024);
    let attr_inner = cfg
        .function_attr
        .trim_start_matches('#')
        .trim_start_matches('[')
        .trim_end_matches(']');
    writeln!(out, "#[{attr_inner}]").ok();
    if cfg.needs_signature {
        let sig = function_sig_defaults(&func.params);
        writeln!(out, "{}{}{}", cfg.signature_prefix, sig, cfg.signature_suffix).ok();
    }
    write!(out, "{} {{\n    {body}\n}}", func_sig,).ok();
    out
}

/// Generate a full methods impl block.
pub fn gen_impl_block(
    typ: &TypeDef,
    mapper: &dyn TypeMapper,
    cfg: &RustBindingConfig,
    adapter_bodies: &AdapterBodies,
) -> String {
    let (instance, statics) = partition_methods(&typ.methods);
    if instance.is_empty() && statics.is_empty() && typ.fields.is_empty() {
        return String::new();
    }

    let empty_opaque = AHashSet::new();
    let mut out = String::with_capacity(2048);
    if let Some(block_attr) = cfg.method_block_attr {
        writeln!(out, "#[{block_attr}]").ok();
    }
    writeln!(out, "impl {} {{", typ.name).ok();

    // Constructor
    if !typ.fields.is_empty() {
        out.push_str(&gen_constructor(typ, mapper, cfg));
        out.push_str("\n\n");
    }

    // Instance methods
    for m in &instance {
        out.push_str(&gen_method(m, mapper, cfg, typ, false, &empty_opaque, adapter_bodies));
        out.push_str("\n\n");
    }

    // Static methods
    for m in &statics {
        out.push_str(&gen_static_method(m, mapper, cfg, typ, adapter_bodies, &empty_opaque));
        out.push_str("\n\n");
    }

    // Trim trailing newlines inside impl block
    let trimmed = out.trim_end();
    let mut result = trimmed.to_string();
    result.push_str("\n}");
    result
}
