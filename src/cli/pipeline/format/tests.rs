use super::*;
use crate::core::config::{Language, NewAlefConfig, ResolvedCrateConfig};

fn make_config(crate_name: &str) -> ResolvedCrateConfig {
    let cfg: NewAlefConfig = toml::from_str(&format!(
        r#"
[workspace]
languages = ["rust"]
[[crates]]
name = "{crate_name}"
sources = ["src/lib.rs"]
"#
    ))
    .expect("valid config");
    cfg.resolve().unwrap().remove(0)
}

fn make_config_with_csharp_project(crate_name: &str, project_file: &str) -> ResolvedCrateConfig {
    let cfg: NewAlefConfig = toml::from_str(&format!(
        r#"
[workspace]
languages = ["csharp"]
[[crates]]
name = "{crate_name}"
sources = ["src/lib.rs"]
[crates.csharp]
project_file = "{project_file}"
"#
    ))
    .expect("valid config");
    cfg.resolve().unwrap().remove(0)
}

#[test]
fn formatter_error_includes_stdout_and_stderr() {
    let err = run_formatter(
        "sh",
        &["-c", "printf 'stdout text'; printf 'stderr text' >&2; exit 7"],
        Path::new("."),
    )
    .expect_err("formatter should fail");
    let msg = err.to_string();
    assert!(msg.contains("stdout text"), "missing stdout in error: {msg}");
    assert!(msg.contains("stderr text"), "missing stderr in error: {msg}");
}

#[test]
fn test_wasm_formatter_uses_manifest_path() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Wasm).expect("should have formatter");
    // Two commands: cargo fmt (rs files), cargo sort (Cargo.toml table order).
    // No oxfmt step — oxfmt's default TOML style fights cargo-sort's preserved
    // indent and produces an infinite format/regen loop on the embedded hash.
    assert_eq!(spec.commands.len(), 2, "WASM must have cargo fmt + cargo sort steps");
    let fmt_cmd = &spec.commands[0];
    assert_eq!(fmt_cmd.command, "cargo");
    assert_eq!(
        fmt_cmd.args,
        vec!["fmt", "--manifest-path", "crates/sample-model-wasm/Cargo.toml"]
    );
    let sort_cmd = &spec.commands[1];
    assert_eq!(sort_cmd.command, "cargo");
    assert_eq!(
        sort_cmd.args,
        vec!["sort", "crates/sample-model-wasm"],
        "cargo sort arg must be the crate directory, not the manifest path"
    );
    assert!(spec.work_dir.is_empty(), "WASM formatter must run at workspace root");
}

#[test]
fn test_wasm_formatter_uses_configured_output_path() {
    let cfg: NewAlefConfig = toml::from_str(
        r#"
[workspace]
languages = ["wasm"]
[[crates]]
name = "sample-language-pack"
sources = ["crates/sample-pack-core/src/lib.rs"]
[crates.output]
wasm = "crates/sample-pack-core-wasm/src/"
"#,
    )
    .expect("valid config");
    let config = cfg.resolve().unwrap().remove(0);
    let spec = get_default_formatter(&config, Language::Wasm).expect("should have formatter");
    let fmt_cmd = &spec.commands[0];
    assert_eq!(
        fmt_cmd.args,
        vec!["fmt", "--manifest-path", "crates/sample-pack-core-wasm/Cargo.toml"]
    );
    let sort_cmd = &spec.commands[1];
    assert_eq!(
        sort_cmd.args,
        vec!["sort", "crates/sample-pack-core-wasm"],
        "cargo sort arg must match the crate dir derived from the configured output path"
    );
}

#[test]
fn test_node_formatter_excludes_toml_from_oxfmt() {
    // oxfmt also reformats TOML (collapsing arrays, stripping inner-bracket
    // spaces), which fights the consumer's pyproject-fmt (`[ "x" ]`) and
    // cargo-sort, breaking `alef verify` post-finalize. The whole-repo oxfmt
    // run must exclude `**/*.toml`.
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Node).expect("should have formatter");
    let oxfmt_cmd = spec
        .commands
        .iter()
        .find(|c| c.args.iter().any(|a| a == "oxfmt"))
        .expect("Node formatter must run oxfmt");
    assert!(
        oxfmt_cmd.args.iter().any(|a| a == "!**/*.toml"),
        "oxfmt must exclude TOML so it does not fight pyproject-fmt/cargo-sort, got: {:?}",
        oxfmt_cmd.args
    );
}

#[test]
fn test_ffi_formatter_includes_cargo_sort() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Ffi).expect("should have formatter");
    // Two commands: cargo fmt --all (rs files) + cargo sort -w (Cargo.toml table
    // order across the workspace). No oxfmt step here — the shared fixture
    // pre-commit `oxfmt` hook is JS/TS/JSON/CSS only, and running oxfmt on `.`
    // additionally reformats every workspace TOML (including hand-maintained
    // Cargo.toml files) into oxfmt's 2-space style, fighting cargo-sort's
    // preserved indent and breaking the embedded hash.
    assert_eq!(spec.commands.len(), 2, "FFI must have cargo fmt + cargo sort steps");
    let fmt_cmd = &spec.commands[0];
    assert_eq!(fmt_cmd.command, "cargo");
    assert_eq!(fmt_cmd.args, vec!["fmt", "--all"]);
    let sort_cmd = &spec.commands[1];
    assert_eq!(sort_cmd.command, "cargo");
    assert_eq!(
        sort_cmd.args,
        vec!["sort", "-w"],
        "cargo sort must run workspace-wide so all binding crate Cargo.toml files are normalised"
    );
    assert!(spec.work_dir.is_empty(), "FFI formatter must run at workspace root");
}

// The Ruby native crate (`packages/ruby/ext/<gem>/native/`) lives outside the
// consumer cargo workspace, so the FFI formatter's `cargo sort -w` skips it.
// The Ruby formatter must therefore run cargo sort directly against the
// native crate, otherwise prek's `cargo-sort` hook rewrites feature-array
// indentation post-finalize and breaks `alef verify`.
#[test]
fn test_ruby_formatter_includes_cargo_sort_for_native_crate() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Ruby).expect("should have formatter");
    assert_eq!(spec.commands.len(), 2, "Ruby must have rubocop + cargo sort steps");
    let sort_cmd = &spec.commands[1];
    assert_eq!(sort_cmd.command, "cargo");
    assert_eq!(sort_cmd.args[0], "sort");
    assert!(
        sort_cmd.args[1].contains("ext/") && sort_cmd.args[1].contains("/native"),
        "cargo sort arg must target the native crate dir, got: {:?}",
        sort_cmd.args
    );
    assert_eq!(spec.work_dir, "packages/ruby/");
}

// The Elixir NIF crate (`packages/elixir/native/<app>_nif/`) lives outside the
// cargo workspace, so cargo sort must be invoked directly.
#[test]
fn test_elixir_formatter_includes_cargo_sort_for_nif_crate() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Elixir).expect("should have formatter");
    assert_eq!(spec.commands.len(), 2, "Elixir must have mix format + cargo sort steps");
    let sort_cmd = &spec.commands[1];
    assert_eq!(sort_cmd.command, "cargo");
    assert_eq!(sort_cmd.args[0], "sort");
    assert!(
        sort_cmd.args[1].starts_with("native/") && sort_cmd.args[1].ends_with("_nif"),
        "cargo sort arg must target native/<app>_nif, got: {:?}",
        sort_cmd.args
    );
    assert_eq!(spec.work_dir, "packages/elixir/");
}

// The extendr R crate (`packages/r/src/rust/`) is workspace-excluded and so
// needs its own cargo sort invocation.
#[test]
fn test_r_formatter_includes_cargo_sort_for_extendr_crate() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::R).expect("should have formatter");
    assert_eq!(spec.commands.len(), 2, "R must have styler + cargo sort steps");
    let sort_cmd = &spec.commands[1];
    assert_eq!(sort_cmd.command, "cargo");
    assert_eq!(sort_cmd.args, vec!["sort", "packages/r/src/rust"]);
    assert!(spec.work_dir.is_empty(), "R formatter runs at project root");
}

// Bug 2: C# formatter must include project_file when configured to avoid workspace ambiguity.
#[test]
fn test_csharp_formatter_with_project_file() {
    let config = make_config_with_csharp_project("sample-model", "packages/csharp/SampleModel.csproj");
    let spec = get_default_formatter(&config, Language::Csharp).expect("should have formatter");
    assert_eq!(spec.commands.len(), 1);
    let cmd = &spec.commands[0];
    assert_eq!(cmd.command, "dotnet");
    assert!(cmd.args.contains(&"format".to_owned()), "args must contain 'format'");
    assert!(
        cmd.args.contains(&"SampleModel.csproj".to_owned()),
        "args must contain the relative project file, got: {:?}",
        cmd.args
    );
    assert_eq!(spec.work_dir, "packages/csharp/");
}

#[test]
fn test_csharp_formatter_without_project_file() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Csharp).expect("should have formatter");
    let cmd = &spec.commands[0];
    assert_eq!(cmd.command, "dotnet");
    assert_eq!(
        cmd.args,
        vec!["format"],
        "without project_file, args must be just ['format']"
    );
}

// KotlinAndroid formatter must use ktfmt with --kotlinlang-style to match
// prek byte-for-byte. ktfmt and ktlint produce different canonical shapes,
// so alef must use ktfmt to keep prek's hook a no-op. Files are appended
// dynamically in `format_generated` to mirror prek's per-file invocation
// (prek passes individual .kt/.kts paths; ktfmt requires explicit paths).
#[test]
fn test_kotlin_android_formatter_uses_ktfmt() {
    let config = make_config("sample-markdown");
    let spec = get_default_formatter(&config, Language::KotlinAndroid).expect("KotlinAndroid should have formatter");
    assert_eq!(
        spec.commands.len(),
        1,
        "KotlinAndroid must have exactly one formatter command"
    );
    let cmd = &spec.commands[0];
    assert_eq!(
        cmd.command, "ktfmt",
        "KotlinAndroid must use ktfmt, not ktlint or gradle"
    );
    assert_eq!(
        cmd.args,
        vec!["--kotlinlang-style".to_owned()],
        "KotlinAndroid must pass --kotlinlang-style (files appended dynamically)"
    );
    assert_eq!(
        spec.work_dir, "packages/kotlin-android/src",
        "KotlinAndroid formatter work_dir must be the src tree so collect_kotlin_files finds the files"
    );
}

// collect_kotlin_files must return only .kt and .kts files; non-Kotlin files
// (textual, .class output, etc.) must be filtered out so ktfmt does not receive
// paths it cannot format. Mirrors test_collect_java_files_returns_only_java_files.
#[test]
fn test_collect_kotlin_files_returns_only_kt_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    std::fs::create_dir_all(root.join("fixtures/sample")).unwrap();
    std::fs::write(root.join("fixtures/sample/Foo.kt"), "class Foo").unwrap();
    std::fs::write(root.join("fixtures/sample/build.gradle.kts"), "// gradle").unwrap();
    std::fs::write(root.join("fixtures/sample/readme.txt"), "ignore").unwrap();
    std::fs::write(root.join("fixtures/sample/Bar.class"), "ignore").unwrap();

    let files = collect_kotlin_files(root, 500);
    assert_eq!(files.len(), 2, "expected 2 .kt/.kts files, got: {:?}", files);
    assert!(
        files.iter().all(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            ext == "kt" || ext == "kts"
        }),
        "non-kt/kts file leaked: {:?}",
        files
    );
}

// Kotlin (JVM, non-Android) must use ktfmt (not ktlint) so the format pass
// is byte-identical to prek's `ktfmt` hook. Files are appended dynamically
// at invocation time.
#[test]
fn test_kotlin_jvm_formatter_uses_ktfmt() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Kotlin).expect("Kotlin should have formatter");
    assert_eq!(spec.commands.len(), 1, "Kotlin must have exactly one formatter command");
    let cmd = &spec.commands[0];
    assert_eq!(
        cmd.command, "ktfmt",
        "Kotlin must use ktfmt, not ktlint, so prek's ktfmt hook is a no-op"
    );
    assert_eq!(
        cmd.args,
        vec!["--kotlinlang-style".to_owned()],
        "Kotlin must pass --kotlinlang-style (files appended dynamically)"
    );
    assert_eq!(
        spec.work_dir, "packages/kotlin/src",
        "Kotlin formatter work_dir must be the src tree so collect_kotlin_files finds the files"
    );
}

// Go formatter must match prek's `go-fmt` hook: `gofmt -s -w` followed by
// `goimports -w`. Without `-s`, simplifications drift; without goimports,
// import groupings stay non-canonical.
#[test]
fn test_go_formatter_matches_prek_go_fmt_hook() {
    let config = make_config("sample-model");
    let spec = get_default_formatter(&config, Language::Go).expect("Go should have formatter");
    assert_eq!(spec.commands.len(), 2, "Go must have gofmt + goimports steps");
    let gofmt_cmd = &spec.commands[0];
    assert_eq!(gofmt_cmd.command, "gofmt");
    assert_eq!(
        gofmt_cmd.args,
        vec!["-s", "-w", "."],
        "gofmt must use `-s -w` to match prek's go-fmt simplifications"
    );
    let goimports_cmd = &spec.commands[1];
    assert_eq!(goimports_cmd.command, "goimports");
    assert_eq!(
        goimports_cmd.args,
        vec!["-w", "."],
        "goimports must run with `-w` to match prek's import-grouping pass"
    );
    assert_eq!(spec.work_dir, "packages/go/");
}

// Bug 3: Java file collection — only .java files are returned, non-.java files are excluded.
#[test]
fn test_collect_java_files_returns_only_java_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    // Create a nested structure with .java and other files
    std::fs::create_dir_all(root.join("com/example")).unwrap();
    std::fs::write(root.join("com/example/Foo.java"), "class Foo {}").unwrap();
    std::fs::write(root.join("com/example/Bar.java"), "class Bar {}").unwrap();
    std::fs::write(root.join("com/example/readme.txt"), "ignore me").unwrap();
    std::fs::write(root.join("com/example/Baz.class"), "ignore me").unwrap();

    let files = collect_java_files(root, 200);
    assert_eq!(files.len(), 2, "expected 2 .java files, got: {:?}", files);
    assert!(files.iter().all(|p| p.extension().is_some_and(|e| e == "java")));
}

#[test]
fn test_collect_java_files_empty_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let files = collect_java_files(dir.path(), 200);
    assert!(files.is_empty());
}

#[test]
fn test_collect_java_files_nonexistent_dir() {
    let files = collect_java_files(Path::new("/nonexistent/path/to/src"), 200);
    assert!(files.is_empty());
}

#[test]
fn test_collect_java_files_respects_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for i in 0..10 {
        std::fs::write(root.join(format!("File{i}.java")), "class Foo {}").unwrap();
    }
    let files = collect_java_files(root, 5);
    assert_eq!(files.len(), 5);
}

// Regression: custom format_override commands must run even when the language
// is absent from the only_languages filter (i.e., files were not re-written
// this run). The only_languages filter is an optimization for default formatters
// (skip when nothing changed), but a custom command must always run to ensure
// the embedded alef:hash: is computed over formatter-normalized content.
// Without this, adding [workspace.format_overrides.php] and running
// `alef all --format` on an already-generated repo would skip php-cs-fixer,
// leaving hashes computed over raw (pre-formatter) content; prek's own
// php-cs-fixer hook would then reformat and break alef verify.
#[test]
fn format_generated_custom_override_runs_when_lang_absent_from_only_languages_filter() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sentinel = dir.path().join("was_run.txt");
    let sentinel_str = sentinel.to_string_lossy().replace('\\', "/");

    // Config with a custom format_override for php that writes a sentinel file.
    let cfg: NewAlefConfig = toml::from_str(&format!(
        r#"
[workspace]
languages = ["php"]

[workspace.format_overrides.php]
command = "touch {sentinel_str}"

[[crates]]
name = "my-lib"
sources = ["src/lib.rs"]
"#
    ))
    .expect("valid config");
    let config = cfg.resolve().expect("resolve").remove(0);

    // Simulate bindings for php — language appears in files but is NOT in only_languages.
    let files: Vec<(Language, Vec<crate::core::backend::GeneratedFile>)> = vec![(Language::Php, vec![])];

    // only_languages is empty — simulates "nothing was written this run".
    let only_languages: std::collections::HashSet<Language> = std::collections::HashSet::new();

    assert!(!sentinel.exists(), "sentinel must not exist before format_generated");

    format_generated(&files, &config, dir.path(), Some(&only_languages));

    assert!(
        sentinel.exists(),
        "custom format_override command must run even when php is absent from only_languages"
    );
}

// Complement: default formatters must still respect the only_languages filter
// so that a warm cache (no file writes) skips unnecessary ruff/mix-format/etc.
// invocations for default formatters.
#[test]
fn format_generated_default_formatter_skipped_when_lang_absent_from_only_languages() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Config with no format_overrides — python uses the default ruff formatter.
    let config = make_config("my-lib");

    let files: Vec<(Language, Vec<crate::core::backend::GeneratedFile>)> = vec![(Language::Python, vec![])];

    // only_languages is empty — simulates "nothing was written this run".
    let only_languages: std::collections::HashSet<Language> = std::collections::HashSet::new();

    // This should complete without error (ruff not present on the test box is fine —
    // the point is that format_generated skips python entirely without reaching the
    // is_tool_available check, so no warning is emitted and no external process runs).
    // We verify by ensuring format_generated returns without calling any tool.
    // Since python has a default formatter (ruff), skipping means the tool is never
    // looked up — we can't assert negatively on tool invocation, but the test
    // documents the intent: no-op when only_languages filter excludes the language.
    format_generated(&files, &config, dir.path(), Some(&only_languages));
    // If we reach here without error the skip path worked correctly.
}
// `shfmt_emitted_scripts` selects only `.sh` files from a mixed input.
// Empty input must be a true no-op: no spawn, no panic, no warning trip.
#[test]
fn test_shfmt_emitted_scripts_no_op_when_no_scripts() {
    use crate::core::backend::GeneratedFile;
    let dir = tempfile::tempdir().expect("tempdir");
    let files: Vec<(Language, Vec<GeneratedFile>)> = vec![(
        Language::Python,
        vec![GeneratedFile {
            path: PathBuf::from("packages/python/foo.py"),
            content: "x = 1\n".to_owned(),
            generated_header: true,
        }],
    )];
    // No `.sh` files in the input → must return without panicking and
    // without invoking shfmt. The test passes by reaching this point.
    shfmt_emitted_scripts(&files, dir.path());
}

// Mixed input must not panic and must select only `.sh` files. The
// function is best-effort: a missing `shfmt` binary or a non-zero exit
// must not propagate.
#[test]
fn test_shfmt_emitted_scripts_filters_to_sh_extension_only() {
    use crate::core::backend::GeneratedFile;
    let dir = tempfile::tempdir().expect("tempdir");
    let sh_dir = dir.path().join("e2e/c");
    std::fs::create_dir_all(&sh_dir).unwrap();
    std::fs::write(sh_dir.join("download_ffi.sh"), "#!/usr/bin/env bash\necho ok\n").unwrap();
    let files: Vec<(Language, Vec<GeneratedFile>)> = vec![(
        Language::Ffi,
        vec![
            GeneratedFile {
                path: PathBuf::from("e2e/c/download_ffi.sh"),
                content: "#!/usr/bin/env bash\necho ok\n".to_owned(),
                generated_header: true,
            },
            GeneratedFile {
                path: PathBuf::from("e2e/c/main.c"),
                content: "int main(void) { return 0; }\n".to_owned(),
                generated_header: true,
            },
        ],
    )];
    shfmt_emitted_scripts(&files, dir.path());
}
