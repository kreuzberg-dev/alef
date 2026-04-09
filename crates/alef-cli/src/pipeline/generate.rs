use alef_core::backend::GeneratedFile;
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;
use anyhow::Context as _;
use std::path::Path;
use tracing::{debug, info};

use crate::cache;
use crate::registry;

/// Generate bindings for given languages.
pub fn generate(
    api: &ApiSurface,
    config: &AlefConfig,
    languages: &[Language],
    clean: bool,
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
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

        let files = backend
            .generate_bindings(api, config)
            .with_context(|| format!("failed to generate bindings for {lang_str}"))?;
        cache::write_lang_hash(&lang_str, &lang_hash)
            .with_context(|| format!("failed to write language hash for {lang_str}"))?;
        results.push((lang, files));
    }

    Ok(results)
}

/// Generate type stubs for given languages.
pub fn generate_stubs(
    api: &ApiSurface,
    config: &AlefConfig,
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

/// Generate public API wrappers for given languages.
pub fn generate_public_api(
    api: &ApiSurface,
    config: &AlefConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<(Language, Vec<GeneratedFile>)>> {
    let mut results = vec![];
    for &lang in languages {
        let backend = registry::get_backend(lang);
        let files = backend.generate_public_api(api, config)?;
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
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory {}", parent.display()))?;
            }
            std::fs::write(&full_path, &file.content)
                .with_context(|| format!("failed to write generated file {}", full_path.display()))?;
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

/// Generate scaffold files for given languages.
pub fn scaffold(api: &ApiSurface, config: &AlefConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    alef_scaffold::scaffold(api, config, languages)
}

/// Generate README files for given languages.
pub fn readme(api: &ApiSurface, config: &AlefConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    alef_readme::generate_readmes(api, config, languages)
}

/// Write standalone generated files (not grouped by language) to disk.
pub fn write_scaffold_files(files: &[GeneratedFile], base_dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for file in files {
        let full_path = base_dir.join(&file.path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        std::fs::write(&full_path, &file.content)
            .with_context(|| format!("failed to write generated file {}", full_path.display()))?;
        count += 1;
        debug!("  wrote: {}", full_path.display());
    }
    Ok(count)
}

/// Auto-format generated Rust files using `cargo fmt` (best-effort, doesn't fail on error).
pub fn format_rust_files(files: &[(Language, Vec<GeneratedFile>)], base_dir: &Path) {
    let rs_files: Vec<_> = files
        .iter()
        .flat_map(|(_, lang_files)| lang_files.iter())
        .filter(|f| f.path.extension().is_some_and(|ext| ext == "rs"))
        .map(|f| base_dir.join(&f.path))
        .collect();

    if rs_files.is_empty() {
        return;
    }

    // Run rustfmt on each file individually (more reliable than cargo fmt for specific files)
    for path in &rs_files {
        let result = std::process::Command::new("rustfmt")
            .arg("--edition")
            .arg("2024")
            .arg(path)
            .output();
        match result {
            Ok(output) if !output.status.success() => {
                debug!(
                    "rustfmt warning on {}: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Err(e) => {
                debug!("rustfmt not available: {e}");
                return; // Don't try other files if rustfmt isn't installed
            }
            _ => {}
        }
    }
}
