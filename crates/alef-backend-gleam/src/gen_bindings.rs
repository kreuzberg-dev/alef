use alef_core::backend::{Backend, BuildConfig, BuildDependency, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;

pub struct GleamBackend;

impl Backend for GleamBackend {
    fn name(&self) -> &str {
        "gleam"
    }

    fn language(&self) -> Language {
        Language::Gleam
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: false,
            supports_classes: false,
            supports_enums: true,
            supports_option: true,
            supports_result: true,
            supports_callbacks: false,
            supports_streaming: false,
        }
    }

    fn generate_bindings(&self, _api: &ApiSurface, _config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
        Ok(vec![])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "gleam",
            crate_suffix: "",
            build_dep: BuildDependency::Rustler,
            post_build: vec![],
        })
    }
}
