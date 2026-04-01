use skif_core::config::{AdapterConfig, Language, SkifConfig};

/// Generate an async method adapter for the given language.
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

/// Build conversion let-bindings for core types (used in Python async).
fn core_let_bindings(adapter: &AdapterConfig, core_import: &str) -> Vec<String> {
    adapter
        .params
        .iter()
        .map(|p| {
            if p.optional {
                format!(
                    "    let core_{name} = {name}.map(|v| -> {core_import}::{ty} {{ v.into() }});",
                    name = p.name,
                    core_import = core_import,
                    ty = p.ty,
                )
            } else {
                format!(
                    "    let core_{name}: {core_import}::{ty} = {name}.into();",
                    name = p.name,
                    core_import = core_import,
                    ty = p.ty,
                )
            }
        })
        .collect()
}

/// Build core call arguments (prefixed with core_).
fn core_call_args(adapter: &AdapterConfig) -> Vec<String> {
    adapter.params.iter().map(|p| format!("core_{}", p.name)).collect()
}

// ---------------------------------------------------------------------------
// Python (PyO3)
// ---------------------------------------------------------------------------

fn gen_python(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let returns = adapter.returns.as_deref().unwrap_or("()");
    let core_import = config.core_import();

    let params = rust_params(adapter);
    let let_bindings = core_let_bindings(adapter, &core_import);
    let core_args = core_call_args(adapter);

    let param_str = if params.is_empty() {
        String::new()
    } else {
        format!(", {}", params.join(", "))
    };
    let core_call_str = core_args.join(", ");

    let bindings_block = if let_bindings.is_empty() {
        String::new()
    } else {
        format!("{}\n", let_bindings.join("\n"))
    };

    format!(
        "// Method on {owner_type} impl block\n\
         pub fn {name}<'py>(&self, py: Python<'py>{param_str}) -> PyResult<Bound<'py, PyAny>> {{\n    \
             let inner = self.inner.clone();\n\
         {bindings_block}\
             pyo3_async_runtimes::tokio::future_into_py(py, async move {{\n        \
                 let result = inner.{core_path}({core_call_str}).await\n            \
                     .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;\n        \
                 Ok({returns}::from(result))\n    \
             }})\n\
         }}",
        owner_type = adapter.owner_type.as_deref().unwrap_or("Self"),
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

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    let js_name = to_camel_case(name);

    format!(
        "#[napi(js_name = \"{js_name}\")]\n\
         pub async fn {name}({param_str}) -> napi::Result<{returns}> {{\n    \
             let core_req = {call_str};\n    \
             self.inner.{core_path}(core_req).await\n        \
             .map({returns}::from)\n        \
             .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))\n\
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

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "fn {name}({param_str}) -> Result<{returns}, magnus::Error> {{\n    \
             let rt = tokio::runtime::Runtime::new()\n        \
                 .map_err(|e| magnus::Error::new(magnus::exception::runtime_error(), e.to_string()))?;\n    \
             let core_req = {call_str};\n    \
             rt.block_on(async {{ self.inner.{core_path}(core_req).await }})\n        \
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

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "pub fn {name}({param_str}) -> PhpResult<{returns}> {{\n    \
             WORKER_RUNTIME.block_on(async {{\n        \
                 self.inner.{core_path}({call_str}.into()).await\n    \
             }})\n    \
             .map({returns}::from)\n    \
             .map_err(|e| ext_php_rs::exception::PhpException::default(e.to_string()).into())\n\
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
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Self");
    let owner_snake = to_snake_case(owner_type);

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = if params.is_empty() {
        format!("client: ResourceArc<{owner_type}>")
    } else {
        format!("client: ResourceArc<{owner_type}>, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "#[rustler::nif(schedule = \"DirtyCpu\")]\n\
         fn {owner_snake}_{name}({param_str}) -> Result<{returns}, String> {{\n    \
             let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;\n    \
             rt.block_on(async {{ client.inner.{core_path}({call_str}).await }})\n        \
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

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    let js_name = to_camel_case(name);

    format!(
        "#[wasm_bindgen(js_name = \"{js_name}\")]\n\
         pub async fn {name}({param_str}) -> Result<{returns}, JsValue> {{\n    \
             self.inner.{core_path}({call_str}).await\n        \
             .map({returns}::from)\n        \
             .map_err(|e| JsValue::from_str(&e.to_string()))\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// FFI (C ABI) — async becomes sync via block_on
// ---------------------------------------------------------------------------

fn gen_ffi(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let prefix = config.ffi_prefix();
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Self");
    let owner_snake = to_snake_case(owner_type);

    let json_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            if p.ty == "String" || p.ty == "&str" {
                format!("{}: *const std::os::raw::c_char", p.name)
            } else {
                format!("{}_json: *const std::os::raw::c_char", p.name)
            }
        })
        .collect();

    let mut all_params = vec![format!("client: *const {owner_type}")];
    all_params.extend(json_params);

    let conversions: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            if p.ty == "String" || p.ty == "&str" {
                format!(
                    "    let {name} = unsafe {{ std::ffi::CStr::from_ptr({name}) }}\n        \
                     .to_str()\n        \
                     .unwrap_or_default()\n        \
                     .to_owned();",
                    name = p.name,
                )
            } else {
                format!(
                    "    let {name}_str = unsafe {{ std::ffi::CStr::from_ptr({name}_json) }}\n        \
                     .to_str()\n        \
                     .unwrap_or_default();\n    \
                     let {name}: {ty} = match serde_json::from_str({name}_str) {{\n        \
                         Ok(v) => v,\n        \
                         Err(e) => {{\n            \
                             update_last_error(e);\n            \
                             return std::ptr::null_mut();\n        \
                         }}\n    \
                     }};",
                    name = p.name,
                    ty = p.ty,
                )
            }
        })
        .collect();

    let call_args: Vec<String> = adapter.params.iter().map(|p| p.name.clone()).collect();

    let param_str = all_params.join(",\n    ");
    let call_str = call_args.join(", ");
    let conversion_block = if conversions.is_empty() {
        String::new()
    } else {
        format!("{}\n", conversions.join("\n"))
    };

    format!(
        "#[unsafe(no_mangle)]\n\
         pub extern \"C\" fn {prefix}_{owner_snake}_{name}(\n    {param_str},\n) -> *mut std::os::raw::c_char {{\n    \
             let client = unsafe {{ &*client }};\n\
         {conversion_block}\
             let rt = match tokio::runtime::Runtime::new() {{\n        \
                 Ok(rt) => rt,\n        \
                 Err(e) => {{\n            \
                     update_last_error(e);\n            \
                     return std::ptr::null_mut();\n        \
                 }}\n    \
             }};\n    \
             match rt.block_on(async {{ client.inner.{core_path}({call_str}).await }}) {{\n        \
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
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Client");

    let go_name = to_pascal_case(name);
    let owner_snake = to_snake_case(owner_type);

    let go_params: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            let go_ty = rust_type_to_go(&p.ty);
            format!("{} {}", p.name, go_ty)
        })
        .collect();

    let marshal_block: Vec<String> = adapter
        .params
        .iter()
        .filter(|p| p.ty != "String" && p.ty != "&str")
        .map(|p| {
            format!(
                "    {name}JSON, err := json.Marshal({name})\n    \
                 if err != nil {{\n        \
                     return nil, err\n    \
                 }}",
                name = p.name,
            )
        })
        .collect();

    let c_call_args: Vec<String> = adapter
        .params
        .iter()
        .map(|p| {
            if p.ty == "String" || p.ty == "&str" {
                format!("C.CString({})", p.name)
            } else {
                format!("C.CString(string({name}JSON))", name = p.name)
            }
        })
        .collect();

    let param_str = go_params.join(", ");
    let call_str = c_call_args.join(", ");
    let marshal_str = if marshal_block.is_empty() {
        String::new()
    } else {
        format!("{}\n", marshal_block.join("\n"))
    };

    format!(
        "// {go_name} calls the {name} async method via FFI.\n\
         func (c *{owner_type}) {go_name}({param_str}) (*{returns}, error) {{\n\
         {marshal_str}\
             result := C.{prefix}_{owner_snake}_{name}(c.ptr, {call_str})\n    \
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
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Client");
    let owner_snake = to_snake_case(owner_type);

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
         \x20    * Calls the {name} async method via FFI.\n\
         \x20    */\n\
         \x20   public {returns} {java_name}({param_str}) throws Exception {{\n\
         \x20       try (var arena = Arena.ofConfined()) {{\n\
         \x20           var result = (MemorySegment) {prefix}_{owner_snake}_{name}.invokeExact(this.handle, arena{arg_pass});\n\
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
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Client");
    let owner_snake = to_snake_case(owner_type);

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
        "    [DllImport(LibName, EntryPoint = \"{prefix}_{owner_snake}_{name}\")]\n\
         \x20   private static extern IntPtr {prefix}_{owner_snake}_{name}_native(IntPtr client{native_params});\n\
         \n\
         \x20   /// <summary>Calls the {name} async method via FFI.</summary>\n\
         \x20   public {returns} {csharp_name}({param_str})\n\
         \x20   {{\n\
         \x20       var ptr = {prefix}_{owner_snake}_{name}_native(this.handle{call_pass});\n\
         \x20       if (ptr == IntPtr.Zero)\n\
         \x20           throw new InvalidOperationException(GetLastError());\n\
         \x20       try {{ return Marshal.PtrToStringUTF8(ptr)!; }}\n\
         \x20       finally {{ FreeString(ptr); }}\n\
         \x20   }}",
        native_params = if adapter.params.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                adapter
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
        },
        call_pass = if call_str.is_empty() {
            String::new()
        } else {
            format!(", {}", call_str)
        },
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

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "#[extendr]\n\
         fn {name}({param_str}) -> extendr_api::Result<{returns}> {{\n    \
             let rt = tokio::runtime::Runtime::new()\n        \
                 .map_err(|e| extendr_api::Error::Other(e.to_string()))?;\n    \
             rt.block_on(async {{ self.inner.{core_path}({call_str}).await }})\n        \
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

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
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
