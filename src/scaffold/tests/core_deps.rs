use super::*;

// ---------------------------------------------------------------------------
// Dual-form core-facade dependency (`{ version = "X.Y.Z", path = "..." }`).
//
// The scaffolded binding-crate Cargo.toml must emit its workspace-member
// core-facade dependency in dual form so in-repo dev path builds keep working
// AND cargo-package flows (maturin sdist, `cargo package`) can strip the path
// to a registry version-dependency. The version equals the workspace version
// (here `api.version` == "0.1.0"), the path is preserved unchanged, features
// are preserved, and external (non-member) deps are emitted untouched.
// ---------------------------------------------------------------------------

/// Locate the binding-crate `Cargo.toml` generated for `lang` and return its
/// content. Filters out the Ruby `[lib]` Cargo (which lives under `native/`)
/// by matching the dependency-bearing manifest containing `[dependencies]`.
fn core_cargo_toml_for(lang: Language) -> String {
    let mut config = test_config();
    config.features = vec!["full".to_string(), "ocr".to_string()];
    let api = test_api();
    let all_files = scaffold(&api, &config, &[lang]).unwrap();
    let files = language_files(&all_files);
    files
        .iter()
        .find(|f| f.path.ends_with("Cargo.toml") && f.content.contains("my-lib = {"))
        .map(|f| f.content.clone())
        .unwrap_or_else(|| panic!("no core Cargo.toml with `my-lib` dep emitted for {lang:?}"))
}

#[test]
fn render_core_dep_emits_dual_form_with_version_first() {
    let line = render_core_dep("my-lib", "../my-lib", "", "1.2.3");
    assert_eq!(line, r#"my-lib = { version = "1.2.3", path = "../my-lib" }"#);
}

#[test]
fn render_core_dep_preserves_features_suffix() {
    let line = render_core_dep("my-lib", "../my-lib", ", features = [\"full\", \"ocr\"]", "1.2.3");
    assert_eq!(
        line,
        r#"my-lib = { version = "1.2.3", path = "../my-lib", features = ["full", "ocr"] }"#
    );
}

#[test]
fn render_core_dep_falls_back_to_path_only_when_version_empty() {
    // Some unit fixtures (e.g. JNI with `ApiSurface::default()`) have no
    // resolvable workspace version; emit path-only rather than `version = ""`.
    let line = render_core_dep("my-lib", "../my-lib", "", "");
    assert_eq!(line, r#"my-lib = { path = "../my-lib" }"#);
}

#[test]
fn test_scaffold_python_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::Python);
    assert!(
        content.contains(r#"my-lib = { version = "0.1.0", path = "../my-lib", features = ["full", "ocr"] }"#),
        "python core dep must be dual form with version + path + features; content:\n{content}"
    );
    // External deps unchanged.
    assert!(
        content.contains(r#"serde_json = "1""#),
        "external serde_json unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_node_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::Node);
    assert!(
        content.contains(r#"my-lib = { version = "0.1.0", path = "../my-lib", features = ["full", "ocr"] }"#),
        "node core dep must be dual form; content:\n{content}"
    );
    assert!(
        content.contains(r#"serde = { version = "1", features = ["derive"] }"#),
        "external serde unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_ruby_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::Ruby);
    assert!(
        content.contains(
            r#"my-lib = { version = "0.1.0", path = "../../../../../crates/my-lib", features = ["full", "ocr"] }"#
        ),
        "ruby core dep must be dual form with the deep crates path preserved; content:\n{content}"
    );
    assert!(
        content.contains("magnus = "),
        "external magnus unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_php_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::Php);
    assert!(
        content.contains(r#"my-lib = { version = "0.1.0", path = "../my-lib", features = ["full", "ocr"] }"#),
        "php core dep must be dual form; content:\n{content}"
    );
    assert!(
        content.contains("ext-php-rs = "),
        "external ext-php-rs unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_elixir_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::Elixir);
    assert!(
        content.contains(
            r#"my-lib = { version = "0.1.0", path = "../../../../crates/my-lib", features = ["full", "ocr"] }"#
        ),
        "elixir core dep must be dual form with the deep crates path preserved; content:\n{content}"
    );
    assert!(
        content.contains("rustler = "),
        "external rustler unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_r_core_dep_is_dual_form() {
    let content = core_cargo_toml_for(Language::R);
    assert!(
        content.contains(
            r#"my-lib = { version = "0.1.0", path = "../../../../crates/my-lib", features = ["full", "ocr"] }"#
        ),
        "r core dep must be dual form; content:\n{content}"
    );
    assert!(
        content.contains("extendr-api = "),
        "external extendr-api unchanged; content:\n{content}"
    );
}

#[test]
fn test_scaffold_swift_core_dep_is_dual_form() {
    // Swift's binding-crate Cargo.toml is emitted by the swift backend's
    // `gen_rust_crate::emit` (not the generic scaffold step), so assert the
    // dual form there. The path must be preserved for dev builds and the
    // workspace version injected for cargo-package flows.
    let config = test_config();
    let api = test_api(); // version "0.1.0", crate "my-lib"
    let files = crate::backends::swift::gen_rust_crate::emit(&api, &config).unwrap();
    let cargo = files
        .iter()
        .find(|f| f.path.ends_with("Cargo.toml"))
        .expect("swift Cargo.toml must be emitted");
    // core_dep_key is the Rust-ident form (`my_lib`); since it differs from the
    // cargo package name (`my-lib`) a `package = "..."` rename is appended after
    // the version + path. `core_path` is `../../..` for the same-as-workspace case.
    assert!(
        cargo
            .content
            .contains(r#"my_lib = { version = "0.1.0", path = "../../..", package = "my-lib" }"#),
        "swift core dep must be dual form (version + path) with package rename; content:\n{}",
        cargo.content
    );
    // External deps unchanged.
    assert!(
        cargo.content.contains(r#"serde_json = "1""#),
        "external serde_json unchanged; content:\n{}",
        cargo.content
    );
}

#[test]
fn test_scaffold_dev_path_build_form_preserved() {
    // The whole point of dual form: the `path` is still present (so in-repo dev
    // builds resolve the local member crate) AND a `version` is added (so
    // cargo-package can strip the path to a registry dep).
    for lang in [
        Language::Python,
        Language::Node,
        Language::Ruby,
        Language::Php,
        Language::Elixir,
        Language::R,
    ] {
        let content = core_cargo_toml_for(lang);
        let dep_line = content
            .lines()
            .find(|l| l.trim_start().starts_with("my-lib = {"))
            .unwrap_or_else(|| panic!("no my-lib dep line for {lang:?}"));
        assert!(
            dep_line.contains("path = "),
            "{lang:?}: dev-path-build path must be preserved: {dep_line}"
        );
        assert!(
            dep_line.contains(r#"version = "0.1.0""#),
            "{lang:?}: version must be injected: {dep_line}"
        );
    }
}
