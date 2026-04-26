use alef_core::backend::{Backend, BuildConfig, BuildDependency, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;

pub struct SwiftBackend;

impl Backend for SwiftBackend {
    fn name(&self) -> &str {
        "swift"
    }

    fn language(&self) -> Language {
        Language::Swift
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_async: true,
            supports_classes: true,
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
            tool: "swift",
            crate_suffix: "-swift",
            build_dep: BuildDependency::None,
            post_build: vec![],
        })
    }
}
