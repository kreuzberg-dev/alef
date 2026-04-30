/// Integration tests verifying that `alef generate` runs formatters by default
/// and skips them when `--no-format` is passed.
///
/// These tests exercise only the CLI flag plumbing: they confirm the binary
/// accepts `--no-format`, rejects the old `--format`, and that the help text
/// reflects the new default-on behaviour.  Full formatting behaviour is
/// covered by e2e tests that run against a real alef project.
use std::process::Command;

fn alef_binary() -> std::path::PathBuf {
    // `cargo test` sets CARGO_BIN_EXE_alef when the binary is declared in Cargo.toml.
    // Fall back to finding it in the target directory for environments that don't set
    // the env var (e.g. when tests are run from a parent workspace).
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_alef") {
        return std::path::PathBuf::from(path);
    }
    // Traverse up to locate the workspace target directory.
    let mut dir = std::env::current_exe()
        .expect("current_exe")
        .parent()
        .expect("parent")
        .to_path_buf();
    // The test binary lives in target/<profile>/deps/; the CLI binary is in target/<profile>/.
    if dir.ends_with("deps") {
        dir = dir.parent().expect("parent of deps").to_path_buf();
    }
    dir.join("alef")
}

/// Running `alef generate --help` must mention `--no-format` and must NOT
/// mention `--format` as a standalone flag (it was removed as a breaking change).
#[test]
fn generate_help_shows_no_format_flag() {
    let output = Command::new(alef_binary())
        .args(["generate", "--help"])
        .output()
        .expect("failed to run alef generate --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("--no-format"),
        "`alef generate --help` must list --no-format flag; got:\n{combined}"
    );
    // The old --format flag must not appear (it was the opt-in flag before inversion).
    // We check for the standalone word "  --format" (with leading spaces that clap uses
    // for option lines) so we don't accidentally match "--no-format".
    assert!(
        !combined.contains("  --format"),
        "`alef generate --help` must not list the old --format flag; got:\n{combined}"
    );
}

/// Running `alef all --help` must expose `--no-format` and not the old `--format`.
#[test]
fn all_help_shows_no_format_flag() {
    let output = Command::new(alef_binary())
        .args(["all", "--help"])
        .output()
        .expect("failed to run alef all --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(
        combined.contains("--no-format"),
        "`alef all --help` must list --no-format flag; got:\n{combined}"
    );
    assert!(
        !combined.contains("  --format"),
        "`alef all --help` must not list the old --format flag; got:\n{combined}"
    );
}

/// Passing the old `--format` flag to `alef generate` must be rejected by clap
/// (unknown argument error, non-zero exit).
#[test]
fn generate_rejects_old_format_flag() {
    // We need a config to get past the config-load check, but clap parses flags
    // before we hit any logic — an unknown flag exits immediately.  We can point
    // at a non-existent config; clap will reject `--format` first.
    let output = Command::new(alef_binary())
        .args(["generate", "--format"])
        .output()
        .expect("failed to spawn alef");

    assert!(
        !output.status.success(),
        "alef generate --format (old flag) must exit non-zero; it was accepted unexpectedly"
    );
}

/// Passing the old `--format` flag to `alef all` must also be rejected.
#[test]
fn all_rejects_old_format_flag() {
    let output = Command::new(alef_binary())
        .args(["all", "--format"])
        .output()
        .expect("failed to spawn alef");

    assert!(
        !output.status.success(),
        "alef all --format (old flag) must exit non-zero; it was accepted unexpectedly"
    );
}
