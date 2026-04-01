use skif_core::backend::GeneratedFile;
use skif_core::config::{Language, SkifConfig};
use skif_core::ir::ApiSurface;
use std::path::Path;

use crate::cache;
use crate::registry;
use tracing::{debug, info};

/// Run extraction, with caching.
pub fn extract(config: &SkifConfig, config_path: &Path, clean: bool) -> anyhow::Result<ApiSurface> {
    let source_hash = cache::compute_source_hash(&config.crate_config.sources, config_path)?;

    if !clean && cache::is_ir_cached(&source_hash) {
        info!("Using cached IR");
        return cache::read_cached_ir();
    }

    info!("Extracting API surface from Rust source...");
    let sources: Vec<&Path> = config.crate_config.sources.iter().map(|p| p.as_path()).collect();

    // Read version from Cargo.toml
    let version = read_version(&config.crate_config.version_from)?;

    let api = skif_extract::extractor::extract(&sources, &config.crate_config.name, &version)?;

    // Apply global exclusions
    let api = apply_exclusions(api, &config.exclude);

    cache::write_ir_cache(&api, &source_hash)?;
    info!(
        "Extracted {} types, {} functions, {} enums",
        api.types.len(),
        api.functions.len(),
        api.enums.len()
    );

    Ok(api)
}

/// Generate bindings for given languages.
pub fn generate(
    api: &ApiSurface,
    config: &SkifConfig,
    languages: &[Language],
    clean: bool,
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let ir_json = serde_json::to_string(api)?;
    let config_toml = toml::to_string(config).unwrap_or_default();
    let mut results = vec![];

    for &lang in languages {
        let lang_str = lang.to_string();
        let lang_hash = cache::compute_lang_hash(&ir_json, &lang_str, &config_toml);

        if !clean && cache::is_lang_cached(&lang_str, &lang_hash) {
            debug!("  {}: cached, skipping", lang_str);
            continue;
        }

        let backend = registry::get_backend(lang);
        info!("  {}: generating...", lang_str);

        let files = backend.generate_bindings(api, config)?;
        cache::write_lang_hash(&lang_str, &lang_hash)?;
        results.push((lang, files));
    }

    Ok(results)
}

/// Generate type stubs for given languages.
pub fn generate_stubs(
    api: &ApiSurface,
    config: &SkifConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let mut results = vec![];
    for &lang in languages {
        let backend = registry::get_backend(lang);
        let files = backend.generate_type_stubs(api, config)?;
        if !files.is_empty() {
            results.push((lang, files));
        }
    }
    Ok(results)
}

/// Write generated files to disk.
pub fn write_files(files: &[(Language, Vec<GeneratedFile>)], base_dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for (_lang, lang_files) in files {
        for file in lang_files {
            let full_path = base_dir.join(&file.path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, &file.content)?;
            count += 1;
            debug!("  wrote: {}", full_path.display());
        }
    }
    Ok(count)
}

/// Diff generated files against what's on disk.
pub fn diff_files(files: &[(Language, Vec<GeneratedFile>)], base_dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut diffs = vec![];
    for (lang, lang_files) in files {
        for file in lang_files {
            let full_path = base_dir.join(&file.path);
            let existing = std::fs::read_to_string(&full_path).unwrap_or_default();
            if existing != file.content {
                diffs.push(format!("[{lang}] {}", file.path.display()));
            }
        }
    }
    Ok(diffs)
}

fn read_version(version_from: &str) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(version_from)?;
    let value: toml::Value = toml::from_str(&content)?;
    if let Some(v) = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(v.to_string());
    }
    if let Some(v) = value
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
    {
        return Ok(v.to_string());
    }
    anyhow::bail!("Could not find version in {version_from}")
}

fn apply_exclusions(mut api: ApiSurface, exclude: &skif_core::config::ExcludeConfig) -> ApiSurface {
    api.types.retain(|t| !exclude.types.contains(&t.name));
    api.functions.retain(|f| !exclude.functions.contains(&f.name));
    api.enums.retain(|e| !exclude.types.contains(&e.name));
    api.errors.retain(|e| !exclude.types.contains(&e.name));
    api
}
