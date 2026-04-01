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

/// Generate scaffold files for given languages.
pub fn scaffold(api: &ApiSurface, config: &SkifConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    skif_scaffold::scaffold(api, config, languages)
}

/// Generate README files for given languages.
pub fn readme(api: &ApiSurface, config: &SkifConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    skif_readme::generate_readmes(api, config, languages)
}

/// Write standalone generated files (not grouped by language) to disk.
pub fn write_scaffold_files(files: &[GeneratedFile], base_dir: &Path) -> anyhow::Result<usize> {
    let mut count = 0;
    for file in files {
        let full_path = base_dir.join(&file.path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, &file.content)?;
        count += 1;
        debug!("  wrote: {}", full_path.display());
    }
    Ok(count)
}

/// Sync version from Cargo.toml to all package manifest files.
pub fn sync_versions(config: &SkifConfig) -> anyhow::Result<()> {
    let version = read_version(&config.crate_config.version_from)?;
    info!("Syncing version {version}");

    let mut updated = vec![];

    // Python: pyproject.toml
    if let Ok(content) = std::fs::read_to_string("packages/python/pyproject.toml") {
        if let Some(new_content) = replace_version_pattern(&content, r#"version = "[^"]*""#, &version) {
            std::fs::write("packages/python/pyproject.toml", &new_content)?;
            updated.push("packages/python/pyproject.toml".to_string());
        }
    }

    // Node: package.json
    if let Ok(content) = std::fs::read_to_string("packages/typescript/package.json") {
        if let Some(new_content) = replace_version_pattern(&content, r#""version": "[^"]*""#, &version) {
            std::fs::write("packages/typescript/package.json", &new_content)?;
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

    for file in updated {
        info!("  Updated: {file}");
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
        p if p.contains("version:") => format!(r#"version: "{version}""#),
        _ => return None,
    };

    Some(regex.replace(content, replacement.as_str()).to_string())
}

/// Run configured lint/format commands on generated output.
pub fn lint(config: &SkifConfig, languages: &[Language]) -> anyhow::Result<()> {
    let lint_config = config.lint.as_ref();
    for lang in languages {
        let lang_str = lang.to_string();
        if let Some(lint_map) = lint_config {
            if let Some(lang_lint) = lint_map.get(&lang_str) {
                // Run format command if configured
                if let Some(fmt_cmd) = &lang_lint.format {
                    run_command(fmt_cmd)?;
                }
                // Run check command if configured
                if let Some(check_cmd) = &lang_lint.check {
                    run_command(check_cmd)?;
                }
                // Run typecheck command if configured
                if let Some(typecheck_cmd) = &lang_lint.typecheck {
                    run_command(typecheck_cmd)?;
                }
            }
        }
    }
    Ok(())
}

fn run_command(cmd: &str) -> anyhow::Result<()> {
    info!("Running: {cmd}");
    let status = std::process::Command::new("sh").args(["-c", cmd]).status()?;
    if !status.success() {
        anyhow::bail!("Command failed: {cmd}");
    }
    Ok(())
}

/// Initialize a new skif.toml config file.
pub fn init(config_path: &std::path::Path, languages: Option<Vec<String>>) -> anyhow::Result<()> {
    // Read crate name and version from Cargo.toml
    let (crate_name, crate_version) = read_crate_metadata()?;

    // Use provided languages or default to ["python", "node", "ffi"]
    let langs = languages.unwrap_or_else(|| vec!["python".to_string(), "node".to_string(), "ffi".to_string()]);

    // Generate config content
    let config_content = generate_init_config(&crate_name, &crate_version, &langs);

    // Write to skif.toml
    std::fs::write(config_path, config_content)?;
    info!("Created {}", config_path.display());

    Ok(())
}

fn read_crate_metadata() -> anyhow::Result<(String, String)> {
    let content = std::fs::read_to_string("Cargo.toml")?;
    let value: toml::Value = toml::from_str(&content)?;

    // Try workspace.package first
    if let Some(name) = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
    {
        if let Some(version) = value
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
        {
            return Ok((name.to_string(), version.to_string()));
        }
    }

    // Try package directly
    if let Some(name) = value
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
    {
        if let Some(version) = value
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
        {
            return Ok((name.to_string(), version.to_string()));
        }
    }

    anyhow::bail!("Could not find package name and version in Cargo.toml")
}

fn generate_init_config(crate_name: &str, _crate_version: &str, languages: &[String]) -> String {
    let source_path = format!("crates/{}/src/lib.rs", crate_name);

    let mut config = String::from("languages = [");

    for (i, lang) in languages.iter().enumerate() {
        if i > 0 {
            config.push_str(", ");
        }
        config.push('"');
        config.push_str(lang);
        config.push('"');
    }
    config.push_str("]\n\n");

    config.push_str(&format!(
        "[crate]\nname = \"{}\"\nsources = [\"{}\"]\nversion_from = \"Cargo.toml\"\n",
        crate_name, source_path
    ));

    // Add language-specific configs
    if languages.contains(&"python".to_string()) {
        config.push_str(&format!(
            "\n[python]\nmodule_name = \"_{}\"\n",
            crate_name.replace('-', "_")
        ));
    }

    if languages.contains(&"node".to_string()) {
        config.push_str(&format!("\n[node]\npackage_name = \"{crate_name}\"\n"));
    }

    if languages.contains(&"ffi".to_string()) {
        config.push_str(&format!("\n[ffi]\nprefix = \"{}\"\n", crate_name.replace('-', "_")));
    }

    if languages.contains(&"go".to_string()) {
        config.push_str(&format!(
            "\n[go]\nmodule = \"github.com/kreuzberg-dev/{}\"\n",
            crate_name
        ));
    }

    if languages.contains(&"ruby".to_string()) {
        config.push_str(&format!("\n[ruby]\ngem_name = \"{}\"\n", crate_name.replace('-', "_")));
    }

    if languages.contains(&"java".to_string()) {
        config.push_str("\n[java]\npackage = \"dev.kreuzberg\"\n");
    }

    if languages.contains(&"csharp".to_string()) {
        config.push_str(&format!("\n[csharp]\nnamespace = \"{}\"\n", to_pascal_case(crate_name)));
    }

    config
}

fn to_pascal_case(s: &str) -> String {
    s.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

fn apply_exclusions(mut api: ApiSurface, exclude: &skif_core::config::ExcludeConfig) -> ApiSurface {
    api.types.retain(|t| !exclude.types.contains(&t.name));
    api.functions.retain(|f| !exclude.functions.contains(&f.name));
    api.enums.retain(|e| !exclude.types.contains(&e.name));
    api.errors.retain(|e| !exclude.types.contains(&e.name));
    api
}
