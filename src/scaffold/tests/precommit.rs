use super::*;

#[test]
fn test_pre_commit_config_python_node() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Python, Language::Node]);
    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    // Common hooks always present
    assert!(content.contains("cargo-fmt"));
    assert!(content.contains("cargo-clippy"));
    assert!(content.contains("trailing-whitespace"));
    assert!(content.contains("cargo-deny"));
    // Python-specific TOML formatting
    assert!(content.contains("pyproject-fmt"));
    // Alef hooks are opt-in — not present in default config
    assert!(!content.contains("alef-readme"));
    assert!(!content.contains("alef-verify"));
    // No per-language hooks
    assert!(!content.contains("ruff-pre-commit"));
    assert!(!content.contains("oxlint"));
    assert!(!content.contains("php-lint"));
    assert!(!content.contains("golangci-lint"));
    assert!(!content.contains("mix-credo"));
}

#[test]
fn test_pre_commit_config_ffi_only() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Ffi]);
    assert_eq!(files.len(), 1);
    let content = &files[0].content;
    // Common + Rust hooks
    assert!(content.contains("cargo-fmt"));
    assert!(content.contains("cargo-clippy"));
    // Alef hooks are opt-in — not present in default config
    assert!(!content.contains("alef-verify"));
    assert!(!content.contains("alef-readme"));
    // No per-language hooks
    assert!(!content.contains("clang-format"));
    assert!(!content.contains("ruff"));
    assert!(!content.contains(concat!("bio", "me")));
}

#[test]
fn test_pre_commit_config_clippy_excludes() {
    let config = test_config();
    let files = generate_pre_commit_config(
        &config,
        &[Language::Python, Language::Node, Language::Php, Language::Wasm],
    );
    let content = &files[0].content;
    assert!(content.contains("--exclude=my-lib-py"));
    assert!(content.contains("--exclude=my-lib-node"));
    assert!(content.contains("--exclude=my-lib-php"));
    // Wasm is NOT excluded — rust-toolchain.toml provides the target
    assert!(!content.contains("--exclude=my-lib-wasm"));
    // Ruby not in languages, should not be excluded
    assert!(!content.contains("--exclude=my-lib-rb"));
}

#[test]
fn test_pre_commit_config_all_languages() {
    let config = test_config();
    let files = generate_pre_commit_config(
        &config,
        &[
            Language::Python,
            Language::Node,
            Language::Ruby,
            Language::Php,
            Language::Ffi,
            Language::Go,
            Language::Java,
            Language::Csharp,
            Language::Elixir,
            Language::R,
        ],
    );
    let content = &files[0].content;
    // Common hooks always present
    assert!(content.contains("cargo-fmt"));
    assert!(content.contains("cargo-clippy"));
    assert!(content.contains("trailing-whitespace"));
    assert!(content.contains("typos"));
    // Python-specific TOML formatting
    assert!(content.contains("pyproject-fmt"));
    // Alef hooks are opt-in — not present in default config
    assert!(!content.contains("alef-readme"));
    assert!(!content.contains("alef-verify"));
    // Clippy excludes for all binding crates
    assert!(content.contains("--exclude=my-lib-py"));
    assert!(content.contains("--exclude=my-lib-node"));
    assert!(content.contains("--exclude=my-lib-rb"));
    assert!(content.contains("--exclude=my-lib-php"));
    assert!(content.contains("--exclude=my-lib-r"));
    // No per-language hooks
    assert!(!content.contains("ruff"));
    assert!(!content.contains("oxlint"));
    assert!(!content.contains("clang-format"));
    assert!(!content.contains("golangci-lint"));
    assert!(!content.contains("cpd"));
    assert!(!content.contains("dotnet-format"));
    assert!(!content.contains("mix-credo"));
    assert!(!content.contains("rubocop"));
    assert!(!content.contains("php-lint"));
    assert!(!content.contains("r-lintr"));
}

#[test]
fn test_pre_commit_config_uses_default_shared_repo() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Python]);
    let content = &files[0].content;
    // All cargo/rumdl/typos/pyproject-fmt/file-safety hooks live under the
    // single shared repo as of the configured hooks bundle v2.0.0.
    assert!(content.contains("https://github.com/xberg-io/pre-commit-hooks"));
    // The dropped upstream sources must NOT reappear in the scaffold output.
    assert!(!content.contains("https://github.com/pre-commit/pre-commit-hooks"));
    assert!(!content.contains("AndrejOrsula/pre-commit-cargo"));
    assert!(!content.contains("DevinR528/cargo-sort"));
    assert!(!content.contains("bnjbvr/cargo-machete"));
    assert!(!content.contains("EmbarkStudios/cargo-deny"));
    assert!(!content.contains("rvben/rumdl-pre-commit"));
    assert!(!content.contains("tox-dev/pyproject-fmt"));
    assert!(!content.contains("crate-ci/typos"));
}

#[test]
fn test_pre_commit_config_includes_rust_max_lines() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Ffi]);
    let content = &files[0].content;
    // rust-max-lines lives under the shared block; default --max=1000.
    assert!(content.contains("rust-max-lines"));
    assert!(content.contains("--max=1000"));
}

#[test]
fn test_pre_commit_config_includes_new_file_safety_hooks() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Ffi]);
    let content = &files[0].content;
    // Added in the shared hooks bundle v2.0.0.
    assert!(content.contains("check-executables-have-shebangs"));
    assert!(content.contains("check-shebang-scripts-are-executable"));
    assert!(content.contains("mixed-line-ending"));
}

#[test]
fn test_pre_commit_config_drops_cargo_check_hook() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Ffi]);
    let content = &files[0].content;
    // cargo-check was removed in v2.0.0 — cargo-clippy with -D warnings
    // already runs the same compile pipeline plus the clippy lints.
    assert!(!content.contains("id: cargo-check"));
}

// --- Oxc toolchain tests ---

#[test]
fn test_node_scaffold_uses_oxc_tooling() {
    let config = test_config();
    let api = test_api();
    let all_files = scaffold(&api, &config, &[Language::Node]).unwrap();
    let files = language_files(&all_files);
    for f in &files {
        assert!(
            !f.content.contains(concat!("bio", "me")),
            "File {} should not reference the legacy Node formatter: found in content",
            f.path.display()
        );
        assert!(
            !f.path.to_string_lossy().contains(concat!("bio", "me")),
            "File path should not contain the legacy Node formatter: {}",
            f.path.display()
        );
    }
}

// The dead `packages/node/` scaffold previously emitted `.oxfmtrc.json`,
// `.oxlintrc.json`, and a top-level `package.json` with `oxfmt`/`oxlint`
// dev-deps. With that scaffold removed, the only `package.json` we emit is
// the crate-level NAPI-RS manifest at `crates/<crate>-node/`, which doesn't
// run formatting/linting (those are managed at the workspace root). The
// previous tests asserting on those files are intentionally removed.

#[test]
fn test_precommit_uses_unified_hooks_with_node() {
    let config = test_config();
    let files = generate_pre_commit_config(&config, &[Language::Node]);
    let content = &files[0].content;
    assert!(!content.contains(concat!("bio", "me", "-format")));
    assert!(!content.contains(concat!("bio", "me", "-lint")));
    assert!(!content.contains(concat!("bio", "me", "js")));
    assert!(!content.contains("alef-readme"));
    assert!(!content.contains("alef-verify"));
    assert!(!content.contains("oxlint"));
}

#[test]
fn test_precommit_includes_alef_hooks_when_explicitly_enabled() {
    let mut config = test_config();
    config.scaffold.as_mut().unwrap().precommit = Some(PrecommitConfig {
        include_shared_hooks: None,
        shared_hooks_repo: None,
        shared_hooks_rev: None,
        include_alef_hooks: Some(true),
        alef_hooks_repo: None,
        alef_hooks_rev: None,
    });

    let files = generate_pre_commit_config(&config, &[Language::Node]);
    let content = &files[0].content;

    assert!(content.contains("- repo: local"));
    assert!(content.contains("alef-readme"));
    assert!(content.contains("alef-verify"));
    assert!(content.contains("alef-sync-versions"));
}

#[test]
fn test_precommit_uses_configured_hook_repositories() {
    let mut config = test_config();
    config.scaffold.as_mut().unwrap().precommit = Some(PrecommitConfig {
        include_shared_hooks: Some(true),
        shared_hooks_repo: Some("https://github.com/acme/hooks".to_string()),
        shared_hooks_rev: Some("v9.8.7".to_string()),
        include_alef_hooks: Some(false),
        alef_hooks_repo: None,
        alef_hooks_rev: None,
    });

    let files = generate_pre_commit_config(&config, &[Language::Node]);
    let content = &files[0].content;

    assert!(content.contains("https://github.com/acme/hooks"));
    assert!(content.contains("rev: v9.8.7"));
    assert!(!content.contains("https://github.com/sample_crate-dev/alef"));
    assert!(!content.contains("alef-readme"));
}

#[test]
fn test_precommit_defaults_do_not_invent_alef_remote_or_bot_identity() {
    let config = minimal_config_from_toml("");
    let files = generate_pre_commit_config(&config, &[Language::Node]);
    let content = &files[0].content;
    let project_org = format!("{}-{}", "project", "dev");

    assert!(
        !content.contains(&project_org) && !content.contains("project-bot") && !content.contains("bot@"),
        "unconfigured project precommit scaffold must not copy Alef organization, repo, or bot metadata:\n{content}"
    );
    assert!(
        !content.contains("alef-readme") && !content.contains("alef-verify"),
        "unconfigured project precommit scaffold must leave Alef hooks opt-in:\n{content}"
    );
}

// --- Java checkstyle tests ---
