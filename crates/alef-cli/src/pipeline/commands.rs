use alef_core::config::{AlefConfig, Language};
use anyhow::Context as _;
use std::path::Path;
use tracing::{debug, info};

use crate::registry;

use super::helpers::run_command;

/// Run configured lint/format commands on generated output.
pub fn lint(config: &AlefConfig, languages: &[Language]) -> anyhow::Result<()> {
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

/// Run configured test commands for each language.
pub fn test(config: &AlefConfig, languages: &[Language], e2e: bool) -> anyhow::Result<()> {
    let test_config = config.test.as_ref();
    for lang in languages {
        let lang_str = lang.to_string();
        if let Some(test_map) = test_config {
            if let Some(lang_test) = test_map.get(&lang_str) {
                // Run unit/integration test command if configured
                if let Some(cmd) = &lang_test.command {
                    run_command(cmd)?;
                }
                // Run e2e test command if --e2e flag is set and command is configured
                if e2e {
                    if let Some(e2e_cmd) = &lang_test.e2e {
                        run_command(e2e_cmd)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Build language bindings using native build tools.
///
/// Resolves build order (FFI-dependent languages build after FFI), then invokes
/// each language's build tool with the appropriate flags.
pub fn build(config: &AlefConfig, languages: &[Language], release: bool) -> anyhow::Result<()> {
    let crate_name = &config.crate_config.name;
    let base_dir = std::env::current_dir()?;

    // Split into FFI-independent and FFI-dependent languages
    let mut independent = Vec::new();
    let mut ffi_dependent = Vec::new();
    let mut need_ffi = false;

    for &lang in languages {
        let backend = registry::get_backend(lang);
        if let Some(bc) = backend.build_config() {
            if bc.depends_on_ffi {
                ffi_dependent.push((lang, bc));
                need_ffi = true;
            } else {
                independent.push((lang, bc));
            }
        } else {
            info!("No build config for {lang}, skipping");
        }
    }

    // Build FFI first if needed by dependent languages
    if need_ffi
        && !independent
            .iter()
            .any(|(_, bc)| bc.tool == "cargo" && bc.crate_suffix == "-ffi")
    {
        // Resolve FFI crate name from output path
        let ffi_crate = output_path_for(Language::Ffi, config)
            .map(resolve_crate_dir)
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| {
                // Fallback: construct from crate name
                Box::leak(format!("{crate_name}-ffi").into_boxed_str())
            });
        info!("Building FFI crate: {ffi_crate}");
        let mut cmd = format!("cargo build -p {ffi_crate}");
        if release {
            cmd.push_str(" --release");
        }
        run_command(&cmd).context("failed to build FFI crate")?;
    }

    // Build independent languages
    for (lang, bc) in &independent {
        info!("Building {lang} ({})...", bc.tool);
        let build_cmd = build_command_for(*lang, bc, config, release);
        run_command(&build_cmd).with_context(|| format!("failed to build language bindings for {lang}"))?;

        // Run post-build steps
        run_post_build(*lang, bc, config, &base_dir)
            .with_context(|| format!("failed to run post-build steps for {lang}"))?;
    }

    // Build FFI-dependent languages
    for (lang, bc) in &ffi_dependent {
        info!("Building {lang} ({})...", bc.tool);
        let build_cmd = build_command_for(*lang, bc, config, release);
        run_command(&build_cmd).with_context(|| format!("failed to build language bindings for {lang}"))?;

        run_post_build(*lang, bc, config, &base_dir)
            .with_context(|| format!("failed to run post-build steps for {lang}"))?;
    }

    Ok(())
}

/// Resolve the crate directory from the output config path.
/// Output paths like `crates/html-to-markdown-node/src/` → `crates/html-to-markdown-node`.
fn resolve_crate_dir(output_path: &Path) -> &Path {
    // If path ends in src/ or src, go up one level
    if output_path.file_name().is_some_and(|n| n == "src") {
        output_path.parent().unwrap_or(output_path)
    } else {
        output_path
    }
}

/// Get the output path for a language from config.
fn output_path_for(lang: Language, config: &AlefConfig) -> Option<&Path> {
    match lang {
        Language::Python => config.output.python.as_deref(),
        Language::Node => config.output.node.as_deref(),
        Language::Ruby => config.output.ruby.as_deref(),
        Language::Php => config.output.php.as_deref(),
        Language::Ffi => config.output.ffi.as_deref(),
        Language::Go => config.output.go.as_deref(),
        Language::Java => config.output.java.as_deref(),
        Language::Csharp => config.output.csharp.as_deref(),
        Language::Wasm => config.output.wasm.as_deref(),
        Language::Elixir => config.output.elixir.as_deref(),
        Language::R => config.output.r.as_deref(),
    }
}

/// Generate the shell command to build a specific language.
fn build_command_for(
    lang: Language,
    bc: &alef_core::backend::BuildConfig,
    config: &AlefConfig,
    release: bool,
) -> String {
    let release_flag = if release { " --release" } else { "" };

    // Resolve the crate directory from the output path
    let crate_dir = output_path_for(lang, config)
        .map(resolve_crate_dir)
        .and_then(|p| p.to_str())
        .unwrap_or("");

    match bc.tool {
        "maturin" => {
            format!("maturin develop --manifest-path {crate_dir}/Cargo.toml{release_flag}")
        }
        "napi" => {
            // NAPI outputs .node + .d.ts to the crate directory
            format!("napi build --platform --manifest-path {crate_dir}/Cargo.toml -o {crate_dir}{release_flag}")
        }
        "wasm-pack" => {
            let profile = if release { "--release" } else { "--dev" };
            format!("wasm-pack build {crate_dir} {profile} --target bundler")
        }
        "cargo" => {
            // Extract crate name from directory name for -p flag
            let crate_name = Path::new(crate_dir)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(crate_dir);
            format!("cargo build -p {crate_name}{release_flag}")
        }
        "mix" => "mix compile".to_string(),
        "mvn" => {
            let dir = config
                .output
                .java
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("packages/java");
            format!("cd {dir} && mvn package -DskipTests -q")
        }
        "dotnet" => {
            let dir = config
                .output
                .csharp
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("packages/csharp");
            let dotnet_config = if release { "Release" } else { "Debug" };
            format!("cd {dir} && dotnet build --configuration {dotnet_config} -q")
        }
        "go" => {
            let dir = config
                .output
                .go
                .as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("packages/go");
            format!("cd {dir} && go build ./...")
        }
        other => format!("echo 'Unknown build tool: {other}'"),
    }
}

/// Run post-build processing steps (e.g., patching .d.ts files).
fn run_post_build(
    lang: Language,
    bc: &alef_core::backend::BuildConfig,
    config: &AlefConfig,
    base_dir: &Path,
) -> anyhow::Result<()> {
    use alef_core::backend::PostBuildStep;

    // Resolve the crate directory from the output path
    let crate_dir = output_path_for(lang, config)
        .map(resolve_crate_dir)
        .unwrap_or(Path::new(""));

    for step in &bc.post_build {
        match step {
            PostBuildStep::PatchFile { path, find, replace } => {
                let file_path = base_dir.join(crate_dir).join(path);
                if file_path.exists() {
                    let content = std::fs::read_to_string(&file_path)
                        .with_context(|| format!("failed to read post-build patch target {}", file_path.display()))?;
                    let patched = content.replace(find, replace);
                    if patched != content {
                        std::fs::write(&file_path, &patched)
                            .with_context(|| format!("failed to write patched file {}", file_path.display()))?;
                        info!("Patched {}: replaced '{}' → '{}'", file_path.display(), find, replace);
                    }
                } else {
                    debug!("Post-build patch target not found: {}", file_path.display());
                }
            }
        }
    }

    Ok(())
}
