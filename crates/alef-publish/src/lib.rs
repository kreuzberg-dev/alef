//! Publish pipeline for alef — vendoring, building, and packaging artifacts
//! for distribution across language package registries.
//!
//! This crate provides the local logic behind `alef publish prepare`,
//! `alef publish build`, and `alef publish package`. It does NOT handle
//! registry authentication or publishing — those remain in CI actions.

pub mod platform;

use alef_core::config::AlefConfig;
use alef_core::config::extras::Language;
use alef_core::config::publish::{PublishLanguageConfig, VendorMode};
use anyhow::Result;
use platform::RustTarget;
use std::path::Path;

/// Prepare a language package for publishing: vendor dependencies, stage FFI artifacts.
pub fn prepare(config: &AlefConfig, languages: &[Language], target: Option<&RustTarget>, dry_run: bool) -> Result<()> {
    for &lang in languages {
        let lang_config = publish_config_for_language(config, lang);
        let vendor_mode = lang_config.vendor_mode.unwrap_or_else(|| default_vendor_mode(lang));

        match vendor_mode {
            VendorMode::CoreOnly => {
                let core_crate_dir = resolve_core_crate_dir(config);
                if dry_run {
                    eprintln!("[dry-run] Would vendor core crate from {core_crate_dir} for {lang}");
                } else {
                    eprintln!("Vendoring core crate from {core_crate_dir} for {lang}...");
                    // TODO: Phase 2 — implement vendor_core_only()
                    eprintln!("  vendoring not yet implemented");
                }
            }
            VendorMode::Full => {
                let core_crate_dir = resolve_core_crate_dir(config);
                if dry_run {
                    eprintln!("[dry-run] Would vendor all dependencies from {core_crate_dir} for {lang}");
                } else {
                    eprintln!("Vendoring all dependencies from {core_crate_dir} for {lang}...");
                    // TODO: Phase 2 — implement vendor_full()
                    eprintln!("  full vendoring not yet implemented");
                }
            }
            VendorMode::None => {}
        }

        // Stage FFI artifacts for FFI-dependent languages.
        if is_ffi_dependent(lang) {
            if let Some(target) = target {
                if dry_run {
                    let platform = target.platform_for(lang);
                    eprintln!("[dry-run] Would stage FFI artifacts for {lang} (platform: {platform})");
                } else {
                    eprintln!("Staging FFI artifacts for {lang}...");
                    // TODO: Phase 3 — implement ffi_stage()
                    eprintln!("  FFI staging not yet implemented");
                }
            } else {
                eprintln!("Skipping FFI staging for {lang}: no --target specified");
            }
        }
    }
    Ok(())
}

/// Build release artifacts for a specific platform.
pub fn build(
    _config: &AlefConfig,
    languages: &[Language],
    target: Option<&RustTarget>,
    _use_cross: bool,
) -> Result<()> {
    for &lang in languages {
        let target_str = target.map(|t| t.triple.as_str()).unwrap_or("host");
        eprintln!("Building {lang} for target {target_str}...");
        // TODO: Phase 5 — implement build logic
        eprintln!("  build not yet implemented");
    }
    Ok(())
}

/// Package built artifacts into distributable archives.
pub fn package(
    _config: &AlefConfig,
    languages: &[Language],
    target: Option<&RustTarget>,
    output_dir: &Path,
    _version: &str,
    dry_run: bool,
) -> Result<()> {
    for &lang in languages {
        let platform = target
            .map(|t| t.platform_for(lang))
            .unwrap_or_else(|| "host".to_string());
        if dry_run {
            eprintln!(
                "[dry-run] Would package {lang} for platform {platform} into {}",
                output_dir.display()
            );
        } else {
            eprintln!("Packaging {lang} for platform {platform}...");
            // TODO: Phase 4 — implement per-language packagers
            eprintln!("  packaging not yet implemented");
        }
    }
    Ok(())
}

/// Validate that all package manifests are ready for publishing.
pub fn validate(_config: &AlefConfig) -> Result<Vec<String>> {
    // TODO: Phase 6 — extend verify_versions with file presence checks
    Ok(vec![])
}

/// Get the publish configuration for a language, falling back to defaults.
fn publish_config_for_language(config: &AlefConfig, lang: Language) -> PublishLanguageConfig {
    if let Some(publish) = &config.publish {
        let lang_str = lang.to_string();
        if let Some(lang_config) = publish.languages.get(&lang_str) {
            return lang_config.clone();
        }
    }
    PublishLanguageConfig::default()
}

/// Resolve the core crate directory path.
fn resolve_core_crate_dir(config: &AlefConfig) -> String {
    if let Some(publish) = &config.publish {
        if let Some(core_crate) = &publish.core_crate {
            return core_crate.clone();
        }
    }
    // Fall back to deriving from [crate].sources.
    let dir = config.core_crate_dir();
    if !config.crate_config.sources.is_empty() {
        let first = config.crate_config.sources[0].to_string_lossy();
        if first.contains("crates/") {
            return format!("crates/{dir}");
        }
    }
    dir
}

/// Return the default vendor mode for a language.
fn default_vendor_mode(lang: Language) -> VendorMode {
    match lang {
        Language::Ruby | Language::Elixir => VendorMode::CoreOnly,
        Language::R => VendorMode::Full,
        _ => VendorMode::None,
    }
}

/// Whether a language depends on the C FFI crate for its bindings.
fn is_ffi_dependent(lang: Language) -> bool {
    matches!(lang, Language::Go | Language::Java | Language::Csharp)
}
