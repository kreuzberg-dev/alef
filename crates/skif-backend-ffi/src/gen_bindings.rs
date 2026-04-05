use crate::type_map::{c_param_type, c_return_type, is_void_return};
use heck::ToSnakeCase;
use skif_codegen::builder::RustFileBuilder;
use skif_codegen::generators;
use skif_core::backend::{Backend, Capabilities, GeneratedFile};
use skif_core::config::{Language, SkifConfig, resolve_output_dir};
use skif_core::ir::{ApiSurface, EnumDef, FieldDef, FunctionDef, MethodDef, ParamDef, ReceiverKind, TypeDef, TypeRef};
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

    fn generate_bindings(&self, api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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
                content: gen_cbindgen_toml(&prefix),
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
}

// ---------------------------------------------------------------------------
// lib.rs generation
// ---------------------------------------------------------------------------

fn gen_lib_rs(api: &ApiSurface, prefix: &str, config: &SkifConfig) -> String {
    use std::collections::HashSet;

    let mut builder = RustFileBuilder::new().with_generated_header();

    // Imports
    builder.add_import("std::ffi::{c_char, CStr, CString}");
    builder.add_import("std::cell::RefCell");
    builder.add_import("serde_json");
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
        if skif_codegen::conversions::can_generate_enum_conversion(enum_def) {
            builder.add_item(&gen_enum_from_i32(enum_def, prefix, &core_import));
            builder.add_item(&gen_enum_to_i32(enum_def, prefix, &core_import));
        }
    }

    // Free functions
    for func in &api.functions {
        builder.add_item(&gen_free_function(func, prefix, &core_import, &opaque_types));
    }

    // Build adapter body map (consumed by generators via body substitution)
    let _adapter_bodies = skif_adapters::build_adapter_bodies(config, Language::Ffi).unwrap_or_default();

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
    .unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// JSON string must be valid UTF-8 and null-terminated.").unwrap();
    writeln!(
        out,
        "/// Returned handle must be freed with `{prefix}_{type_snake}_free`."
    )
    .unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{type_snake}_from_json(json: *const c_char) -> *mut {qualified} {{"
    )
    .unwrap();
    writeln!(out, "    clear_last_error();").unwrap();
    writeln!(out, "    if json.is_null() {{").unwrap();
    writeln!(
        out,
        "        set_last_error(1, \"Null pointer passed for JSON string\");"
    )
    .unwrap();
    writeln!(out, "        return std::ptr::null_mut();").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(
        out,
        "    let c_str = match unsafe {{ CStr::from_ptr(json) }}.to_str() {{"
    )
    .unwrap();
    writeln!(out, "        Ok(s) => s,").unwrap();
    writeln!(out, "        Err(_) => {{").unwrap();
    writeln!(out, "            set_last_error(1, \"Invalid UTF-8 in JSON string\");").unwrap();
    writeln!(out, "            return std::ptr::null_mut();").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }};").unwrap();
    writeln!(out, "    match serde_json::from_str::<{qualified}>(c_str) {{").unwrap();
    writeln!(out, "        Ok(val) => Box::into_raw(Box::new(val)),").unwrap();
    writeln!(out, "        Err(e) => {{").unwrap();
    writeln!(out, "            set_last_error(2, &e.to_string());").unwrap();
    writeln!(out, "            std::ptr::null_mut()").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();

    out
}

fn gen_type_free(typ: &TypeDef, prefix: &str, core_import: &str) -> String {
    let type_snake = typ.name.to_snake_case();
    let type_name = &typ.name;
    let qualified = format!("{core_import}::{type_name}");
    let mut out = String::with_capacity(2048);

    writeln!(out, "/// Free a `{type_name}` handle.").unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// Pointer must have been returned by this library, or be null.").unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{type_snake}_free(ptr: *mut {qualified}) {{"
    )
    .unwrap();
    writeln!(out, "    if !ptr.is_null() {{").unwrap();
    writeln!(out, "        unsafe {{ drop(Box::from_raw(ptr)); }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();

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

    writeln!(out, "/// Get the `{field_name}` field from a `{type_name}`.").unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// Pointer must be a valid handle returned by this library.").unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();

    // Determine if we need an extra out-param for byte-length
    let needs_len_out = matches!(field.ty, TypeRef::Bytes) && !field.optional;

    if needs_len_out {
        writeln!(
            out,
            "pub unsafe extern \"C\" fn {prefix}_{type_snake}_{field_name}(ptr: *const {qualified}, out_len: *mut usize) -> {ret_type} {{"
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "pub unsafe extern \"C\" fn {prefix}_{type_snake}_{field_name}(ptr: *const {qualified}) -> {ret_type} {{"
        )
        .unwrap();
    }

    // Null-check on ptr
    writeln!(out, "    if ptr.is_null() {{").unwrap();
    writeln!(out, "        return {};", null_return_value(&effective_ty)).unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    let obj = unsafe {{ &*ptr }};").unwrap();

    // Generate the accessor body based on field type
    write!(out, "{}", gen_field_access_body(field, needs_len_out)).unwrap();

    write!(out, "}}").unwrap();
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
        writeln!(out, "    match &obj.{field_name} {{").unwrap();
        writeln!(out, "        Some(val) => {{").unwrap();
        write!(out, "{}", gen_value_to_c(val_expr, &field.ty, "            ")).unwrap();
        writeln!(out, "        }}").unwrap();
        writeln!(
            out,
            "        None => {},",
            null_return_value(&TypeRef::Optional(Box::new(field.ty.clone())))
        )
        .unwrap();
        writeln!(out, "    }}").unwrap();
    } else if needs_len_out {
        // Bytes with length out-param
        writeln!(out, "    let data = &obj.{field_name};").unwrap();
        writeln!(out, "    if !out_len.is_null() {{").unwrap();
        writeln!(out, "        unsafe {{ *out_len = data.len(); }}").unwrap();
        writeln!(out, "    }}").unwrap();
        writeln!(out, "    data.as_ptr() as *mut u8").unwrap();
    } else {
        // When is_boxed: obj.field_name is Box<T>, deref to get T before cloning
        let access_expr = if field.is_boxed {
            format!("(*obj.{field_name})")
        } else {
            format!("obj.{field_name}")
        };
        write!(out, "{}", gen_value_to_c(&access_expr, &field.ty, "    ")).unwrap();
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
            if matches!(p, skif_core::ir::PrimitiveType::Bool) {
                writeln!(out, "{indent}{expr} as i32").unwrap();
            } else {
                writeln!(out, "{indent}{expr}").unwrap();
            }
        }
        TypeRef::String => {
            writeln!(out, "{indent}match CString::new({expr}.clone()) {{").unwrap();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Path => {
            writeln!(
                out,
                "{indent}match CString::new({expr}.to_string_lossy().to_string()) {{"
            )
            .unwrap();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Json => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").unwrap();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").unwrap();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}    }},").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Named(_) => {
            writeln!(out, "{indent}Box::into_raw(Box::new({expr}.clone()))").unwrap();
        }
        TypeRef::Vec(_) | TypeRef::Map(_, _) => {
            // Serialize as JSON
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").unwrap();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").unwrap();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}    }},").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Bytes => {
            // Return pointer; caller must also get length. Cast to *mut u8 to match FFI signature.
            writeln!(out, "{indent}{expr}.as_ptr() as *mut u8").unwrap();
        }
        TypeRef::Duration => {
            writeln!(out, "{indent}{expr}.as_secs()").unwrap();
        }
        TypeRef::Unit => {
            // nothing to return
        }
        TypeRef::Optional(inner) => {
            writeln!(out, "{indent}match &{expr} {{").unwrap();
            writeln!(out, "{indent}    Some(val) => {{").unwrap();
            write!(out, "{}", gen_value_to_c("val", inner, &format!("{indent}        "))).unwrap();
            writeln!(out, "{indent}    }}").unwrap();
            writeln!(
                out,
                "{indent}    None => {},",
                null_return_value(&TypeRef::Optional(inner.clone()))
            )
            .unwrap();
            writeln!(out, "{indent}}}").unwrap();
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
    use skif_core::ir::PrimitiveType;
    match ty {
        TypeRef::Primitive(p) => match p {
            PrimitiveType::F32 | PrimitiveType::F64 => "0.0",
            _ => "0",
        },
        TypeRef::String | TypeRef::Path | TypeRef::Json => "std::ptr::null_mut()",
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
            writeln!(out, "/// {}", line).unwrap();
        }
    }
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").unwrap();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();

    let qualified = format!("{core_import}::{type_name}");

    // Build parameter list
    let mut params = Vec::new();
    if !method.is_static {
        let receiver_ty = match method.receiver.as_ref().unwrap_or(&ReceiverKind::Ref) {
            ReceiverKind::Ref => format!("*const {qualified}"),
            ReceiverKind::RefMut | ReceiverKind::Owned => format!("*mut {qualified}"),
        };
        params.push(format!("    this: {receiver_ty}"));
    }
    for p in &method.params {
        params.push(format!("    {}: {}", p.name, c_param_type(&p.ty, core_import)));
        // Bytes parameters need a separate length parameter
        if matches!(p.ty, TypeRef::Bytes) {
            params.push(format!("    {}_len: usize", p.name));
        }
    }

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

    if is_void_return(&method.return_type) && !has_error {
        writeln!(out, "pub unsafe extern \"C\" fn {fn_name}(").unwrap();
        writeln!(out, "{}", params.join(",\n")).unwrap();
        writeln!(out, ") {{").unwrap();
    } else {
        writeln!(out, "pub unsafe extern \"C\" fn {fn_name}(").unwrap();
        writeln!(out, "{}", params.join(",\n")).unwrap();
        writeln!(out, ") -> {ret_type} {{").unwrap();
    }

    writeln!(out, "    clear_last_error();").unwrap();

    // If method signature was sanitized, generate todo!() — delegation would fail
    // Also skip if any param is an opaque Named type (no serde/Clone available)
    // or if return type involves opaque types in Vec/Map (no serde Serialize)
    let has_opaque_param = method
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if opaque_types.contains(n)));
    let has_opaque_return = has_opaque_type_in_return(&method.return_type, opaque_types);
    if method.sanitized || has_opaque_param || has_opaque_return {
        writeln!(
            out,
            "{}",
            gen_ffi_unimplemented_body(&method.return_type, &format!("{type_name}::{method_name}"), has_error)
        )
        .unwrap();
        write!(out, "}}").unwrap();
        return out;
    }

    // Null-check self
    if !method.is_static {
        writeln!(out, "    if this.is_null() {{").unwrap();
        writeln!(out, "        set_last_error(1, \"Null pointer passed for self\");").unwrap();
        let fail_ret = if has_error && is_void_return(&method.return_type) {
            "return -1;".to_string()
        } else if is_void_return(&method.return_type) {
            "return;".to_string()
        } else {
            format!("return {};", null_return_value(&method.return_type))
        };
        writeln!(out, "        {fail_ret}").unwrap();
        writeln!(out, "    }}").unwrap();

        let deref = match method.receiver.as_ref().unwrap_or(&ReceiverKind::Ref) {
            ReceiverKind::Ref => "let obj = unsafe { &*this };".to_string(),
            ReceiverKind::RefMut => "let obj = unsafe { &mut *this };".to_string(),
            ReceiverKind::Owned => "let obj = unsafe { *Box::from_raw(this) };".to_string(),
        };
        writeln!(out, "    {deref}").unwrap();
    }

    // Null-check and convert each parameter
    for p in &method.params {
        write!(
            out,
            "{}",
            gen_param_conversion(p, has_error, &method.return_type, core_import)
        )
        .unwrap();
    }

    // Build the call expression — pass &ref for String/Bytes/Named params, owned for Path
    let arg_names: Vec<String> = method
        .params
        .iter()
        .map(|p| {
            let rs = format!("{}_rs", p.name);
            match &p.ty {
                TypeRef::Path if !p.optional => rs, // PathBuf is passed owned
                TypeRef::String | TypeRef::Bytes | TypeRef::Named(_) if !p.optional => {
                    format!("&{rs}")
                }
                TypeRef::String | TypeRef::Bytes if p.optional => {
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
        .unwrap();
    } else {
        writeln!(out, "    let result = obj.{method_name}({call_args});").unwrap();
    }

    // Handle return
    if has_error {
        writeln!(out, "    match result {{").unwrap();
        if is_void_return(&method.return_type) {
            writeln!(out, "        Ok(()) => 0,").unwrap();
        } else {
            writeln!(out, "        Ok(val) => {{").unwrap();
            write!(out, "{}", gen_value_to_c("val", &method.return_type, "            ")).unwrap();
            writeln!(out, "        }}").unwrap();
        }
        writeln!(out, "        Err(e) => {{").unwrap();
        writeln!(out, "            set_last_error(2, &e.to_string());").unwrap();
        if is_void_return(&method.return_type) {
            writeln!(out, "            -1").unwrap();
        } else {
            writeln!(out, "            {}", null_return_value(&method.return_type)).unwrap();
        }
        writeln!(out, "        }}").unwrap();
        writeln!(out, "    }}").unwrap();
    } else if is_void_return(&method.return_type) {
        // void, no error — result is already ()
    } else {
        write!(out, "{}", gen_owned_value_to_c("result", &method.return_type, "    ")).unwrap();
    }

    write!(out, "}}").unwrap();
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
            writeln!(out, "/// {}", line).unwrap();
        }
    }
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").unwrap();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();

    // Build parameter list
    let mut params = Vec::new();
    for p in &func.params {
        params.push(format!("    {}: {}", p.name, c_param_type(&p.ty, core_import)));
        // Bytes parameters need a separate length parameter
        if matches!(p.ty, TypeRef::Bytes) {
            params.push(format!("    {}_len: usize", p.name));
        }
    }

    let has_error = func.error_type.is_some();
    let ret_type = if has_error && is_void_return(&func.return_type) {
        "i32".to_string()
    } else {
        c_return_type(&func.return_type, core_import).into_owned()
    };

    if is_void_return(&func.return_type) && !has_error {
        writeln!(out, "pub unsafe extern \"C\" fn {ffi_name}(").unwrap();
        writeln!(out, "{}", params.join(",\n")).unwrap();
        writeln!(out, ") {{").unwrap();
    } else {
        writeln!(out, "pub unsafe extern \"C\" fn {ffi_name}(").unwrap();
        writeln!(out, "{}", params.join(",\n")).unwrap();
        writeln!(out, ") -> {ret_type} {{").unwrap();
    }

    writeln!(out, "    clear_last_error();").unwrap();

    // If function signature was sanitized or involves opaque types, generate todo!()
    let has_opaque_param = func
        .params
        .iter()
        .any(|p| matches!(&p.ty, TypeRef::Named(n) if opaque_types.contains(n)));
    let has_opaque_return = has_opaque_type_in_return(&func.return_type, opaque_types);
    if func.sanitized || has_opaque_param || has_opaque_return {
        writeln!(
            out,
            "{}",
            gen_ffi_unimplemented_body(&func.return_type, func_name, has_error)
        )
        .unwrap();
        write!(out, "}}").unwrap();
        return out;
    }

    // Convert parameters
    for p in &func.params {
        write!(
            out,
            "{}",
            gen_param_conversion(p, has_error, &func.return_type, core_import)
        )
        .unwrap();
    }

    // Call — pass &ref for String/Bytes/Named params, owned for Path
    let arg_names: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            let rs = format!("{}_rs", p.name);
            match &p.ty {
                TypeRef::Path if !p.optional => rs, // PathBuf is passed owned
                TypeRef::String | TypeRef::Bytes | TypeRef::Named(_) if !p.optional => {
                    format!("&{rs}")
                }
                TypeRef::String | TypeRef::Bytes if p.optional => {
                    format!("{rs}.as_deref()")
                }
                TypeRef::Path if p.optional => rs, // Optional<PathBuf> passed owned
                _ => rs,
            }
        })
        .collect();
    let call_args = arg_names.join(", ");

    writeln!(out, "    let result = {core_import}::{func_name}({call_args});").unwrap();

    // Handle return
    if has_error {
        writeln!(out, "    match result {{").unwrap();
        if is_void_return(&func.return_type) {
            writeln!(out, "        Ok(()) => 0,").unwrap();
        } else {
            writeln!(out, "        Ok(val) => {{").unwrap();
            write!(out, "{}", gen_value_to_c("val", &func.return_type, "            ")).unwrap();
            writeln!(out, "        }}").unwrap();
        }
        writeln!(out, "        Err(e) => {{").unwrap();
        writeln!(out, "            set_last_error(2, &e.to_string());").unwrap();
        if is_void_return(&func.return_type) {
            writeln!(out, "            -1").unwrap();
        } else {
            writeln!(out, "            {}", null_return_value(&func.return_type)).unwrap();
        }
        writeln!(out, "        }}").unwrap();
        writeln!(out, "    }}").unwrap();
    } else if is_void_return(&func.return_type) {
        // nothing
    } else {
        write!(out, "{}", gen_owned_value_to_c("result", &func.return_type, "    ")).unwrap();
    }

    write!(out, "}}").unwrap();
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
            TypeRef::String | TypeRef::Path | TypeRef::Json => {
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").unwrap();
                writeln!(out, "        None").unwrap();
                writeln!(out, "    }} else {{").unwrap();
                writeln!(out, "        match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{").unwrap();
                writeln!(out, "            Ok(s) => Some(s.to_string()),").unwrap();
                writeln!(out, "            Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "                set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "                {fail_ret}").unwrap();
                writeln!(out, "            }}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
            TypeRef::Named(type_name) => {
                let qualified = format!("{core_import}::{type_name}");
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").unwrap();
                writeln!(out, "        None").unwrap();
                writeln!(out, "    }} else {{").unwrap();
                writeln!(
                    out,
                    "        Some(unsafe {{ &*({name} as *const {qualified}) }}.clone())"
                )
                .unwrap();
                writeln!(out, "    }};").unwrap();
            }
            _ => {
                // Fallback: treat as nullable JSON string
                writeln!(out, "    let {rs_name} = if {name}.is_null() {{").unwrap();
                writeln!(out, "        None").unwrap();
                writeln!(out, "    }} else {{").unwrap();
                writeln!(out, "        match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{").unwrap();
                writeln!(out, "            Ok(s) => Some(s.to_string()),").unwrap();
                writeln!(out, "            Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "                set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "                {fail_ret}").unwrap();
                writeln!(out, "            }}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
        }
    } else {
        match &param.ty {
            TypeRef::String => {
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .unwrap();
                writeln!(out, "        Ok(s) => s.to_string(),").unwrap();
                writeln!(out, "        Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "            {fail_ret}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
            TypeRef::Path => {
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .unwrap();
                writeln!(out, "        Ok(s) => std::path::PathBuf::from(s),").unwrap();
                writeln!(out, "        Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "            {fail_ret}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
            TypeRef::Json => {
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(
                    out,
                    "    let {rs_name} = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .unwrap();
                writeln!(out, "        Ok(s) => s.to_string(),").unwrap();
                writeln!(out, "        Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "            {fail_ret}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
            TypeRef::Primitive(prim) => match prim {
                skif_core::ir::PrimitiveType::Bool => {
                    writeln!(out, "    let {rs_name} = {name} != 0;").unwrap();
                }
                _ => {
                    writeln!(out, "    let {rs_name} = {name};").unwrap();
                }
            },
            TypeRef::Named(_type_name) => {
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(out, "    let {rs_name} = unsafe {{ &*{name} }}.clone();").unwrap();
            }
            TypeRef::Bytes => {
                // Bytes come as (*const u8, len: usize) — the len param is a separate
                // parameter named {name}_len by convention.
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(
                    out,
                    "    let {rs_name} = unsafe {{ std::slice::from_raw_parts({name}, {name}_len) }}.to_vec();"
                )
                .unwrap();
            }
            TypeRef::Vec(_) | TypeRef::Map(_, _) => {
                // Passed as JSON string
                writeln!(out, "    if {name}.is_null() {{").unwrap();
                writeln!(
                    out,
                    "        set_last_error(1, \"Null pointer passed for parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "        {fail_ret}").unwrap();
                writeln!(out, "    }}").unwrap();
                writeln!(
                    out,
                    "    let {rs_name}_str = match unsafe {{ CStr::from_ptr({name}) }}.to_str() {{"
                )
                .unwrap();
                writeln!(out, "        Ok(s) => s,").unwrap();
                writeln!(out, "        Err(_) => {{").unwrap();
                writeln!(
                    out,
                    "            set_last_error(1, \"Invalid UTF-8 in parameter '{name}'\");"
                )
                .unwrap();
                writeln!(out, "            {fail_ret}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
                writeln!(out, "    let {rs_name} = match serde_json::from_str({rs_name}_str) {{").unwrap();
                writeln!(out, "        Ok(v) => v,").unwrap();
                writeln!(out, "        Err(e) => {{").unwrap();
                writeln!(out, "            set_last_error(2, &e.to_string());").unwrap();
                writeln!(out, "            {fail_ret}").unwrap();
                writeln!(out, "        }}").unwrap();
                writeln!(out, "    }};").unwrap();
            }
            TypeRef::Optional(_) => {
                // Should not happen for non-optional param, but handle gracefully
                writeln!(out, "    let {rs_name} = {name};").unwrap();
            }
            TypeRef::Duration => {
                // Duration passed as u64 seconds
                writeln!(out, "    let {rs_name} = std::time::Duration::from_secs({name});").unwrap();
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
            skif_core::ir::PrimitiveType::Bool => {
                writeln!(out, "{indent}if {expr} {{ 1 }} else {{ 0 }}").unwrap();
            }
            _ => {
                writeln!(out, "{indent}{expr}").unwrap();
            }
        },
        TypeRef::String => {
            writeln!(out, "{indent}match CString::new({expr}) {{").unwrap();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Json => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").unwrap();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").unwrap();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}    }},").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Path => {
            writeln!(
                out,
                "{indent}match CString::new({expr}.to_string_lossy().to_string()) {{"
            )
            .unwrap();
            writeln!(out, "{indent}    Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Named(_) => {
            writeln!(out, "{indent}Box::into_raw(Box::new({expr}))").unwrap();
        }
        TypeRef::Vec(_) | TypeRef::Map(_, _) => {
            writeln!(out, "{indent}match serde_json::to_string(&{expr}) {{").unwrap();
            writeln!(out, "{indent}    Ok(s) => match CString::new(s) {{").unwrap();
            writeln!(out, "{indent}        Ok(cs) => cs.into_raw(),").unwrap();
            writeln!(out, "{indent}        Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}    }},").unwrap();
            writeln!(out, "{indent}    Err(_) => std::ptr::null_mut(),").unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Bytes => {
            // Return pointer; assume out-param for length. Cast to *mut u8 to match FFI signature.
            writeln!(out, "{indent}{expr}.as_ptr() as *mut u8").unwrap();
        }
        TypeRef::Optional(inner) => {
            writeln!(out, "{indent}match {expr} {{").unwrap();
            writeln!(out, "{indent}    Some(val) => {{").unwrap();
            write!(
                out,
                "{}",
                gen_owned_value_to_c("val", inner, &format!("{indent}        "))
            )
            .unwrap();
            writeln!(out, "{indent}    }}").unwrap();
            writeln!(
                out,
                "{indent}    None => {},",
                null_return_value(&TypeRef::Optional(inner.clone()))
            )
            .unwrap();
            writeln!(out, "{indent}}}").unwrap();
        }
        TypeRef::Duration => {
            writeln!(out, "{indent}{expr}.as_secs()").unwrap();
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
    .unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(out, "/// Caller must ensure all pointer arguments are valid or null.").unwrap();
    writeln!(
        out,
        "/// Returned pointers must be freed with the appropriate free function."
    )
    .unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{enum_snake}_from_i32(value: i32) -> i32 {{"
    )
    .unwrap();
    writeln!(out, "    match value {{").unwrap();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "        {idx} => {idx}, // {}", variant.name).unwrap();
    }
    writeln!(out, "        _ => {{").unwrap();
    writeln!(out, "            set_last_error(1, \"Invalid {enum_name} variant\");").unwrap();
    writeln!(out, "            -1").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
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
    .unwrap();
    writeln!(out, "/// # Safety").unwrap();
    writeln!(
        out,
        "/// Caller must ensure `ptr` is a valid pointer to a `c_char` or null."
    )
    .unwrap();
    writeln!(out, "#[unsafe(no_mangle)]").unwrap();
    writeln!(
        out,
        "pub unsafe extern \"C\" fn {prefix}_{enum_snake}_from_str(name: *const c_char) -> i32 {{"
    )
    .unwrap();
    writeln!(out, "    if name.is_null() {{").unwrap();
    writeln!(out, "        set_last_error(1, \"Null pointer passed for enum name\");").unwrap();
    writeln!(out, "        return -1;").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    let s = match unsafe {{ CStr::from_ptr(name) }}.to_str() {{").unwrap();
    writeln!(out, "        Ok(s) => s,").unwrap();
    writeln!(out, "        Err(_) => {{").unwrap();
    writeln!(out, "            set_last_error(1, \"Invalid UTF-8 in enum name\");").unwrap();
    writeln!(out, "            return -1;").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }};").unwrap();
    writeln!(out, "    match s {{").unwrap();
    for (idx, variant) in enum_def.variants.iter().enumerate() {
        writeln!(out, "        \"{}\" => {idx},", variant.name).unwrap();
    }
    writeln!(out, "        _ => {{").unwrap();
    writeln!(out, "            set_last_error(1, \"Unknown {enum_name} variant\");").unwrap();
    writeln!(out, "            -1").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "    }}").unwrap();
    write!(out, "}}").unwrap();
    out
}

// ---------------------------------------------------------------------------
// cbindgen.toml generation
// ---------------------------------------------------------------------------

fn gen_cbindgen_toml(prefix: &str) -> String {
    let prefix_upper = prefix.to_uppercase();
    format!(
        r#"# This file is auto-generated by skif. DO NOT EDIT.
language = "C"
include_guard = "{prefix_upper}_H"
pragma_once = true
autogen_warning = "/* This file is auto-generated by skif. DO NOT EDIT. */"

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
        r#"// This file is auto-generated by skif. DO NOT EDIT.
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
    use skif_core::ir::*;

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
                    },
                ],
                methods: vec![],
                is_opaque: false,
                is_clone: true,
                is_trait: false,
                has_default: false,
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
                    },
                    EnumVariant {
                        name: "Html".to_string(),
                        fields: vec![],
                        doc: String::new(),
                    },
                ],
                doc: "Output format.".to_string(),
                cfg: None,
            }],
            errors: vec![],
        }
    }

    fn sample_config() -> SkifConfig {
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
        let config: SkifConfig = toml::from_str(
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
