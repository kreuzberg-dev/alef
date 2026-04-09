use crate::type_map::{c_param_type, c_return_type, is_void_return};
use alef_codegen::builder::RustFileBuilder;
use alef_codegen::generators;
use alef_core::backend::{Backend, BuildConfig, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language, resolve_output_dir};
use alef_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, ParamDef, ReceiverKind, TypeDef, TypeRef};
use heck::{ToPascalCase, ToSnakeCase};
use std::fmt::Write;
use std::path::PathBuf;

pub struct FfiBackend;

impl FfiBackend {}

impl Backend for FfiBackend {
    fn name(&self) -> &str {
        "ffi"
    }

    fn language(&self) -> Language {
        Language::Ffi
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: false,
            supports_classes: true,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            ..Capabilities::default()
        }
    }

    fn generate_bindings(&self, api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        let prefix = config.ffi_prefix();
        let header_name = config.ffi_header_name();

        let output_dir = resolve_output_dir(
            config.output.ffi.as_ref(),
            &config.crate_config.name,
            "crates/{name}-ffi/src/",
        );

        let parent_dir = PathBuf::from(&output_dir)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let files = vec![
            GeneratedFile {
                path: PathBuf::from(&output_dir).join("lib.rs"),
                content: gen_lib_rs(api, &prefix, config),
                generated_header: false,
            },
            GeneratedFile {
                path: parent_dir.join("cbindgen.toml"),
                content: gen_cbindgen_toml(&prefix, api),
                generated_header: false,
            },
            GeneratedFile {
                path: parent_dir.join("build.rs"),
                content: gen_build_rs(&header_name),
                generated_header: false,
            },
        ];

        Ok(files)
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "cargo",
            crate_suffix: "-ffi",
            depends_on_ffi: false,
            post_build: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// lib.rs generation
// ---------------------------------------------------------------------------

fn gen_lib_rs(api: &ApiSurface, prefix: &str, config: &AlefConfig) -> String {
    use std::collections::HashSet;

    let mut builder = RustFileBuilder::new().with_generated_header();

    // Imports
    builder.add_import("std::ffi::{c_char, CStr, CString}");
    builder.add_import("std::cell::RefCell");
    let core_import = config.core_import();

    // Import traits needed for trait method dispatch
    for trait_path in generators::collect_trait_imports(api) {
        builder.add_import(&trait_path);
    }

    // Collect opaque type names for skipping serde/clone operations
    let opaque_types: HashSet<String> = api
        .types
        .iter()
        .filter(|t| t.is_opaque)
        .map(|t| t.name.clone())
        .collect();

    // Only import serde_json when types need from_json deserialization or
    // when Json/Vec/Map fields/returns require serialization
    let has_from_json_types = api
        .types
        .iter()
        .any(|t| !t.is_opaque && !t.fields.iter().any(|f| f.sanitized));
    let has_serde_fields = api.types.iter().any(|t| {
        t.fields.iter().any(|f| {
            matches!(f.ty, TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _))
                || matches!(&f.ty, TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _)))
        })
    });
    let has_serde_returns = api.types.iter().any(|t| {
        t.methods.iter().any(|m| {
            matches!(m.return_type, TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _))
                || matches!(&m.return_type, TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _)))
        })
    }) || api.functions.iter().any(|f| {
        matches!(f.return_type, TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _))
            || matches!(&f.return_type, TypeRef::Optional(inner) if matches!(inner.as_ref(), TypeRef::Json | TypeRef::Vec(_) | TypeRef::Map(_, _)))
    });
    if has_from_json_types || has_serde_fields || has_serde_returns {
        builder.add_import("serde_json");
    }

    // Custom module declarations
    let custom_mods = config.custom_modules.for_language(Language::Ffi);
    for module in custom_mods {
        builder.add_item(&format!("pub mod {module};"));
    }

    // Thread-local last_error infrastructure
    builder.add_item(&gen_last_error(prefix));

    // free_string helper
    builder.add_item(&gen_free_string(prefix));

    // version helper
    builder.add_item(&gen_version(prefix));

    // Struct opaque-handle functions (from_json + free + field accessors + methods)
    for typ in &api.types {
        // Opaque types don't implement serde Deserialize, so skip from_json.
        // Types with sanitized fields may not implement Deserialize either
        // (the core type has non-serializable field types).
        let has_sanitized = typ.fields.iter().any(|f| f.sanitized);
        if !typ.is_opaque && !has_sanitized {
            builder.add_item(&gen_type_from_json(typ, prefix, &core_import));
        }
        builder.add_item(&gen_type_free(typ, prefix, &core_import));

        // Field accessors — skip sanitized fields (binding type differs from core)
        for field in &typ.fields {
            if !field.sanitized {
                builder.add_item(&gen_field_accessor(typ, field, prefix, &core_import));
            }
        }

        // Method wrappers
        for method in &typ.methods {
            builder.add_item(&gen_method_wrapper(typ, method, prefix, &core_import, &opaque_types));
        }
    }

    // Enum functions (from_i32 + to_i32) — only for simple unit-variant enums
    for enum_def in &api.enums {
        if alef_codegen::conversions::can_generate_enum_conversion(enum_def) {
            builder.add_item(&gen_enum_from_i32(enum_def, prefix, &core_import));
            builder.add_item(&gen_enum_to_i32(enum_def, prefix, &core_import));
        }
    }

    // Free functions (skip async — FFI can't express async; use sync wrappers instead)
    for func in &api.functions {
        if func.is_async {
            continue;
        }
        builder.add_item(&gen_free_function(func, prefix, &core_import, &opaque_types));
    }

    // Build adapter body map (consumed by generators via body substitution)
    let _adapter_bodies = alef_adapters::build_adapter_bodies(config, Language::Ffi).unwrap_or_default();

    // Visitor/callback FFI support — generated when `[ffi] visitor_callbacks = true`.
    // Note: the generated code uses std::rc::Rc fully qualified, so no extra import needed.
    if config.ffi.as_ref().is_some_and(|f| f.visitor_callbacks) {
        builder.add_item(&crate::gen_visitor::gen_visitor_bindings(prefix, &core_import));
    }

    builder.build()
}

// ---------------------------------------------------------------------------
// last_error pattern
// ---------------------------------------------------------------------------

fn gen_last_error(prefix: &str) -> String {
    format!(
        r#"thread_local! {{
    static LAST_ERROR_CODE: RefCell<i32> = const {{ RefCell::new(0) }};
    static LAST_ERROR_CONTEXT: RefCell<Option<CString>> = const {{ RefCell::new(None) }};
}}

fn set_last_error(code: i32, message: &str) {{
    LAST_ERROR_CODE.with_borrow_mut(|c| *c = code);
    LAST_ERROR_CONTEXT.with_borrow_mut(|c| *c = CString::new(message).ok());
}}

fn clear_last_error() {{
    LAST_ERROR_CODE.with_borrow_mut(|c| *c = 0);
    LAST_ERROR_CONTEXT.with_borrow_mut(|c| *c = None);
}}

/// Return the last error code (0 means no error).
/// # Safety
/// Caller must ensure all pointer arguments are valid or null.
/// Returned pointers must be freed with the appropriate free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn {prefix}_last_error_code() -> i32 {{
    LAST_ERROR_CODE.with_borrow(|c| *c)
}}

/// Return the last error message. The pointer is valid until the next FFI call on this thread.
/// # Safety
/// Caller must ensure all pointer arguments are valid or null.
/// Returned pointers must be freed with the appropriate free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn {prefix}_last_error_context() -> *const c_char {{
    LAST_ERROR_CONTEXT.with_borrow(|ctx| {{
        ctx.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    }})
}}"#,
        prefix = prefix
    )
}

// ---------------------------------------------------------------------------
// free_string
// ---------------------------------------------------------------------------

fn gen_free_string(prefix: &str) -> String {
    format!(
        r#"/// Free a string previously returned by this library.
/// # Safety
/// Pointer must have been returned by this library, or be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn {prefix}_free_string(ptr: *mut c_char) {{
    if !ptr.is_null() {{
        unsafe {{ drop(CString::from_raw(ptr)); }}
    }}
}}"#,
        prefix = prefix
    )
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

fn gen_version(prefix: &str) -> String {
    format!(
        r#"/// Return the library version string. The pointer is static and must NOT be freed.
/// # Safety
/// Caller must ensure all pointer arguments are valid or null.
/// Returned pointers must be freed with the appropriate free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn {prefix}_version() -> *const c_char {{
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}}"#,
        prefix = prefix
    )
}

// ---------------------------------------------------------------------------
// Type: from_json + free
// ---------------------------------------------------------------------------

fn gen_type_from_json(typ: &TypeDef, prefix: &str, core_import: &str) -> String {
    let type_snake = typ.name.to_snake_case();
    let type_name = &typ.name;
    let qualified = format!("{core_import}::{type_name}");
    let mut out = String::with_capacity(2048);

    writeln!(
        out,
        "/// Create a `{type_name}` from a JSON string. Returns null on failure."
    )
    .ok();
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// JSON string must be valid UTF-8 and null-terminated.").ok();
    writeln!(
        out,
        "/// Returned handle must be freed with `{prefix}_{type_snake}_free`."
    )
    .ok();
    writeln!(out, "#[unsafe(no_mangle)]").ok();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{type_snake}_from_json(json: *const c_char) -> *mut {qualified} {{"
    )
    .ok();
    writeln!(out, "    clear_last_error();").ok();
    writeln!(out, "    if json.is_null() {{").ok();
    writeln!(
        out,
        "        set_last_error(1, \"Null pointer passed for JSON string\");"
    )
    .ok();
    writeln!(out, "        return std::ptr::null_mut();").ok();
    writeln!(out, "    }}").ok();
    writeln!(
        out,
        "    let c_str = match unsafe {{ CStr::from_ptr(json) }}.to_str() {{"
    )
    .ok();
    writeln!(out, "        Ok(s) => s,").ok();
    writeln!(out, "        Err(_) => {{").ok();
    writeln!(out, "            set_last_error(1, \"Invalid UTF-8 in JSON string\");").ok();
    writeln!(out, "            return std::ptr::null_mut();").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }};").ok();
    writeln!(out, "    match serde_json::from_str::<{qualified}>(c_str) {{").ok();
    writeln!(out, "        Ok(val) => Box::into_raw(Box::new(val)),").ok();
    writeln!(out, "        Err(e) => {{").ok();
    writeln!(out, "            set_last_error(2, &e.to_string());").ok();
    writeln!(out, "            std::ptr::null_mut()").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();

    out
}

fn gen_type_free(typ: &TypeDef, prefix: &str, core_import: &str) -> String {
    let type_snake = typ.name.to_snake_case();
    let type_name = &typ.name;
    let qualified = format!("{core_import}::{type_name}");
    let mut out = String::with_capacity(2048);

    writeln!(out, "/// Free a `{type_name}` handle.").ok();
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// Pointer must have been returned by this library, or be null.").ok();
    writeln!(out, "#[unsafe(no_mangle)]").ok();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{type_snake}_free(ptr: *mut {qualified}) {{"
    )
    .ok();
    writeln!(out, "    if !ptr.is_null() {{").ok();
    writeln!(out, "        unsafe {{ drop(Box::from_raw(ptr)); }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();

    out
}

// ---------------------------------------------------------------------------
// Field accessors
// ---------------------------------------------------------------------------

fn gen_field_accessor(typ: &TypeDef, field: &FieldDef, prefix: &str, core_import: &str) -> String {
    let type_snake = typ.name.to_snake_case();
    let type_name = &typ.name;
    let qualified = format!("{core_import}::{type_name}");
    let field_name = &field.name;

    let effective_ty = if field.optional {
        TypeRef::Optional(Box::new(field.ty.clone()))
    } else {
        field.ty.clone()
    };

    // When the field has a specific type_rust_path, use it for Named types to avoid
    // ambiguity when multiple types share the same short name.
    let field_core_import = if let Some(ref rust_path) = field.type_rust_path {
        // type_rust_path is e.g. "types::extraction::OutputFormat" — we need the module path
        // prefix without the type name itself.
        if let Some(pos) = rust_path.rfind("::") {
            // e.g. "types::extraction" from "types::extraction::OutputFormat"
            // Prefix with core_import to get the fully qualified module
            format!("{core_import}::{}", &rust_path[..pos])
        } else {
            core_import.to_string()
        }
    } else {
        core_import.to_string()
    };

    let mut ret_type = c_return_type(&effective_ty, &field_core_import).into_owned();
    // Replace "Self" with the actual qualified type name in FFI signatures
    if ret_type.contains("Self") {
        ret_type = ret_type.replace("Self", &qualified);
    }
    let mut out = String::with_capacity(2048);

    writeln!(out, "/// Get the `{field_name}` field from a `{type_name}`.").ok();
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// Pointer must be a valid handle returned by this library.").ok();
    writeln!(out, "#[unsafe(no_mangle)]").ok();

    // Determine if we need an extra out-param for byte-length
    let needs_len_out = matches!(field.ty, TypeRef::Bytes) && !field.optional;

    // Simple field accessors (non-optional primitives) can be const
    let is_const = !field.optional && matches!(field.ty, TypeRef::Primitive(_));
    let const_kw = if is_const { "const " } else { "" };

    if needs_len_out {
        writeln!(
            out,
            "pub unsafe extern \"C\" fn {prefix}_{type_snake}_{field_name}(ptr: *const {qualified}, out_len: *mut usize) -> {ret_type} {{"
        )
        .ok();
    } else {
        writeln!(
            out,
            "pub {const_kw}unsafe extern \"C\" fn {prefix}_{type_snake}_{field_name}(ptr: *const {qualified}) -> {ret_type} {{"
        )
        .ok();
    }

    // Null-check on ptr
    writeln!(out, "    if ptr.is_null() {{").ok();
    writeln!(out, "        return {};", null_return_value(&effective_ty)).ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "    let obj = unsafe {{ &*ptr }};").ok();

    // Generate the accessor body based on field type
    write!(out, "{}", gen_field_access_body(field, needs_len_out)).ok();

    write!(out, "}}").ok();
    out
}

/// Generate the body of a field accessor that reads from `obj.{field_name}`.
fn gen_field_access_body(field: &FieldDef, needs_len_out: bool) -> String {
    let field_name = &field.name;
    let mut out = String::with_capacity(2048);

    if field.optional {
        // Wrap in match on Option — val is a reference from &Option<T> destructure
        // When is_boxed: val is &Box<T>, so deref twice (**val) to get &T
        let val_expr = if matches!(field.ty, TypeRef::Primitive(_)) {
            "*val" // dereference for Copy types
        } else if field.is_boxed {
            "(**val)" // deref &Box<T> → &T
        } else {
            "val"
        };
        writeln!(out, "    match &obj.{field_name} {{").ok();
        writeln!(out, "        Some(val) => {{").ok();
        write!(out, "{}", gen_value_to_c(val_expr, &field.ty, "            ")).ok();
        writeln!(out, "        }}").ok();
        writeln!(
            out,
            "        None => {},",
            null_return_value(&TypeRef::Optional(Box::new(field.ty.clone())))
        )
        .ok();
        writeln!(out, "    }}").ok();
    } else if needs_len_out {
        // Bytes with length out-param
        writeln!(out, "    let data = &obj.{field_name};").ok();
        writeln!(out, "    if !out_len.is_null() {{").ok();
        writeln!(out, "        unsafe {{ *out_len = data.len(); }}").ok();
        writeln!(out, "    }}").ok();
        writeln!(out, "    data.as_ptr() as *mut u8").ok();
    } else {
        // When is_boxed: obj.field_name is Box<T>, deref to get T before cloning
        let access_expr = if field.is_boxed {
            format!("(*obj.{field_name})")
        } else {
            format!("obj.{field_name}")
        };
        write!(out, "{}", gen_value_to_c(&access_expr, &field.ty, "    ")).ok();
    }

    out
}

/// Generate code to convert a Rust value reference to a C return value.
/// `expr` is the Rust expression to read from (must be borrowable).
fn gen_value_to_c(expr: &str, ty: &TypeRef, indent: &str) -> String {
    let mut out = String::with_capacity(2048);
    match ty {
        TypeRef::Primitive(p) => {
            // Bool needs cast to i32 for C ABI; other primitives may need deref if from Option
            if matches!(p, alef_core::ir::PrimitiveType::Bool) {
                writeln!(out, "{indent}{expr} as i32").ok();
            } else {
                writeln!(out, "{indent}{expr}").ok();
            }
        }
        TypeRef::String | TypeRef::Char => {
            writeln!(out, "{indent}match CString::new({expr}.to_string()) {{").ok();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Path => {
            writeln!(
                out,
                "{indent}match CString::new({expr}.to_string_lossy().to_string()) {{"
            )
            .ok();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Json => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").ok();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").ok();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}    }},").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Named(_) => {
            writeln!(out, "{indent}Box::into_raw(Box::new({expr}.clone()))").ok();
        }
        TypeRef::Vec(_) | TypeRef::Map(_, _) => {
            // Serialize as JSON
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").ok();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").ok();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}    }},").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Bytes => {
            // Return pointer; caller must also get length. Cast to *mut u8 to match FFI signature.
            writeln!(out, "{indent}{expr}.as_ptr() as *mut u8").ok();
        }
        TypeRef::Duration => {
            writeln!(out, "{indent}{expr}.as_secs()").ok();
        }
        TypeRef::Unit => {
            // nothing to return
        }
        TypeRef::Optional(inner) => {
            writeln!(out, "{indent}match &{expr} {{").ok();
            writeln!(out, "{indent}    Some(val) => {{").ok();
            write!(out, "{}", gen_value_to_c("val", inner, &format!("{indent}        "))).ok();
            writeln!(out, "{indent}    }}").ok();
            writeln!(
                out,
                "{indent}    None => {},",
                null_return_value(&TypeRef::Optional(inner.clone()))
            )
            .ok();
            writeln!(out, "{indent}}}").ok();
        }
    }
    out
}

/// Generate a type-appropriate unimplemented body for FFI (no todo!()).
/// Uses set_last_error + null/zero return instead of panicking.
fn gen_ffi_unimplemented_body(return_type: &TypeRef, fn_name: &str, has_error: bool) -> String {
    let err_msg = format!("Not implemented: {fn_name}");
    if has_error && is_void_return(return_type) {
        // Fallible + void: return error code
        format!("    set_last_error(99, \"{err_msg}\");\n    -1")
    } else if is_void_return(return_type) {
        // Infallible void: just set error context and return
        format!("    set_last_error(99, \"{err_msg}\");")
    } else {
        // Non-void: set error and return null/zero
        let ret = null_return_value(return_type);
        format!("    set_last_error(99, \"{err_msg}\");\n    {ret}")
    }
}

/// Return the null/zero value for a given type in return position.
fn null_return_value(ty: &TypeRef) -> &'static str {
    use alef_core::ir::PrimitiveType;
    match ty {
        TypeRef::Primitive(p) => match p {
            PrimitiveType::F32 | PrimitiveType::F64 => "0.0",
            _ => "0",
        },
        TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => "std::ptr::null_mut()",
        TypeRef::Bytes => "std::ptr::null_mut()",
        TypeRef::Named(_) => "std::ptr::null_mut()",
        TypeRef::Vec(_) | TypeRef::Map(_, _) => "std::ptr::null_mut()",
        TypeRef::Duration => "0",
        TypeRef::Optional(inner) => match inner.as_ref() {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::F32 | PrimitiveType::F64 => "0.0",
                _ => "0",
            },
            _ => "std::ptr::null_mut()",
        },
        TypeRef::Unit => "()",
    }
}

// ---------------------------------------------------------------------------
// Method wrappers
// ---------------------------------------------------------------------------

fn gen_method_wrapper(
    typ: &TypeDef,
    method: &MethodDef,
    prefix: &str,
    core_import: &str,
    opaque_types: &std::collections::HashSet<String>,
) -> String {
    let type_snake = typ.name.to_snake_case();
    let type_name = &typ.name;
    let method_name = &method.name;
    let fn_name = format!("{prefix}_{type_snake}_{method_name}");

    let mut out = String::with_capacity(2048);

    if !method.doc.is_empty() {
        for line in method.doc.lines() {
            writeln!(out, "/// {}", line).ok();
        }
    }
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").ok();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .ok();
    // Count total FFI params: this + params + extra _len for Bytes params
    let ffi_param_count = (if method.is_static { 0 } else { 1 })
        + method.params.len()
        + method.params.iter().filter(|p| matches!(p.ty, TypeRef::Bytes)).count();
    if ffi_param_count > 7 {
        writeln!(out, "#[allow(clippy::too_many_arguments)]").ok();
    }
    writeln!(out, "#[unsafe(no_mangle)]").ok();

    let qualified = format!("{core_import}::{type_name}");

    // Return type
    let has_error = method.error_type.is_some();
    let mut ret_type = if has_error && is_void_return(&method.return_type) {
        "i32".to_string() // 0 = success, nonzero = error
    } else if has_error {
        // Fallible + non-void: return nullable pointer
        match &method.return_type {
            TypeRef::Primitive(_) => c_return_type(&method.return_type, core_import).into_owned(),
            _ => c_return_type(&method.return_type, core_import).into_owned(),
        }
    } else {
        c_return_type(&method.return_type, core_import).into_owned()
    };

    // Replace "Self" with the actual qualified type name in FFI signatures
    if ret_type.contains("Self") {
        ret_type = ret_type.replace("Self", &qualified);
    }

    // Check if this method will be unimplemented before building params
    let has_opaque_param = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if opaque_types.contains(n)));
    let has_opaque_return = has_opaque_type_in_return(&method.return_type, opaque_types);
    let will_be_unimplemented = method.sanitized || has_opaque_param || has_opaque_return;

    // Build parameter list — prefix with _ if unimplemented
    let mut params = Vec::new();
    if !method.is_static {
        let receiver_ty = match method.receiver.as_ref().unwrap_or(&ReceiverKind::Ref) {
            ReceiverKind::Ref => format!("*const {qualified}"),
            ReceiverKind::RefMut | ReceiverKind::Owned => format!("*mut {qualified}"),
        };
        let param_name = if will_be_unimplemented { "_this" } else { "this" };
        params.push(format!("    {param_name}: {receiver_ty}"));
    }
    for p in &method.params {
        let param_name = if will_be_unimplemented {
            format!("_{}", p.name)
        } else {
            p.name.clone()
        };
        params.push(format!("    {}: {}", param_name, c_param_type(&p.ty, core_import)));
        // Bytes parameters need a separate length parameter
        if matches!(p.ty, TypeRef::Bytes) {
            let len_param_name = if will_be_unimplemented {
                format!("_{}_len", p.name)
            } else {
                format!("{}_len", p.name)
            };
            params.push(format!("    {}: usize", len_param_name));
        }
    }

    if is_void_return(&method.return_type) && !has_error {
        writeln!(out, "pub unsafe extern \"C\" fn {fn_name}(").ok();
        writeln!(out, "{}", params.join(",\n")).ok();
        writeln!(out, ") {{").ok();
    } else {
        writeln!(out, "pub unsafe extern \"C\" fn {fn_name}(").ok();
        writeln!(out, "{}", params.join(",\n")).ok();
        writeln!(out, ") -> {ret_type} {{").ok();
    }

    writeln!(out, "    clear_last_error();").ok();

    // If method signature was sanitized, generate unimplemented body
    if will_be_unimplemented {
        writeln!(
            out,
            "{}",
            gen_ffi_unimplemented_body(&method.return_type, &format!("{type_name}::{method_name}"), has_error)
        )
        .ok();
        write!(out, "}}").ok();
        return out;
    }

    // Null-check self
    if !method.is_static {
        writeln!(out, "    if this.is_null() {{").ok();
        writeln!(out, "        set_last_error(1, \"Null pointer passed for self\");").ok();
        let fail_ret = if has_error && is_void_return(&method.return_type) {
            "return -1;".to_string()
        } else if is_void_return(&method.return_type) {
            "return;".to_string()
        } else {
            format!("return {};", null_return_value(&method.return_type))
        };
        writeln!(out, "        {fail_ret}").ok();
        writeln!(out, "    }}").ok();

        let deref = match method.receiver.as_ref().unwrap_or(&ReceiverKind::Ref) {
            ReceiverKind::Ref => "let obj = unsafe { &*this };".to_string(),
            ReceiverKind::RefMut => "let obj = unsafe { &mut *this };".to_string(),
            ReceiverKind::Owned => "let obj = unsafe { *Box::from_raw(this) };".to_string(),
        };
        writeln!(out, "    {deref}").ok();
    }

    // Null-check and convert each parameter
    for p in &method.params {
        write!(
            out,
            "{}",
            gen_param_conversion(p, has_error, &method.return_type, core_import)
        )
        .ok();
    }

    // Build the call expression — pass &ref for String/Bytes/Named params, owned for Path
    let arg_names: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let rs = format!("{}_rs", p.name);
            match &p.ty {
                TypeRef::Path if !p.optional => rs, // PathBuf is passed owned
                TypeRef::String | TypeRef::Char | TypeRef::Bytes | TypeRef::Named(_) if !p.optional => {
                    format!("&{rs}")
                }
                TypeRef::String | TypeRef::Char | TypeRef::Bytes if p.optional => {
                    format!("{rs}.as_deref()")
                }
                TypeRef::Path if p.optional => rs, // Optional<PathBuf> passed owned
                _ => rs,
            }
        })
        .collect();
    let call_args = arg_names.join(", ");

    if method.is_static {
        writeln!(
            out,
            "    let result = {core_import}::{type_name}::{method_name}({call_args});"
        )
        .ok();
    } else {
        writeln!(out, "    let result = obj.{method_name}({call_args});").ok();
    }

    // Handle return
    if has_error {
        writeln!(out, "    match result {{").ok();
        if is_void_return(&method.return_type) {
            writeln!(out, "        Ok(()) => 0,").ok();
        } else {
            writeln!(out, "        Ok(val) => {{").ok();
            write!(out, "{}", gen_value_to_c("val", &method.return_type, "            ")).ok();
            writeln!(out, "        }}").ok();
        }
        writeln!(out, "        Err(e) => {{").ok();
        writeln!(out, "            set_last_error(2, &e.to_string());").ok();
        if is_void_return(&method.return_type) {
            writeln!(out, "            -1").ok();
        } else {
            writeln!(out, "            {}", null_return_value(&method.return_type)).ok();
        }
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
    } else if is_void_return(&method.return_type) {
        // void, no error — result is already ()
    } else {
        write!(out, "{}", gen_owned_value_to_c("result", &method.return_type, "    ")).ok();
    }

    write!(out, "}}").ok();
    out
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

fn gen_free_function(
    func: &FunctionDef,
    prefix: &str,
    core_import: &str,
    opaque_types: &std::collections::HashSet<String>,
) -> String {
    let fn_name_snake = func.name.to_snake_case();
    let ffi_name = format!("{prefix}_{fn_name_snake}");
    let func_name = &func.name;

    let mut out = String::with_capacity(2048);

    if !func.doc.is_empty() {
        for line in func.doc.lines() {
            writeln!(out, "/// {}", line).ok();
        }
    }
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").ok();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .ok();
    // Count total FFI params: params + extra _len for Bytes params
    let ffi_param_count = func.params.len() + func.params.iter().filter(|p| matches!(p.ty, TypeRef::Bytes)).count();
    if ffi_param_count > 7 {
        writeln!(out, "#[allow(clippy::too_many_arguments)]").ok();
    }
    writeln!(out, "#[unsafe(no_mangle)]").ok();

    let has_error = func.error_type.is_some();
    let ret_type = if has_error && is_void_return(&func.return_type) {
        "i32".to_string()
    } else {
        c_return_type(&func.return_type, core_import).into_owned()
    };

    // Check if this function will be unimplemented before building params
    let has_opaque_param = func
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if opaque_types.contains(n)));
    let has_opaque_return = has_opaque_type_in_return(&func.return_type, opaque_types);
    let will_be_unimplemented = func.sanitized || has_opaque_param || has_opaque_return;

    // Build parameter list — prefix with _ if unimplemented
    let mut params = Vec::new();
    for p in &func.params {
        let param_name = if will_be_unimplemented {
            format!("_{}", p.name)
        } else {
            p.name.clone()
        };
        params.push(format!("    {}: {}", param_name, c_param_type(&p.ty, core_import)));
        // Bytes parameters need a separate length parameter
        if matches!(p.ty, TypeRef::Bytes) {
            let len_param_name = if will_be_unimplemented {
                format!("_{}_len", p.name)
            } else {
                format!("{}_len", p.name)
            };
            params.push(format!("    {}: usize", len_param_name));
        }
    }

    if is_void_return(&func.return_type) && !has_error {
        writeln!(out, "pub unsafe extern \"C\" fn {ffi_name}(").ok();
        writeln!(out, "{}", params.join(",\n")).ok();
        writeln!(out, ") {{").ok();
    } else {
        writeln!(out, "pub unsafe extern \"C\" fn {ffi_name}(").ok();
        writeln!(out, "{}", params.join(",\n")).ok();
        writeln!(out, ") -> {ret_type} {{").ok();
    }

    writeln!(out, "    clear_last_error();").ok();

    // If function signature was sanitized or involves opaque types, generate unimplemented body
    if will_be_unimplemented {
        writeln!(
            out,
            "{}",
            gen_ffi_unimplemented_body(&func.return_type, func_name, has_error)
        )
        .ok();
        write!(out, "}}").ok();
        return out;
    }

    // Convert parameters
    for p in &func.params {
        write!(
            out,
            "{}",
            gen_param_conversion(p, has_error, &func.return_type, core_import)
        )
        .ok();
    }

    // Call — pass &ref for String/Bytes/Named params, owned for Path
    let arg_names: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let rs = format!("{}_rs", p.name);
            match &p.ty {
                TypeRef::Path if !p.optional => rs, // PathBuf is passed owned
                TypeRef::String | TypeRef::Char | TypeRef::Bytes | TypeRef::Named(_) if !p.optional => {
                    format!("&{rs}")
                }
                TypeRef::String | TypeRef::Char | TypeRef::Bytes if p.optional => {
                    format!("{rs}.as_deref()")
                }
                TypeRef::Path if p.optional => rs, // Optional<PathBuf> passed owned
                _ => rs,
            }
        })
        .collect();
    let call_args = arg_names.join(", ");

    writeln!(out, "    let result = {core_import}::{func_name}({call_args});").ok();

    // Handle return
    if has_error {
        writeln!(out, "    match result {{").ok();
        if is_void_return(&func.return_type) {
            writeln!(out, "        Ok(()) => 0,").ok();
        } else {
            writeln!(out, "        Ok(val) => {{").ok();
            write!(out, "{}", gen_value_to_c("val", &func.return_type, "            ")).ok();
            writeln!(out, "        }}").ok();
        }
        writeln!(out, "        Err(e) => {{").ok();
        writeln!(out, "            set_last_error(2, &e.to_string());").ok();
        if is_void_return(&func.return_type) {
            writeln!(out, "            -1").ok();
        } else {
            writeln!(out, "            {}", null_return_value(&func.return_type)).ok();
        }
        writeln!(out, "        }}").ok();
        writeln!(out, "    }}").ok();
    } else if is_void_return(&func.return_type) {
        // nothing
    } else {
        write!(out, "{}", gen_owned_value_to_c("result", &func.return_type, "    ")).ok();
    }

    write!(out, "}}").ok();
    out
}

// ---------------------------------------------------------------------------
// Parameter conversion (C types -> Rust)
// ---------------------------------------------------------------------------

fn gen_param_conversion(param: &ParamDef, has_error: bool, return_type: &TypeRef, core_import: &str) -> String {
    let name = &param.name;
    let rs_name = format!("{name}_rs");
    let mut out = String::with_capacity(2048);

    let fail_ret = if has_error && is_void_return(return_type) {
        "return -1;"
    } else if is_void_return(return_type) {
        "return;"
    } else {
        // Use null_return_value to get the correct default for the return type
        // (handles primitives, floats, Optional, Duration, pointers)
        match null_return_value(return_type) {
            "()" => "return;",
            v => {
                // Leak: we need a 'static str but null_return_value returns &'static str
                // The values are all string literals so this is fine
                let ret = format!("return {};", v);
                // Use a leaked string since fail_ret needs 'static lifetime
                // This is called once per function generation, not in a hot loop
                Box::leak(ret.into_boxed_str()) as &str
            }
        }
    };

    if param.optional {
        // Optional parameter — null means None
        match &param.ty {
            TypeRef::String | TypeRef::Char | TypeRef::Path | TypeRef::Json => {
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").ok();
                writeln!(out, "        None").ok();
                writeln!(out, "    }} else {{").ok();
                writeln!(out, "        match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{").ok();
                writeln!(out, "            Ok(s) => Some(s.to_string()),").ok();
                writeln!(out, "            Err(_) => {{").ok();
                writeln!(
                    out,
                    "                set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "                {fail_ret}").ok();
                writeln!(out, "            }}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
            TypeRef::Named(type_name) => {
                let qualified = format!("{core_import}::{type_name}");
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").ok();
                writeln!(out, "        None").ok();
                writeln!(out, "    }} else {{").ok();
                writeln!(
                    out,
                    "        Some(unsafe {{ &*({name} as *const {qualified}) }}.clone())"
                )
                .ok();
                writeln!(out, "    }};").ok();
            }
            _ => {
                // Fallback: treat as nullable JSON string
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").ok();
                writeln!(out, "        None").ok();
                writeln!(out, "    }} else {{").ok();
                writeln!(out, "        match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{").ok();
                writeln!(out, "            Ok(s) => Some(s.to_string()),").ok();
                writeln!(out, "            Err(_) => {{").ok();
                writeln!(
                    out,
                    "                set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "                {fail_ret}").ok();
                writeln!(out, "            }}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
        }
    } else {
        match &param.ty {
            TypeRef::String | TypeRef::Char => {
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .ok();
                writeln!(out, "        Ok(s) => s.to_string(),").ok();
                writeln!(out, "        Err(_) => {{").ok();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "            {fail_ret}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
            TypeRef::Path => {
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .ok();
                writeln!(out, "        Ok(s) => std::path::PathBuf::from(s),").ok();
                writeln!(out, "        Err(_) => {{").ok();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "            {fail_ret}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
            TypeRef::Json => {
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .ok();
                writeln!(out, "        Ok(s) => s.to_string(),").ok();
                writeln!(out, "        Err(_) => {{").ok();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "            {fail_ret}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
            TypeRef::Primitive(prim) => match prim {
                alef_core::ir::PrimitiveType::Bool => {
                    writeln!(out, "    let {rs_name} = {name} != 0;").ok();
                }
                _ => {
                    writeln!(out, "    let {rs_name} = {name};").ok();
                }
            },
            TypeRef::Named(_type_name) => {
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(out, "    let {rs_name} = unsafe {{ &*{name} }}.clone();").ok();
            }
            TypeRef::Bytes => {
                // Bytes come as (*const u8, len: usize) — the len param is a separate
                // parameter named {name}_len by convention.
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(
                    out,
                    "    let {rs_name} = unsafe {{ std::slice::from_raw_parts({name}, {name}_len) }}.to_vec();"
                )
                .ok();
            }
            TypeRef::Vec(_) | TypeRef::Map(_, _) => {
                // Passed as JSON string
                writeln!(out, "    if {name}.is_null() {{").ok();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "        {fail_ret}").ok();
                writeln!(out, "    }}").ok();
                writeln!(
                    out,
                    "    let {rs_name}_str = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .ok();
                writeln!(out, "        Ok(s) => s,").ok();
                writeln!(out, "        Err(_) => {{").ok();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .ok();
                writeln!(out, "            {fail_ret}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
                writeln!(out, "    let {rs_name} = match serde_json::from_str({rs_name}_str) {{").ok();
                writeln!(out, "        Ok(v) => v,").ok();
                writeln!(out, "        Err(e) => {{").ok();
                writeln!(out, "            set_last_error(2, &e.to_string());").ok();
                writeln!(out, "            {fail_ret}").ok();
                writeln!(out, "        }}").ok();
                writeln!(out, "    }};").ok();
            }
            TypeRef::Optional(_) => {
                // Should not happen for non-optional param, but handle gracefully
                writeln!(out, "    let {rs_name} = {name};").ok();
            }
            TypeRef::Duration => {
                // Duration passed as u64 seconds
                writeln!(out, "    let {rs_name} = std::time::Duration::from_secs({name});").ok();
            }
            TypeRef::Unit => {
                // No parameter to convert
            }
        }
    }

    out
}

/// Check if a return type involves opaque types in positions that need serde
/// (Vec/Map of opaque types can't be serialized to JSON).
fn has_opaque_type_in_return(ty: &TypeRef, opaque_types: &std::collections::HashSet<String>) -> bool {
    match ty {
        TypeRef::Vec(inner) | TypeRef::Optional(inner) => has_opaque_type_in_return(inner, opaque_types),
        TypeRef::Map(k, v) => has_opaque_type_in_return(k, opaque_types) || has_opaque_type_in_return(v, opaque_types),
        TypeRef::Named(n) => opaque_types.contains(n),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Convert owned Rust value to C return (non-Result path)
// ---------------------------------------------------------------------------

fn gen_owned_value_to_c(expr: &str, ty: &TypeRef, indent: &str) -> String {
    let mut out = String::with_capacity(2048);
    match ty {
        TypeRef::Primitive(prim) => match prim {
            alef_core::ir::PrimitiveType::Bool => {
                writeln!(out, "{indent}if {expr} {{ 1 }} else {{ 0 }}").ok();
            }
            _ => {
                writeln!(out, "{indent}{expr}").ok();
            }
        },
        TypeRef::String | TypeRef::Char => {
            writeln!(out, "{indent}match CString::new({expr}) {{").ok();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Json => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").ok();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").ok();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}    }},").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Path => {
            writeln!(
                out,
                "{indent}match CString::new({expr}.to_string_lossy().to_string()) {{"
            )
            .ok();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Named(_) => {
            writeln!(out, "{indent}Box::into_raw(Box::new({expr}))").ok();
        }
        TypeRef::Vec(_) | TypeRef::Map(_, _) => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").ok();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").ok();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").ok();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}    }},").ok();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Bytes => {
            // Return pointer; assume out-param for length. Cast to *mut u8 to match FFI signature.
            writeln!(out, "{indent}{expr}.as_ptr() as *mut u8").ok();
        }
        TypeRef::Optional(inner) => {
            writeln!(out, "{indent}match {expr} {{").ok();
            writeln!(out, "{indent}    Some(val) => {{").ok();
            write!(
                out,
                "{}",
                gen_owned_value_to_c("val", inner, &format!("{indent}        "))
            )
            .ok();
            writeln!(out, "{indent}    }}").ok();
            writeln!(
                out,
                "{indent}    None => {},",
                null_return_value(&TypeRef::Optional(inner.clone()))
            )
            .ok();
            writeln!(out, "{indent}}}").ok();
        }
        TypeRef::Duration => {
            writeln!(out, "{indent}{expr}.as_secs()").ok();
        }
        TypeRef::Unit => {}
    }
    out
}

// ---------------------------------------------------------------------------
// Enum conversions
// ---------------------------------------------------------------------------

fn gen_enum_from_i32(enum_def: &EnumDef, prefix: &str, _core_import: &str) -> String {
    let enum_snake = enum_def.name.to_snake_case();
    let enum_name = &enum_def.name;
    let mut out = String::with_capacity(2048);

    writeln!(
        out,
        "/// Convert an integer to a `{enum_name}` variant. Returns -1 on invalid input."
    )
    .ok();
    writeln!(out, "/// # Safety").ok();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").ok();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .ok();
    writeln!(out, "#[unsafe(no_mangle)]").ok();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{enum_snake}_from_i32(value: i32) -> i32 {{"
    )
    .ok();
    writeln!(out, "    match value {{").ok();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "        {idx} => {idx}, // {}", variant.name).ok();
    }
    writeln!(out, "        _ => {{").ok();
    writeln!(out, "            set_last_error(1, \"Invalid {enum_name} variant\");").ok();
    writeln!(out, "            -1").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

fn gen_enum_to_i32(enum_def: &EnumDef, prefix: &str, _core_import: &str) -> String {
    let enum_snake = enum_def.name.to_snake_case();
    let enum_name = &enum_def.name;
    let mut out = String::with_capacity(2048);

    writeln!(
        out,
        "/// Convert a `{enum_name}` variant name (C string) to its integer value. Returns -1 on invalid input."
    )
    .ok();
    writeln!(out, "/// # Safety").ok();
    writeln!(
        out,
        "/// Caller must ensure `ptr` is a valid pointer to a `c_char` or null."
    )
    .ok();
    writeln!(out, "#[unsafe(no_mangle)]").ok();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{enum_snake}_from_str(name: *const c_char) -> i32 {{"
    )
    .ok();
    writeln!(out, "    if name.is_null() {{").ok();
    writeln!(out, "        set_last_error(1, \"Null pointer passed for enum name\");").ok();
    writeln!(out, "        return -1;").ok();
    writeln!(out, "    }}").ok();
    writeln!(out, "    let s = match unsafe {{ CStr::from_ptr(name) }}.to_str() {{").ok();
    writeln!(out, "        Ok(s) => s,").ok();
    writeln!(out, "        Err(_) => {{").ok();
    writeln!(out, "            set_last_error(1, \"Invalid UTF-8 in enum name\");").ok();
    writeln!(out, "            return -1;").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }};").ok();
    writeln!(out, "    match s {{").ok();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "        \"{}\" => {idx},", variant.name).ok();
    }
    writeln!(out, "        _ => {{").ok();
    writeln!(out, "            set_last_error(1, \"Unknown {enum_name} variant\");").ok();
    writeln!(out, "            -1").ok();
    writeln!(out, "        }}").ok();
    writeln!(out, "    }}").ok();
    write!(out, "}}").ok();
    out
}

// ---------------------------------------------------------------------------
// cbindgen.toml generation
// ---------------------------------------------------------------------------

fn gen_cbindgen_toml(prefix: &str, api: &ApiSurface) -> String {
    let prefix_upper = prefix.to_uppercase();

    // Collect all type names that appear in the API surface and need forward
    // declarations in the generated C header. cbindgen renames Rust types using
    // the export prefix, producing e.g. `HTMMetadataConfig` for prefix `HTM`.
    let mut type_names: Vec<String> = api
        .types
        .iter()
        .map(|t| format!("{prefix_upper}{}", t.name.to_pascal_case()))
        .collect();

    // Include enum types as well — they may appear as opaque handles in
    // function signatures when used across module boundaries.
    for e in &api.enums {
        let c_name = format!("{prefix_upper}{}", e.name.to_pascal_case());
        if !type_names.contains(&c_name) {
            type_names.push(c_name);
        }
    }

    type_names.sort();

    let forward_decls: String = type_names
        .iter()
        .map(|name| format!("typedef struct {name} {name};"))
        .collect::<Vec<_>>()
        .join("\n");

    let after_includes = if forward_decls.is_empty() {
        String::new()
    } else {
        format!("\nafter_includes = \"\"\"\n/* Opaque type forward declarations */\n{forward_decls}\n\"\"\"\n")
    };

    format!(
        r#"# This file is auto-generated by alef. DO NOT EDIT.
language = "C"
include_guard = "{prefix_upper}_H"
pragma_once = true
autogen_warning = "/* This file is auto-generated by alef. DO NOT EDIT. */"
{after_includes}
[defines]
"target_os = windows" = "SKIF_WINDOWS"

[export]
prefix = "{prefix_upper}"
include = []
exclude = []

[fn]
args = "vertical"
"#
    )
}

// ---------------------------------------------------------------------------
// build.rs generation
// ---------------------------------------------------------------------------

fn gen_build_rs(header_name: &str) -> String {
    format!(
        r#"// This file is auto-generated by alef. DO NOT EDIT.
fn main() {{
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    cbindgen::generate(crate_dir)
        .expect("Unable to generate C bindings")
        .write_to_file("include/{header_name}");
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::ir::*;

    fn sample_api() -> ApiSurface {
        ApiSurface {
            crate_name: "my-lib".to_string(),
            version: "1.0.0".to_string(),
            types: vec![TypeDef {
                name: "Config".to_string(),
                rust_path: "my_lib::Config".to_string(),
                fields: vec![
                    FieldDef {
                        name: "timeout".to_string(),
                        ty: TypeRef::Primitive(PrimitiveType::U64),
                        optional: false,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                    FieldDef {
                        name: "name".to_string(),
                        ty: TypeRef::String,
                        optional: false,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                    FieldDef {
                        name: "verbose".to_string(),
                        ty: TypeRef::Primitive(PrimitiveType::Bool),
                        optional: true,
                        default: None,
                        doc: String::new(),
                        sanitized: false,
                        is_boxed: false,
                        type_rust_path: None,
                        cfg: None,
                        typed_default: None,
                    },
                ],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_trait: false,
                has_default: false,
                has_stripped_cfg_fields: false,
                is_return_type: false,
                doc: "Configuration struct.".to_string(),
                cfg: None,
            }],
            functions: vec![FunctionDef {
                name: "extract".to_string(),
                rust_path: "my_lib::extract".to_string(),
                params: vec![ParamDef {
                    name: "path".to_string(),
                    ty: TypeRef::Path,
                    optional: false,
                    default: None,
                    sanitized: false,
                    typed_default: None,
                }],
                return_type: TypeRef::Named("ExtractionResult".to_string()),
                is_async: false,
                error_type: Some("MyError".to_string()),
                doc: "Extract content from a file.".to_string(),
                cfg: None,
                sanitized: false,
                returns_ref: false,
            }],
            enums: vec![EnumDef {
                name: "OutputFormat".to_string(),
                rust_path: "my_lib::OutputFormat".to_string(),
                variants: vec![
                    EnumVariant {
                        name: "Text".to_string(),
                        fields: vec![],
                        doc: String::new(),
                        is_default: false,
                    },
                    EnumVariant {
                        name: "Html".to_string(),
                        fields: vec![],
                        doc: String::new(),
                        is_default: false,
                    },
                ],
                doc: "Output format.".to_string(),
                cfg: None,
            }],
            errors: vec![],
        }
    }

    fn sample_config() -> AlefConfig {
        toml::from_str(
            r#"
            languages = ["ffi"]
            [crate]
            name = "my-lib"
            sources = ["src/lib.rs"]
            "#,
        )
        .unwrap()
    }

    #[test]
    fn test_generates_lib_rs() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        assert!(files.iter().any(|f| f.path.ends_with("lib.rs")));

        let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();
        assert!(lib.content.contains("extern \"C\""));
        assert!(lib.content.contains("my_lib_last_error_code"));
        assert!(lib.content.contains("my_lib_config_from_json"));
        assert!(lib.content.contains("my_lib_config_free"));
        assert!(lib.content.contains("my_lib_config_timeout"));
        assert!(lib.content.contains("my_lib_config_name"));
        assert!(lib.content.contains("my_lib_free_string"));
        assert!(lib.content.contains("my_lib_version"));
        assert!(lib.content.contains("my_lib_extract"));
        assert!(lib.content.contains("my_lib_output_format_from_i32"));
        assert!(lib.content.contains("my_lib_output_format_from_str"));
    }

    #[test]
    fn test_generates_cbindgen_toml() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let cbindgen = files.iter().find(|f| f.path.ends_with("cbindgen.toml")).unwrap();
        assert!(cbindgen.content.contains("MY_LIB_H"));
        assert!(cbindgen.content.contains("language = \"C\""));
    }

    #[test]
    fn test_generates_build_rs() {
        let api = sample_api();
        let config = sample_config();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let build = files.iter().find(|f| f.path.ends_with("build.rs")).unwrap();
        assert!(build.content.contains("cbindgen::generate"));
        assert!(build.content.contains("my_lib.h"));
    }

    #[test]
    fn test_custom_prefix() {
        let api = sample_api();
        let config: AlefConfig = toml::from_str(
            r#"
            languages = ["ffi"]
            [crate]
            name = "my-lib"
            sources = ["src/lib.rs"]
            [ffi]
            prefix = "ml"
            header_name = "mylib.h"
            "#,
        )
        .unwrap();
        let backend = FfiBackend;

        let files = backend.generate_bindings(&api, &config).unwrap();
        let lib = files.iter().find(|f| f.path.ends_with("lib.rs")).unwrap();
        assert!(lib.content.contains("ml_last_error_code"));
        assert!(lib.content.contains("ml_config_from_json"));

        let build = files.iter().find(|f| f.path.ends_with("build.rs")).unwrap();
        assert!(build.content.contains("mylib.h"));
    }
}
