pub mod sync_function;

use skif_core::config::{AdapterPattern, Language, SkifConfig};

/// Generate adapter code for a specific language.
pub fn generate_adapters(config: &SkifConfig, language: Language) -> anyhow::Result<Vec<String>> {
    let mut code_blocks = vec![];

    for adapter in &config.adapters {
        let code = match adapter.pattern {
            AdapterPattern::SyncFunction => sync_function::generate(adapter, language, config),
            AdapterPattern::AsyncMethod => todo!("Phase 2"),
            AdapterPattern::CallbackBridge => todo!("Phase 3"),
            AdapterPattern::Streaming => todo!("Phase 2"),
            AdapterPattern::ServerLifecycle => todo!("Phase 3"),
        }?;

        if !code.is_empty() {
            code_blocks.push(code);
        }
    }

    Ok(code_blocks)
}
