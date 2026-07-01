//! Tests for the repo-root `poly.toml` scaffolding.

use super::*;
use crate::core::config::Language;

/// Locate the generated `poly.toml` in a scaffold result.
fn poly_toml(files: &[GeneratedFile]) -> &GeneratedFile {
    files
        .iter()
        .find(|f| f.path.to_string_lossy() == "poly.toml")
        .expect("scaffold should emit a repo-root poly.toml")
}

#[test]
fn emits_a_generated_poly_toml_replacing_precommit() {
    let config = test_config();
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Python, Language::Node]).unwrap();

    // poly.toml is emitted, alef-managed (hash-tracked, overwritten on regen).
    let poly = poly_toml(&files);
    assert!(poly.generated_header, "poly.toml must be alef-managed (generated_header)");

    // The former per-tool / pre-commit configs are gone.
    let paths: Vec<String> = files.iter().map(|f| f.path.to_string_lossy().into_owned()).collect();
    assert!(
        !paths.iter().any(|p| p.ends_with(".pre-commit-config.yaml")),
        "must not emit .pre-commit-config.yaml; got {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with(".typos.toml")),
        "must not emit .typos.toml; got {paths:?}"
    );
}

#[test]
fn poly_toml_drives_hooks_builtins_and_excludes() {
    let config = test_config();
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Python]).unwrap();
    let c = &poly_toml(&files).content;

    // Single-config hook orchestration: builtins + commit-msg stage.
    assert!(c.contains("[hooks]") && c.contains("stages = [ \"pre-commit\" ]"));
    assert!(c.contains("[hooks.builtin]"));
    assert!(c.contains("cargo = true"), "cargo builtin must be enabled");
    assert!(
        c.contains("commit = { stages = [ \"commit-msg\" ] }"),
        "commit builtin must run on commit-msg"
    );
    // Excludes appear in discovery (direct CLI) and the builtin hook path.
    assert!(c.contains("[discovery]") && c.contains("\"target/**\""));
    assert!(c.contains("polylint = { exclude = ["));
    assert!(c.contains("polyfmt = { exclude = ["));
    assert!(c.contains("file_safety = { exclude = ["));
}

#[test]
fn poly_toml_python_ruff_pyrefly_and_per_file_ignores() {
    let config = test_config();
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Python]).unwrap();
    let c = &poly_toml(&files).content;

    // ruff rule selection (ported from the dropped [tool.ruff]).
    assert!(c.contains("[lint.python.ruff]") && c.contains("select = [ \"ALL\" ]"));
    assert!(c.contains("\"ANN401\","), "ruff ignore list must be ported");
    // Forward-compat ruff params poly will honor once landed.
    assert!(c.contains("pydocstyle_convention = \"google\""));
    assert!(c.contains("pylint_max_args = 10"));
    // Cross-engine per-file ignores for the alef wrappers.
    assert!(c.contains("[per-file-ignores]") && c.contains("\"**/api.py\""));
    // pyrefly type-check hook in project mode (replaces mypy).
    assert!(c.contains("[hooks.pre-commit.commands.pyrefly]") && c.contains("pyrefly check packages/python"));
}

#[test]
fn poly_toml_php_uses_mago_correctness_security() {
    let config = test_config();
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Python, Language::Php]).unwrap();
    let c = &poly_toml(&files).content;

    assert!(
        c.contains("[lint.php.mago]") && c.contains("select = [ \"correctness\", \"security\" ]"),
        "PHP must use mago correctness/security ruleset (replacing phpstan/php-cs-fixer)"
    );
}

#[test]
fn poly_toml_omits_language_tables_when_language_absent() {
    let config = test_config();
    let api = test_api();
    // No Python, no PHP.
    let files = scaffold(&api, &config, &[Language::Node]).unwrap();
    let c = &poly_toml(&files).content;

    assert!(!c.contains("[lint.python.ruff]"), "no python table without python");
    assert!(!c.contains("[lint.php.mago]"), "no php table without php");
    assert!(!c.contains("pyrefly"), "no pyrefly hook without python");
    // per-file-ignores is always emitted (generated test/e2e suites exist in
    // every repo), but the python-wrapper entries must be absent without python.
    assert!(!c.contains("\"**/api.py\""), "no python wrapper per-file-ignores without python");
    assert!(c.contains("\"**/e2e/**\""), "test/e2e per-file-ignores always emitted");
}