use skif_core::backend::Backend;
use skif_core::config::Language;

/// Get the backend for a given language.
pub fn get_backend(lang: Language) -> Box<dyn Backend> {
    match lang {
        Language::Python => Box::new(skif_backend_pyo3::Pyo3Backend),
        Language::Node => Box::new(skif_backend_napi::NapiBackend),
        Language::Ruby => Box::new(skif_backend_magnus::MagnusBackend),
        Language::Php => Box::new(skif_backend_php::PhpBackend),
        Language::Elixir => Box::new(skif_backend_rustler::RustlerBackend),
        Language::Wasm => Box::new(skif_backend_wasm::WasmBackend),
        Language::Ffi => Box::new(skif_backend_ffi::FfiBackend),
        Language::Go => Box::new(skif_backend_go::GoBackend),
        Language::Java => Box::new(skif_backend_java::JavaBackend),
        Language::Csharp => Box::new(skif_backend_csharp::CsharpBackend),
    }
}
