use skif_core::config::{AdapterConfig, Language, SkifConfig};

/// Generate a streaming adapter for the given language.
pub fn generate(adapter: &AdapterConfig, language: Language, config: &SkifConfig) -> anyhow::Result<String> {
    let code = match language {
        Language::Python => gen_python(adapter, config),
        Language::Node => gen_node(adapter, config),
        Language::Ruby => gen_ruby(adapter, config),
        Language::Php => gen_php(adapter, config),
        Language::Elixir => gen_elixir(adapter, config),
        Language::Wasm => gen_wasm(adapter, config),
        Language::Ffi => gen_ffi(adapter),
        Language::Go => gen_go(adapter),
        Language::Java => gen_java(adapter),
        Language::Csharp => gen_csharp(adapter),
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

/// Get the iterator struct name from the adapter name.
fn iterator_name(adapter: &AdapterConfig) -> String {
    to_pascal_case(&adapter.name) + "Iterator"
}

// ---------------------------------------------------------------------------
// Python (PyO3)
// ---------------------------------------------------------------------------

fn gen_python(adapter: &AdapterConfig, config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let item_type = adapter.item_type.as_deref().unwrap_or("()");
    let error_type = adapter.error_type.as_deref().unwrap_or("anyhow::Error");
    let owner_type = adapter.owner_type.as_deref().unwrap_or("Self");
    let core_import = config.core_import();
    let iter_name = iterator_name(adapter);

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = if params.is_empty() {
        String::new()
    } else {
        format!(", {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "#[pyclass]\n\
         pub struct {iter_name} {{\n    \
             inner: Arc<tokio::sync::Mutex<futures::stream::BoxStream<'static, Result<{core_import}::{item_type}, {core_import}::{error_type}>>>>,\n\
         }}\n\
         \n\
         #[pymethods]\n\
         impl {iter_name} {{\n    \
             fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {{ slf }}\n\
             \n    \
             fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {{\n        \
                 let inner = self.inner.clone();\n        \
                 pyo3_async_runtimes::tokio::future_into_py(py, async move {{\n            \
                     let mut stream = inner.lock().await;\n            \
                     match futures::StreamExt::next(&mut *stream).await {{\n                \
                         Some(Ok(chunk)) => Ok(Some({item_type}::from(chunk))),\n                \
                         Some(Err(e)) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),\n                \
                         None => Ok(None),  // StopAsyncIteration\n            \
                     }}\n        \
                 }})\n    \
             }}\n\
         }}\n\
         \n\
         // Method on {owner_type} impl block\n\
         pub fn {name}<'py>(&self, py: Python<'py>{param_str}) -> PyResult<{iter_name}> {{\n    \
             let inner = self.inner.clone();\n    \
             let stream = inner.{core_path}({call_str});\n    \
             Ok({iter_name} {{\n        \
                 inner: Arc::new(tokio::sync::Mutex::new(stream)),\n    \
             }})\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Node (NAPI)
// ---------------------------------------------------------------------------

fn gen_node(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let item_type = adapter.item_type.as_deref().unwrap_or("()");

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
         pub async fn {name}({param_str}) -> napi::Result<Vec<{item_type}>> {{\n    \
             use futures::StreamExt;\n    \
             let stream = self.inner.{core_path}({call_str});\n    \
             let chunks: Vec<_> = stream\n        \
                 .map(|r| r.map({item_type}::from))\n        \
                 .collect::<Vec<_>>().await\n        \
                 .into_iter()\n        \
                 .collect::<Result<Vec<_>, _>>()\n        \
                 .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;\n    \
             Ok(chunks)\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// Ruby (Magnus)
// ---------------------------------------------------------------------------

fn gen_ruby(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let item_type = adapter.item_type.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "fn {name}({param_str}) -> Result<Vec<{item_type}>, magnus::Error> {{\n    \
             use futures::StreamExt;\n    \
             let rt = tokio::runtime::Runtime::new()\n        \
                 .map_err(|e| magnus::Error::new(magnus::exception::runtime_error(), e.to_string()))?;\n    \
             let stream = self.inner.{core_path}({call_str});\n    \
             rt.block_on(async {{\n        \
                 stream\n            \
                     .map(|r| r.map({item_type}::from))\n            \
                     .collect::<Vec<_>>().await\n            \
                     .into_iter()\n            \
                     .collect::<Result<Vec<_>, _>>()\n    \
             }})\n    \
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
    let item_type = adapter.item_type.as_deref().unwrap_or("()");

    let params = rust_params(adapter);
    let args = call_args(adapter);

    let param_str = if params.is_empty() {
        String::from("&self")
    } else {
        format!("&self, {}", params.join(", "))
    };
    let call_str = args.join(", ");

    format!(
        "pub fn {name}({param_str}) -> PhpResult<Vec<{item_type}>> {{\n    \
             use futures::StreamExt;\n    \
             WORKER_RUNTIME.block_on(async {{\n        \
                 let stream = self.inner.{core_path}({call_str});\n        \
                 stream\n            \
                     .map(|r| r.map({item_type}::from))\n            \
                     .collect::<Vec<_>>().await\n            \
                     .into_iter()\n            \
                     .collect::<Result<Vec<_>, _>>()\n    \
             }})\n    \
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
    let item_type = adapter.item_type.as_deref().unwrap_or("()");
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
         fn {owner_snake}_{name}({param_str}) -> Result<Vec<{item_type}>, String> {{\n    \
             use futures::StreamExt;\n    \
             let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;\n    \
             let stream = client.inner.{core_path}({call_str});\n    \
             rt.block_on(async {{\n        \
                 stream\n            \
                     .map(|r| r.map({item_type}::from))\n            \
                     .collect::<Vec<_>>().await\n            \
                     .into_iter()\n            \
                     .collect::<Result<Vec<_>, _>>()\n    \
             }})\n    \
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
    let item_type = adapter.item_type.as_deref().unwrap_or("JsValue");

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
         pub async fn {name}({param_str}) -> Result<Vec<{item_type}>, JsValue> {{\n    \
             use futures::StreamExt;\n    \
             let stream = self.inner.{core_path}({call_str});\n    \
             let chunks: Vec<_> = stream\n        \
                 .map(|r| r.map({item_type}::from))\n        \
                 .collect::<Vec<_>>().await\n        \
                 .into_iter()\n        \
                 .collect::<Result<Vec<_>, _>>()\n        \
                 .map_err(|e| JsValue::from_str(&e.to_string()))?;\n    \
             Ok(chunks)\n\
         }}"
    )
}

// ---------------------------------------------------------------------------
// FFI (C ABI) — Streaming not supported
// ---------------------------------------------------------------------------

fn gen_ffi(adapter: &AdapterConfig) -> String {
    format!(
        "// Streaming not supported via FFI. Use the Rust API directly.\n\
         // Adapter: {}",
        adapter.name,
    )
}

// ---------------------------------------------------------------------------
// Go — Streaming not supported via FFI
// ---------------------------------------------------------------------------

fn gen_go(adapter: &AdapterConfig) -> String {
    format!(
        "// Streaming not supported via FFI. Use the Rust API directly.\n\
         // Adapter: {}",
        adapter.name,
    )
}

// ---------------------------------------------------------------------------
// Java — Streaming not supported via FFI
// ---------------------------------------------------------------------------

fn gen_java(adapter: &AdapterConfig) -> String {
    format!(
        "// Streaming not supported via FFI. Use the Rust API directly.\n\
         // Adapter: {}",
        adapter.name,
    )
}

// ---------------------------------------------------------------------------
// C# — Streaming not supported via FFI
// ---------------------------------------------------------------------------

fn gen_csharp(adapter: &AdapterConfig) -> String {
    format!(
        "// Streaming not supported via FFI. Use the Rust API directly.\n\
         // Adapter: {}",
        adapter.name,
    )
}

// ---------------------------------------------------------------------------
// R (extendr) — collect stream into Vec
// ---------------------------------------------------------------------------

fn gen_r(adapter: &AdapterConfig, _config: &SkifConfig) -> String {
    let name = &adapter.name;
    let core_path = &adapter.core_path;
    let item_type = adapter.item_type.as_deref().unwrap_or("Robj");

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
         fn {name}({param_str}) -> extendr_api::Result<Vec<{item_type}>> {{\n    \
             use futures::StreamExt;\n    \
             let rt = tokio::runtime::Runtime::new()\n        \
                 .map_err(|e| extendr_api::Error::Other(e.to_string()))?;\n    \
             let stream = self.inner.{core_path}({call_str});\n    \
             rt.block_on(async {{\n        \
                 stream\n            \
                     .map(|r| r.map({item_type}::from))\n            \
                     .collect::<Vec<_>>().await\n            \
                     .into_iter()\n            \
                     .collect::<Result<Vec<_>, _>>()\n    \
             }})\n    \
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
