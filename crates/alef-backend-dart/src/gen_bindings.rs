use alef_core::backend::{Backend, BuildConfig, BuildDependency, Capabilities, GeneratedFile};
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;

pub struct DartBackend;

impl Backend for DartBackend {
    fn name(&self) -> &str {
        "dart"
    }

    fn language(&self) -> Language {
        Language::Dart
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
        // Phase 2A skeleton: real codegen lands in Phase 2B.
        Ok(vec![])
    }

    fn build_config(&self) -> Option<BuildConfig> {
        Some(BuildConfig {
            tool: "flutter_rust_bridge_codegen",
            crate_suffix: "-dart",
            build_dep: BuildDependency::None,
            post_build: vec![],
        })
    }
}
