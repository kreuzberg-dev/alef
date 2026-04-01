use skif_core::config::{AdapterConfig, Language, SkifConfig};

/// Generate a sync function adapter for the given language.
pub fn generate(adapter: &AdapterConfig, language: Language, config: &SkifConfig) -> anyhow::Result<String> {
    let code = match language {
        Language::Python => gen_python(adapter, config),
        Language::Node => gen_node(adapter, config),
        Language::Ruby => gen_ruby(adapter, config),
        Language::Php => gen_php(adapter, config),
        Language::Elixir => gen_elixir(adapter, config),
        Language::Wasm => gen_wasm(adapter, config),
        Language::Ffi => gen_ffi(adapter, config),
        Language::Go => gen_go(adapter, config),
        Language::Java => gen_java(adapter, config),
        Language::Csharp => gen_csharp(adapter, config),
        Language::R => gen_r(adapter, config),
    };
    Ok(code)
}

/// Build the parameter list for Rust-side function signatures.
fn rust_params(adapter: &AdapterConfig) -> Vec<String> {
    adapter
        .params
        .iter()
        .map(|p| {
            let ty = if p.optional {
                format!("Option<{}>", p.ty)
            } else {
                p.ty.clone()
            };
            format!("{}: {}", p.name, ty)
        })
        .collect()
}

/// Build the call arguments with `.into()` conversion.
fn call_args(adapter: &AdapterConfig) -> Vec<String> {
    adapter
        .params
        .iter()
        .map(|p| {
            if p.optional {
                format!("{}.map(Into::into)", p.name)
            } else {
                format!("{}.into()", p.name)
            }
        })
        .collect()
}

/// Build the pyo3 signature defaults string.
fn pyo3_sig_defaults(adapter: &AdapterConfig) -> String {
    adapter
        .params
        .iter()
        .map(|p| {
            if p.optional {
                format!("{}=None", p.name)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Python (PyO3)
// ---------------------------------------------------------------------------

fn gen_python(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");
    let gil_release = adapter.gil_release;

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = if params.is_empty() {
        String::new()
    } else {
        format!(", {}", params.join(", "))
    };
    let call_str = args.join(", ");

    let body = if gil_release {
        format!(
            "py.allow_threads(|| {{\n        \
                 {core_path}({call_str})\n            \
                 .map({returns}::from)\n            \
                 .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))\n    \
             }})"
        )
    } else {
        format!(
            "{core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))"
        )
    };

    let sig_defaults = pyo3_sig_defaults(adapter);

    format!(
        "#[pyfunction]\n\
         #[pyo3(signature = ({sig_defaults}))]\n\
         pub fn {name}(py: Python<'_>{param_str}) -> PyResult<{returns}> {{\n    \
             {body}\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Node (NAPI)
// ---------------------------------------------------------------------------

fn gen_node(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "#[napi]\n\
         pub fn {name}({param_str}) -> napi::Result<{returns}> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| napi::Error::from_reason(e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Ruby (Magnus)
// ---------------------------------------------------------------------------

fn gen_ruby(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "pub fn {name}({param_str}) -> Result<{returns}, magnus::Error> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| magnus::Error::new(magnus::exception::runtime_error(), e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// PHP (ext-php-rs)
// ---------------------------------------------------------------------------

fn gen_php(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "#[php_function]\n\
         pub fn {name}({param_str}) -> PhpResult<{returns}> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| PhpException::default(e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Elixir (Rustler)
// ---------------------------------------------------------------------------

fn gen_elixir(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "#[rustler::nif]\n\
         pub fn {name}({param_str}) -> Result<{returns}, String> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| e.to_string())\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// WASM (wasm-bindgen)
// ---------------------------------------------------------------------------

fn gen_wasm(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("JsValue");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "#[wasm_bindgen]\n\
         pub fn {name}({param_str}) -> Result<{returns}, JsValue> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| JsValue::from_str(&e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// FFI (C ABI)
// ---------------------------------------------------------------------------

fn gen_ffi(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let prefix = config.ffi_prefix();

    let c_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            // For FFI, strings become *const c_char, everything else stays
            if p.ty == "String" || p.ty == "&str" {
                format!("{}: *const std::os::raw::c_char", p.name)
            } else {
                format!("{}: {}", p.name, p.ty)
            }
        })
        .collect();

    let conversions: Vec<String> = adapter
        .params
        .iter()
        .filter_map(|p| {
            if p.ty == "String" || p.ty == "&str" {
                Some(format!(
                    "    let {name} = unsafe {{ std::ffi::CStr::from_ptr({name}) }}\n        \
                     .to_str()\n        \
                     .unwrap_or_default()\n        \
                     .to_owned();",
                    name = p.name
                ))
            } else {
                None
            }
        })
        .collect();

    let call_args: Vec<String> = adapter.params.iter().map(|p| p.name.clone()).collect();

    let param_str = c_params.join(", ");
    let call_str = call_args.join(", ");
    let conversion_block = if conversions.is_empty() {
        String::new()
    } else {
        format!("{}\n", conversions.join("\n"))
    };

    format!(
        "#[unsafe(no_mangle)]\n\
         pub extern \"C\" fn {prefix}_{name}({param_str}) -> *mut std::os::raw::c_char {{\n\
         {conversion_block}\
             match {core_path}({call_str}) {{\n        \
                 Ok(result) => {{\n            \
                     let json = serde_json::to_string(&result).unwrap_or_default();\n            \
                     std::ffi::CString::new(json).unwrap_or_default().into_raw()\n        \
                 }}\n        \
                 Err(e) => {{\n            \
                     update_last_error(e);\n            \
                     std::ptr::null_mut()\n        \
                 }}\n    \
             }}\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Go (wraps C FFI)
// ---------------------------------------------------------------------------

fn gen_go(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let prefix = config.ffi_prefix();
    let returns = adapter.returns.as_deref().unwrap_or("string");

    // PascalCase for Go exported name
    let go_name = to_pascal_case(name);

    let go_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            let go_ty = rust_type_to_go(&p.ty);
            format!("{} {}", p.name, go_ty)
        })
        .collect();

    let c_call_args: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            if p.ty == "String" || p.ty == "&str" {
                format!("C.CString({})", p.name)
            } else {
                format!("C.{}({})", rust_type_to_c_go(&p.ty), p.name)
            }
        })
        .collect();

    let param_str = go_params.join(", ");
    let call_str = c_call_args.join(", ");

    format!(
        "// {go_name} calls the {name} adapter via FFI.\n\
         func {go_name}({param_str}) (*{returns}, error) {{\n    \
             result := C.{prefix}_{name}({call_str})\n    \
             if result == nil {{\n        \
                 return nil, fmt.Errorf(\"%s\", lastError())\n    \
             }}\n    \
             defer C.free(unsafe.Pointer(result))\n    \
             var out {returns}\n    \
             if err := json.Unmarshal([]byte(C.GoString(result)), &out); err != nil {{\n        \
                 return nil, err\n    \
             }}\n    \
             return &out, nil\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Java (Panama FFI)
// ---------------------------------------------------------------------------

fn gen_java(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let prefix = config.ffi_prefix();
    let returns = adapter.returns.as_deref().unwrap_or("String");

    // PascalCase for Java method name convention is camelCase
    let java_name = to_camel_case(name);

    let java_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            let java_ty = rust_type_to_java(&p.ty);
            format!("{} {}", java_ty, p.name)
        })
        .collect();

    let param_str = java_params.join(", ");

    format!(
        "    /**\n\
         \x20    * Calls the {name} adapter via FFI.\n\
         \x20    */\n\
         \x20   public static {returns} {java_name}({param_str}) throws Exception {{\n\
         \x20       try (var arena = Arena.ofConfined()) {{\n\
         \x20           var result = (MemorySegment) {prefix}_{name}.invokeExact(arena{arg_pass});\n\
         \x20           if (result.equals(MemorySegment.NULL)) {{\n\
         \x20               throw new RuntimeException(lastError());\n\
         \x20           }}\n\
         \x20           return result.getString(0);\n\
         \x20       }}\n\
         \x20   }}",
        arg_pass = if adapter.params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                adapter
                    .params
                    .iter()
                    .map(|p| {
                        if p.ty == "String" || p.ty == "&str" {
                            format!("arena.allocateFrom({})", p.name)
                        } else {
                            p.name.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    )
}

// ---------------------------------------------------------------------------
// C# (P/Invoke)
// ---------------------------------------------------------------------------

fn gen_csharp(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let prefix = config.ffi_prefix();
    let returns = adapter.returns.as_deref().unwrap_or("string");

    let csharp_name = to_pascal_case(name);

    let csharp_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            let cs_ty = rust_type_to_csharp(&p.ty);
            format!("{} {}", cs_ty, p.name)
        })
        .collect();

    let call_args: Vec<String> = adapter.params.iter().map(|p| p.name.clone()).collect();

    let param_str = csharp_params.join(", ");
    let call_str = call_args.join(", ");

    format!(
        "    [DllImport(LibName, EntryPoint = \"{prefix}_{name}\")]\n\
         \x20   private static extern IntPtr {prefix}_{name}_native({native_params});\n\
         \n\
         \x20   /// <summary>Calls the {name} adapter via FFI.</summary>\n\
         \x20   public static {returns} {csharp_name}({param_str})\n\
         \x20   {{\n\
         \x20       var ptr = {prefix}_{name}_native({call_str});\n\
         \x20       if (ptr == IntPtr.Zero)\n\
         \x20           throw new InvalidOperationException(GetLastError());\n\
         \x20       try {{ return Marshal.PtrToStringUTF8(ptr)!; }}\n\
         \x20       finally {{ FreeString(ptr); }}\n\
         \x20   }}",
        native_params = adapter
            .params
            .iter()
            .map(|p| {
                let cs_ty = if p.ty == "String" || p.ty == "&str" {
                    "string"
                } else {
                    rust_type_to_csharp(&p.ty)
                };
                format!("{} {}", cs_ty, p.name)
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

// ---------------------------------------------------------------------------
// R (extendr)
// ---------------------------------------------------------------------------

fn gen_r(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("Robj");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = params.join(", ");
    let call_str = args.join(", ");

    format!(
        "#[extendr]\n\
         pub fn {name}({param_str}) -> extendr_api::Result<{returns}> {{\n    \
             {core_path}({call_str})\n        \
             .map({returns}::from)\n        \
             .map_err(|e| extendr_api::Error::Other(e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

fn to_camel_case(s: &str) -> String {
    let pascal = to_pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().to_string() + chars.as_str(),
    }
}

fn rust_type_to_go(ty: &str) -> &str {
    match ty {
        "String" | "&str" => "string",
        "bool" => "bool",
        "i32" => "int32",
        "i64" => "int64",
        "u32" => "uint32",
        "u64" => "uint64",
        "f32" => "float32",
        "f64" => "float64",
        "usize" => "uint",
        _ => "string",
    }
}

fn rust_type_to_c_go(ty: &str) -> &str {
    match ty {
        "bool" => "int",
        "i32" => "int",
        "i64" => "longlong",
        "u32" => "uint",
        "u64" => "ulonglong",
        "f32" => "float",
        "f64" => "double",
        _ => "int",
    }
}

fn rust_type_to_java(ty: &str) -> &str {
    match ty {
        "String" | "&str" => "String",
        "bool" => "boolean",
        "i32" => "int",
        "i64" => "long",
        "u32" => "int",
        "u64" => "long",
        "f32" => "float",
        "f64" => "double",
        _ => "String",
    }
}

fn rust_type_to_csharp(ty: &str) -> &str {
    match ty {
        "String" | "&str" => "string",
        "bool" => "bool",
        "i32" => "int",
        "i64" => "long",
        "u32" => "uint",
        "u64" => "ulong",
        "f32" => "float",
        "f64" => "double",
        _ => "string",
    }
}
