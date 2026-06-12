use super::validation::validate_generation_api;
use crate::cli::{cache, registry};
use crate::core::backend::GeneratedFile;
use crate::core::config::{Language, ResolvedCrateConfig};
use crate::core::ir::ApiSurface;
use anyhow::Context as _;
use rayon::prelude::*;
use tracing::{debug, info};

pub fn generate(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
    clean: bool,
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let validated_api = validate_generation_api(api, config, languages)?;

    // Validate that Go/Java/C# have FFI in the languages list
    let has_ffi = languages.contains(&Language::Ffi);
    for &lang in languages {
        if (lang == Language::Go || lang == Language::Java || lang == Language::Csharp) && !has_ffi {
            tracing::warn!(
                "Language {:?} requires FFI to be in the languages list for proper code generation",
                lang
            );
        }
    }

    let ir_json = serde_json::to_string(api)?;
    let config_toml =
        toml::to_string(config).with_context(|| "failed to serialize resolved crate config for cache key")?;

    let to_generate: Vec<_> = languages
        .par_iter()
        .filter_map(|&lang| {
            let lang_str = lang.to_string();
            let lang_hash = cache::compute_lang_hash(&ir_json, &lang_str, &config_toml);

            if !clean && cache::is_lang_cached(&config.name, &lang_str, &lang_hash) {
                debug!("  {}: cached, skipping", lang_str);
                return None;
            }

            Some((lang, lang_str, lang_hash))
        })
        .collect();

    let results: Vec<(Language, Vec<GeneratedFile>)> = to_generate
        .par_iter()
        .map(|(lang, lang_str, lang_hash)| {
            let backend = registry::get_backend(*lang);
            info!("  {}: generating...", lang_str);

            let files = backend
                .generate_bindings_checked(validated_api, config)
                .with_context(|| format!("failed to generate bindings for {lang_str}"))?;
            let base_dir = std::env::current_dir().unwrap_or_default();
            let output_paths: Vec<std::path::PathBuf> = files.iter().map(|f| base_dir.join(&f.path)).collect();
            cache::write_lang_hash(&config.name, lang_str, lang_hash, &output_paths)
                .with_context(|| format!("failed to write language hash for {lang_str}"))?;
            Ok((*lang, files))
        })
        .collect::<anyhow::Result<_>>()?;

    Ok(results)
}

/// Generate type stubs for given languages.
pub fn generate_stubs(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let validated_api = validate_generation_api(api, config, languages)?;

    let results: Vec<(Language, Vec<GeneratedFile>)> = languages
        .par_iter()
        .map(|&lang| {
            let Some(backend) = registry::try_get_backend(lang) else {
                return Ok((lang, Vec::new()));
            };
            let files = backend.generate_type_stubs_checked(validated_api, config)?;
            Ok((lang, files))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, files)| !files.is_empty())
        .collect();
    Ok(results)
}

/// Generate service API (idiomatic app object + handler bridge) for backends that
/// declare `supports_service_api`.  Only invoked when `api.services` is non-empty.
/// Fails for languages whose backends do not support service API yet.
pub fn generate_service_api(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let validated_api = validate_generation_api(api, config, languages)?;
    let api = validated_api.api();

    if api.services.is_empty() {
        return Ok(vec![]);
    }

    let results: Vec<(Language, Vec<GeneratedFile>)> = languages
        .par_iter()
        .copied()
        .filter(|&lang| {
            registry::try_get_backend(lang).is_some_and(|backend| backend.capabilities().supports_service_api)
        })
        .map(|lang| {
            let backend = registry::get_backend(lang);
            let files = backend.generate_service_api_checked(validated_api, config)?;
            Ok((lang, files))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, files)| !files.is_empty())
        .collect();
    Ok(results)
}

/// Generate public API wrappers for given languages.
pub fn generate_public_api(
    api: &ApiSurface,
    config: &ResolvedCrateConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let validated_api = validate_generation_api(api, config, languages)?;

    let results: Vec<(Language, Vec<GeneratedFile>)> = languages
        .par_iter()
        .map(|&lang| {
            let Some(backend) = registry::try_get_backend(lang) else {
                return Ok((lang, Vec::new()));
            };
            let files = backend.generate_public_api_checked(validated_api, config)?;
            Ok((lang, files))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, files)| !files.is_empty())
        .collect();
    Ok(results)
}
