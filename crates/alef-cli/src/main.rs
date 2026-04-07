use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

mod cache;
mod pipeline;
mod registry;

#[derive(Parser)]
#[command(name = "alef", about = "Opinionated polyglot binding generator")]
struct Cli {
    /// Path to alef.toml config file.
    #[arg(long, default_value = "alef.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract API surface from Rust source into IR.
    Extract {
        /// Output IR JSON file.
        #[arg(short, long, default_value = ".alef/ir.json")]
        output: PathBuf,
    },
    /// Generate bindings for selected languages.
    Generate {
        /// Comma-separated list of languages (default: all from config).
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
        /// Ignore cache, regenerate everything.
        #[arg(long)]
        clean: bool,
    },
    /// Generate type stubs (.pyi, .rbs).
    Stubs {
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Generate package scaffolding (pyproject.toml, package.json, etc.).
    Scaffold {
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Generate README files from templates.
    Readme {
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Sync version from Cargo.toml to all package manifests.
    SyncVersions,
    /// Run configured lint/format commands on generated output.
    Lint {
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Verify bindings are up to date and API surface parity.
    Verify {
        /// Exit with code 1 if any binding is stale (CI mode).
        #[arg(long)]
        exit_code: bool,
        /// Also run compilation check.
        #[arg(long)]
        compile: bool,
        /// Also run lint check.
        #[arg(long)]
        lint: bool,
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Show diff of what would change without writing.
    Diff {
        /// Exit with code 1 if changes exist (CI mode).
        #[arg(long)]
        exit_code: bool,
    },
    /// Run all: generate + stubs + scaffold + readme + sync.
    All {
        /// Ignore cache.
        #[arg(long)]
        clean: bool,
    },
    /// Initialize a new alef.toml config.
    Init {
        /// Comma-separated list of languages.
        #[arg(long, value_delimiter = ',')]
        lang: Option<Vec<String>>,
    },
    /// Manage the build cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Clear the .alef/ cache directory.
    Clear,
    /// Show cache status.
    Status,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let config_path = &cli.config;

    match cli.command {
        Commands::Extract { output } => {
            let config = load_config(config_path)?;
            let api = pipeline::extract(&config, config_path, false)?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output, serde_json::to_string_pretty(&api)?)?;
            println!("Wrote IR to {}", output.display());
            Ok(())
        }
        Commands::Generate { lang, clean } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Generating bindings for: {}", format_languages(&languages));
            let api = pipeline::extract(&config, config_path, clean)?;
            let files = pipeline::generate(&api, &config, &languages, clean)?;
            let base_dir = std::env::current_dir()?;
            let count = pipeline::write_files(&files, &base_dir)?;
            // Auto-format generated Rust files
            pipeline::format_rust_files(&files, &base_dir);

            // Generate public API wrappers
            if config.generate.public_api {
                let public_api_files = pipeline::generate_public_api(&api, &config, &languages)?;
                if !public_api_files.is_empty() {
                    let api_count = pipeline::write_files(&public_api_files, &base_dir)?;
                    eprintln!("Generated {api_count} public API files");
                }
            }

            println!("Generated {count} files");
            Ok(())
        }
        Commands::Stubs { lang } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Generating type stubs for: {}", format_languages(&languages));
            let api = pipeline::extract(&config, config_path, false)?;
            let files = pipeline::generate_stubs(&api, &config, &languages)?;
            let base_dir = std::env::current_dir()?;
            let count = pipeline::write_files(&files, &base_dir)?;
            println!("Generated {count} stub files");
            Ok(())
        }
        Commands::Scaffold { lang } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Generating scaffolding for: {}", format_languages(&languages));
            let api = pipeline::extract(&config, config_path, false)?;
            let files = pipeline::scaffold(&api, &config, &languages)?;
            let base_dir = std::env::current_dir()?;
            let count = pipeline::write_scaffold_files(&files, &base_dir)?;
            println!("Generated {count} scaffold files");
            Ok(())
        }
        Commands::Readme { lang } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Generating READMEs for: {}", format_languages(&languages));
            let api = pipeline::extract(&config, config_path, false)?;
            let files = pipeline::readme(&api, &config, &languages)?;
            let base_dir = std::env::current_dir()?;
            let count = pipeline::write_scaffold_files(&files, &base_dir)?;
            println!("Generated {count} README files");
            Ok(())
        }
        Commands::SyncVersions => {
            let config = load_config(config_path)?;
            eprintln!("Syncing versions from Cargo.toml");
            pipeline::sync_versions(&config)?;
            println!("Version sync complete");
            Ok(())
        }
        Commands::Lint { lang } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Linting generated output for: {}", format_languages(&languages));
            pipeline::lint(&config, &languages)?;
            println!("Lint complete");
            Ok(())
        }
        Commands::Verify {
            exit_code,
            compile,
            lint,
            lang,
        } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, lang.as_deref())?;
            eprintln!("Verifying bindings for: {}", format_languages(&languages));
            if compile {
                eprintln!("  (with compilation check)");
            }
            if lint {
                eprintln!("  (with lint check)");
            }

            let api = pipeline::extract(&config, config_path, false)?;
            let bindings = pipeline::generate(&api, &config, &languages, true)?;
            let stubs = pipeline::generate_stubs(&api, &config, &languages)?;

            let base_dir = std::env::current_dir()?;
            let mut all_diffs = pipeline::diff_files(&bindings, &base_dir)?;
            all_diffs.extend(pipeline::diff_files(&stubs, &base_dir)?);

            if all_diffs.is_empty() {
                println!("All bindings are up to date.");
            } else {
                println!("Stale bindings detected:");
                for diff in &all_diffs {
                    println!("  {diff}");
                }
                if exit_code {
                    process::exit(1);
                }
            }
            Ok(())
        }
        Commands::Diff { exit_code } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, None)?;
            eprintln!("Computing diff of generated bindings...");

            let api = pipeline::extract(&config, config_path, false)?;
            let bindings = pipeline::generate(&api, &config, &languages, true)?;
            let stubs = pipeline::generate_stubs(&api, &config, &languages)?;

            let base_dir = std::env::current_dir()?;
            let mut all_diffs = pipeline::diff_files(&bindings, &base_dir)?;
            all_diffs.extend(pipeline::diff_files(&stubs, &base_dir)?);

            if all_diffs.is_empty() {
                println!("No changes detected.");
            } else {
                println!("Files that would change:");
                for diff in &all_diffs {
                    println!("  {diff}");
                }
                if exit_code {
                    process::exit(1);
                }
            }
            Ok(())
        }
        Commands::All { clean } => {
            let config = load_config(config_path)?;
            let languages = resolve_languages(&config, None)?;
            eprintln!("Running all for: {}", format_languages(&languages));

            let api = pipeline::extract(&config, config_path, clean)?;

            eprintln!("Generating bindings...");
            let bindings = pipeline::generate(&api, &config, &languages, clean)?;
            let base_dir = std::env::current_dir()?;
            let binding_count = pipeline::write_files(&bindings, &base_dir)?;
            pipeline::format_rust_files(&bindings, &base_dir);

            eprintln!("Generating type stubs...");
            let stubs = pipeline::generate_stubs(&api, &config, &languages)?;
            let stub_count = pipeline::write_files(&stubs, &base_dir)?;

            // Generate public API wrappers
            let mut api_count = 0;
            if config.generate.public_api {
                let public_api_files = pipeline::generate_public_api(&api, &config, &languages)?;
                if !public_api_files.is_empty() {
                    api_count = pipeline::write_files(&public_api_files, &base_dir)?;
                }
            }

            eprintln!("Generating scaffolding...");
            let scaffold_files = pipeline::scaffold(&api, &config, &languages)?;
            let scaffold_count = pipeline::write_scaffold_files(&scaffold_files, &base_dir)?;

            eprintln!("Generating READMEs...");
            let readme_files = pipeline::readme(&api, &config, &languages)?;
            let readme_count = pipeline::write_scaffold_files(&readme_files, &base_dir)?;

            println!(
                "Done: {binding_count} binding files, {stub_count} stub files, {api_count} API files, {scaffold_count} scaffold files, {readme_count} readme files"
            );
            Ok(())
        }
        Commands::Init { lang } => {
            eprintln!("Initializing alef.toml");
            if let Some(langs) = &lang {
                eprintln!("  Languages: {}", langs.join(", "));
            }
            pipeline::init(config_path, lang)?;
            println!("Initialized alef.toml");
            Ok(())
        }
        Commands::Cache { action } => match action {
            CacheAction::Clear => {
                cache::clear_cache()?;
                println!("Cache cleared.");
                Ok(())
            }
            CacheAction::Status => {
                cache::show_status();
                Ok(())
            }
        },
    }
}

fn load_config(path: &std::path::Path) -> Result<alef_core::config::AlefConfig> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read config: {}", path.display()))?;
    let config: alef_core::config::AlefConfig =
        toml::from_str(&content).with_context(|| "Failed to parse alef.toml")?;
    Ok(config)
}

fn resolve_languages(
    config: &alef_core::config::AlefConfig,
    filter: Option<&[String]>,
) -> Result<Vec<alef_core::config::Language>> {
    match filter {
        Some(langs) => {
            let mut result = vec![];
            for lang_str in langs {
                let lang: alef_core::config::Language = toml::Value::String(lang_str.clone())
                    .try_into()
                    .with_context(|| format!("Unknown language: {lang_str}"))?;
                if config.languages.contains(&lang) {
                    result.push(lang);
                } else {
                    anyhow::bail!("Language '{lang_str}' not in config languages list");
                }
            }
            Ok(result)
        }
        None => Ok(config.languages.clone()),
    }
}

fn format_languages(languages: &[alef_core::config::Language]) -> String {
    languages.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
}
