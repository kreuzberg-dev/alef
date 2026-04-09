use alef_core::config::{AlefConfig, Language};
use anyhow::Context as _;
use tracing::{debug, info};

use super::helpers::run_command;

/// Read the version from a Cargo.toml file (workspace or regular package).
pub(crate) fn read_version(version_from: &str) -> anyhow::Result<String> {
    let content =
        std::fs::read_to_string(version_from).with_context(|| format!("failed to read version file {version_from}"))?;
    let value: toml::Value =
        toml::from_str(&content).with_context(|| format!("failed to parse TOML in {version_from}"))?;
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

/// Bump a semver version string by the given component (major, minor, patch).
fn bump_version(version: &str, component: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid semver version: {version}");
    }
    let mut major: u64 = parts[0]
        .parse()
        .with_context(|| format!("Invalid major version component: {}", parts[0]))?;
    let mut minor: u64 = parts[1]
        .parse()
        .with_context(|| format!("Invalid minor version component: {}", parts[1]))?;
    let mut patch: u64 = parts[2]
        .parse()
        .with_context(|| format!("Invalid patch version component: {}", parts[2]))?;

    match component {
        "major" => {
            major += 1;
            minor = 0;
            patch = 0;
        }
        "minor" => {
            minor += 1;
            patch = 0;
        }
        "patch" => {
            patch += 1;
        }
        other => anyhow::bail!("Unknown bump component '{other}': expected major, minor, or patch"),
    }

    Ok(format!("{major}.{minor}.{patch}"))
}

/// Write a bumped version back into a Cargo.toml (workspace or regular package).
fn write_version_to_cargo_toml(cargo_toml_path: &str, new_version: &str) -> anyhow::Result<()> {
    let content =
        std::fs::read_to_string(cargo_toml_path).with_context(|| format!("Failed to read {cargo_toml_path}"))?;

    // Match `version = "..."` as a standalone line (covers both [package] and [workspace.package])
    let re =
        regex::Regex::new(r#"(?m)^(version\s*=\s*)"[^"]*""#).context("failed to compile version replacement regex")?;
    let new_content = re
        .replace(&content, format!(r#"version = "{new_version}""#).as_str())
        .to_string();

    if new_content == content {
        anyhow::bail!("Could not find a `version = \"...\"` field to update in {cargo_toml_path}");
    }

    std::fs::write(cargo_toml_path, new_content)
        .with_context(|| format!("Failed to write updated version to {cargo_toml_path}"))?;

    Ok(())
}

/// Sync version from Cargo.toml to all package manifest files.
pub fn sync_versions(config: &AlefConfig, bump: Option<&str>) -> anyhow::Result<()> {
    // If bump is requested, read current version, bump it, and write it back to Cargo.toml.
    if let Some(component) = bump {
        let current = read_version(&config.crate_config.version_from)?;
        let bumped = bump_version(&current, component)?;
        info!("Bumping version {current} -> {bumped} ({component})");
        write_version_to_cargo_toml(&config.crate_config.version_from, &bumped).context("failed to sync versions")?;
        info!(
            "Updated {} with bumped version {bumped}",
            config.crate_config.version_from
        );
    }

    let version = read_version(&config.crate_config.version_from)?;
    info!("Syncing version {version}");

    let mut updated = vec![];

    // Python: pyproject.toml
    if let Ok(content) = std::fs::read_to_string("packages/python/pyproject.toml") {
        if let Some(new_content) = replace_version_pattern(&content, r#"version = "[^"]*""#, &version) {
            std::fs::write("packages/python/pyproject.toml", &new_content)
                .context("failed to write packages/python/pyproject.toml")?;
            updated.push("packages/python/pyproject.toml".to_string());
        }
    }

    // Node: package.json
    if let Ok(content) = std::fs::read_to_string("packages/typescript/package.json") {
        if let Some(new_content) = replace_version_pattern(&content, r#""version": "[^"]*""#, &version) {
            std::fs::write("packages/typescript/package.json", &new_content)
                .context("failed to write packages/typescript/package.json")?;
            updated.push("packages/typescript/package.json".to_string());
        }
    }

    // Ruby: *.gemspec
    if let Ok(entries) = std::fs::read_dir("packages/ruby") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "gemspec") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(new_content) =
                        replace_version_pattern(&content, r#"spec\.version\s*=\s*"[^"]*""#, &version)
                    {
                        std::fs::write(&path, &new_content)?;
                        updated.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // PHP: composer.json
    if let Ok(content) = std::fs::read_to_string("packages/php/composer.json") {
        if let Some(new_content) = replace_version_pattern(&content, r#""version": "[^"]*""#, &version) {
            std::fs::write("packages/php/composer.json", &new_content)?;
            updated.push("packages/php/composer.json".to_string());
        }
    }

    // Elixir: mix.exs
    if let Ok(content) = std::fs::read_to_string("packages/elixir/mix.exs") {
        if let Some(new_content) = replace_version_pattern(&content, r#"version: "[^"]*""#, &version) {
            std::fs::write("packages/elixir/mix.exs", &new_content)?;
            updated.push("packages/elixir/mix.exs".to_string());
        }
    }

    // Go: go.mod (no version field, skip)

    // Java: pom.xml
    if let Ok(content) = std::fs::read_to_string("packages/java/pom.xml") {
        if let Some(new_content) = replace_version_pattern(&content, r#"<version>[^<]*</version>"#, &version) {
            std::fs::write("packages/java/pom.xml", &new_content)?;
            updated.push("packages/java/pom.xml".to_string());
        }
    }

    // C#: *.csproj
    if let Ok(entries) = std::fs::read_dir("packages/csharp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "csproj") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(new_content) =
                        replace_version_pattern(&content, r#"<Version>[^<]*</Version>"#, &version)
                    {
                        std::fs::write(&path, &new_content)?;
                        updated.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // R: DESCRIPTION file
    if let Ok(content) = std::fs::read_to_string("packages/r/DESCRIPTION") {
        if let Some(new_content) = replace_version_pattern(&content, r"Version:\s*[^\n]*", &version) {
            std::fs::write("packages/r/DESCRIPTION", &new_content)?;
            updated.push("packages/r/DESCRIPTION".to_string());
        }
    }

    // Python: __init__.py
    if let Ok(content) = std::fs::read_to_string("packages/python/__init__.py") {
        if let Some(new_content) = replace_version_pattern(&content, r#"__version__\s*=\s*"[^"]*""#, &version) {
            std::fs::write("packages/python/__init__.py", &new_content)?;
            updated.push("packages/python/__init__.py".to_string());
        }
    }

    // Go: ffi_loader.go
    if let Ok(content) = std::fs::read_to_string("packages/go/ffi_loader.go") {
        if let Some(new_content) = replace_version_pattern(&content, r#"defaultFFIVersion\s*=\s*"[^"]*""#, &version) {
            std::fs::write("packages/go/ffi_loader.go", &new_content)?;
            updated.push("packages/go/ffi_loader.go".to_string());
        }
    }

    // Process extra_paths from config [sync] section (glob patterns)
    if let Some(sync_config) = &config.sync {
        let version_re = regex::Regex::new(r"\d+\.\d+\.\d+").ok();
        for pattern in &sync_config.extra_paths {
            match glob::glob(pattern) {
                Ok(paths) => {
                    for entry in paths {
                        match entry {
                            Ok(path) => {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Some(ref re) = version_re {
                                        let new_content = re.replace_all(&content, version.as_str()).to_string();
                                        if new_content != content {
                                            if let Err(e) = std::fs::write(&path, &new_content) {
                                                debug!("Could not write {}: {e}", path.display());
                                            } else {
                                                updated.push(path.to_string_lossy().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Glob entry error for pattern '{pattern}': {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Invalid glob pattern '{pattern}': {e}");
                }
            }
        }

        // Process text_replacements from config [sync] section
        for replacement in &sync_config.text_replacements {
            match glob::glob(&replacement.path) {
                Ok(paths) => {
                    for entry in paths {
                        match entry {
                            Ok(path) => {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let search = replacement.search.replace("{version}", &version);
                                    let replace = replacement.replace.replace("{version}", &version);
                                    if let Ok(re) = regex::Regex::new(&search) {
                                        let new_content = re.replace_all(&content, replace.as_str()).to_string();
                                        if new_content != content {
                                            if let Err(e) = std::fs::write(&path, &new_content) {
                                                debug!("Could not write {}: {e}", path.display());
                                            } else {
                                                updated.push(path.to_string_lossy().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Glob entry error for pattern '{}': {e}", replacement.path);
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Invalid glob pattern '{}': {e}", replacement.path);
                }
            }
        }
    }

    for file in updated {
        info!("  Updated: {file}");
    }

    // Rebuild FFI to refresh C headers (cbindgen) if FFI language is configured.
    if config.languages.contains(&Language::Ffi) {
        let ffi_crate = config
            .output
            .ffi
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.replace("src", "").trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("{}-ffi", config.crate_config.name));
        info!("Rebuilding FFI ({ffi_crate}) to refresh C headers...");
        let _ = run_command(&format!("cargo build -p {ffi_crate}"));
    }

    Ok(())
}

/// Replace version pattern in content. Returns Some(new_content) if replaced, None if pattern not found.
fn replace_version_pattern(content: &str, pattern: &str, version: &str) -> Option<String> {
    let regex = regex::Regex::new(pattern).ok()?;
    if !regex.is_match(content) {
        return None;
    }

    let replacement = match pattern {
        p if p.contains("version =") && !p.contains("spec") => format!(r#"version = "{version}""#),
        p if p.contains("\"version\"") && p.contains("\"") => format!(r#""version": "{version}""#),
        p if p.contains("spec") => format!(r#"spec.version = "{version}""#),
        p if p.contains("<version>") => format!("<version>{version}</version>"),
        p if p.contains("<Version>") => format!("<Version>{version}</Version>"),
        p if p.contains("version:") && p.contains(":") => format!(r#"version: "{version}""#),
        p if p.contains("__version__") => format!(r#"__version__ = "{version}""#),
        p if p.contains("defaultFFIVersion") => format!(r#"defaultFFIVersion = "{version}""#),
        p if p.contains("Version:") => format!("Version: {version}"),
        _ => return None,
    };

    Some(regex.replace(content, replacement.as_str()).to_string())
}
