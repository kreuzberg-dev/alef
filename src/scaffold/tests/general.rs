use super::*;

#[test]
fn test_scaffold_csharp_omits_repository_when_unconfigured() {
    let config = minimal_config_from_toml("");
    let api = test_api();
    let all_files = scaffold(&api, &config, &[Language::Csharp]).unwrap();
    let files = language_files(&all_files);
    let csproj = files
        .iter()
        .find(|f| f.path.to_string_lossy().ends_with(".csproj"))
        .expect("C# project file must be emitted");

    assert!(
        !csproj.content.contains("<RepositoryUrl>"),
        "unconfigured C# scaffold must not invent repository metadata:\n{}",
        csproj.content
    );
}

#[test]
fn test_scaffold_wasm_omits_repository_when_unconfigured() {
    let config = minimal_config_from_toml("");
    let api = test_api();
    let all_files = scaffold(&api, &config, &[Language::Wasm]).unwrap();
    let files = language_files(&all_files);
    let package_json = files
        .iter()
        .find(|f| f.path == Path::new("crates/my-lib-wasm/package.json"))
        .expect("WASM package.json must be emitted");
    let parsed: serde_json::Value =
        serde_json::from_str(&package_json.content).expect("emitted package.json must be valid JSON");

    assert!(
        parsed.get("repository").is_none(),
        "unconfigured WASM manifest must not invent repository metadata:\n{}",
        package_json.content
    );
}

#[test]
fn test_scaffold_java_requires_publish_metadata() {
    let config = minimal_config_from_toml("");
    let api = test_api();
    let err = scaffold(&api, &config, &[Language::Java]).expect_err("Java scaffold must require publish metadata");

    assert!(
        err.to_string()
            .contains("Java scaffold requires package metadata repository"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_scaffold_kotlin_requires_publish_metadata() {
    let config = minimal_config_from_toml("");
    let api = test_api();
    let err = scaffold(&api, &config, &[Language::Kotlin]).expect_err("Kotlin scaffold must require publish metadata");

    assert!(
        err.to_string()
            .contains("Kotlin scaffold requires package metadata repository"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_scaffold_r_requires_authors() {
    let config = minimal_config_from_toml("");
    let api = test_api();
    let err = scaffold(&api, &config, &[Language::R]).expect_err("R scaffold must require authors");

    assert!(
        err.to_string().contains("R scaffold requires package metadata authors"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_scaffold_multiple() {
    let config = test_config();
    let api = test_api();
    let all_files = scaffold(&api, &config, &[Language::Python, Language::Node]).unwrap();
    let files = language_files(&all_files);
    // Python: 3 files; Node: 11 files (parent manifest, loader, platform manifests, Cargo.toml).
    assert_eq!(files.len(), 14);
}

#[test]
fn test_scaffold_gitattributes_covers_all_generated_dirs() {
    // Test that .gitattributes is emitted and covers: language package dirs,
    // binding crate dirs (py/php/ffi/jni are separate from package_dir), and e2e/.
    let config = test_config();
    let api = test_api();

    // Python + Node: the default test_config languages.
    let all_files = scaffold(&api, &config, &[Language::Python, Language::Node]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted by scaffold");

    assert!(
        !ga.generated_header,
        "generated_header must be false — create-once seed"
    );

    let content = &ga.content;
    // Package directories
    assert!(content.contains("packages/python/**"), "must cover Python package dir");
    assert!(content.contains("crates/my-lib-node/**"), "must cover Node crate dir");
    // Binding crate separate from package_dir
    assert!(content.contains("crates/my-lib-py/**"), "must cover PyO3 binding crate");
    // e2e is always included regardless of language selection
    assert!(content.contains("e2e/**"), "must cover e2e test output");
    // All entries carry the linguist attribute
    for line in content.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        assert!(
            line.ends_with("linguist-generated=true"),
            "every non-comment line must set linguist-generated=true, got: {line}"
        );
    }
}

#[test]
fn test_scaffold_gitattributes_ffi_and_jni_use_crate_dirs() {
    // FFI and JNI don't have packages/ dirs — their output is the binding crate itself.
    let config = test_config();
    let api = test_api();

    let all_files = scaffold(&api, &config, &[Language::Ffi, Language::Jni]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted");

    let content = &ga.content;
    assert!(content.contains("crates/my-lib-ffi/**"), "must cover FFI crate dir");
    assert!(content.contains("crates/my-lib-jni/**"), "must cover JNI crate dir");
    assert!(!content.contains("packages/ffi"), "must not emit bogus packages/ffi");
    assert!(!content.contains("packages/jni"), "must not emit bogus packages/jni");
}

#[test]
fn test_scaffold_gitattributes_kotlin_native_uses_kotlin_native_dir() {
    use crate::core::config::NewAlefConfig;

    let cfg: NewAlefConfig = toml::from_str(
        r#"
[workspace]
languages = ["kotlin"]

[[crates]]
name = "my-lib"
sources = ["src/lib.rs"]

[crates.scaffold]
description = "Test"
license = "MIT"
repository = "https://github.com/test/my-lib"

[crates.kotlin]
target = "native"
"#,
    )
    .unwrap();
    let config = cfg.resolve().unwrap().remove(0);
    let api = test_api();

    let all_files = scaffold(&api, &config, &[Language::Kotlin]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted");

    assert!(
        ga.content.contains("packages/kotlin-native/**"),
        "native target must use packages/kotlin-native, got:\n{}",
        ga.content
    );
    assert!(
        !ga.content.contains("packages/kotlin/**"),
        "native target must not emit JVM dir, got:\n{}",
        ga.content
    );
}

#[test]
fn test_scaffold_gitattributes_kotlin_mpp_uses_kotlin_mpp_dir() {
    use crate::core::config::NewAlefConfig;

    let cfg: NewAlefConfig = toml::from_str(
        r#"
[workspace]
languages = ["kotlin"]

[[crates]]
name = "my-lib"
sources = ["src/lib.rs"]

[crates.scaffold]
description = "Test"
license = "MIT"
repository = "https://github.com/test/my-lib"

[crates.kotlin]
mode = "kmp"
"#,
    )
    .unwrap();
    let config = cfg.resolve().unwrap().remove(0);
    let api = test_api();

    let all_files = scaffold(&api, &config, &[Language::Kotlin]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted");

    assert!(
        ga.content.contains("packages/kotlin-mpp/**"),
        "kmp mode must use packages/kotlin-mpp, got:\n{}",
        ga.content
    );
    assert!(
        !ga.content.contains("packages/kotlin/**"),
        "kmp mode must not emit JVM dir, got:\n{}",
        ga.content
    );
}

#[test]
fn test_scaffold_gitattributes_kotlin_multiplatform_target_uses_kotlin_mpp_dir() {
    // target = "multiplatform" (no mode) must also resolve to packages/kotlin-mpp/
    use crate::core::config::NewAlefConfig;

    let cfg: NewAlefConfig = toml::from_str(
        r#"
[workspace]
languages = ["kotlin"]

[[crates]]
name = "my-lib"
sources = ["src/lib.rs"]

[crates.scaffold]
description = "Test"
license = "MIT"
repository = "https://github.com/test/my-lib"

[crates.kotlin]
target = "multiplatform"
"#,
    )
    .unwrap();
    let config = cfg.resolve().unwrap().remove(0);
    let api = test_api();

    let all_files = scaffold(&api, &config, &[Language::Kotlin]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted");

    assert!(
        ga.content.contains("packages/kotlin-mpp/**"),
        "target=multiplatform must use packages/kotlin-mpp, got:\n{}",
        ga.content
    );
}

#[test]
fn test_scaffold_gitattributes_kotlin_android_uses_kotlin_android_dir() {
    let config = test_config();
    let api = test_api();

    let all_files = scaffold(&api, &config, &[Language::KotlinAndroid]).unwrap();
    let ga = all_files
        .iter()
        .find(|f| f.path == std::path::Path::new(".gitattributes"))
        .expect(".gitattributes must be emitted");

    assert!(
        ga.content.contains("packages/kotlin-android/**"),
        "KotlinAndroid must use packages/kotlin-android, got:\n{}",
        ga.content
    );
}

#[test]
fn wasm_package_name_strips_node_suffix_from_scoped_package() {
    // @scope/foo-node  →  @scope/foo-wasm  (not @scope/foo-node-wasm)
    let config = test_config_from_toml(
        r#"
[crates.node]
package_name = "@scope/foo-node"
"#,
    );
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Wasm]).unwrap();
    let pkg_json = files
        .iter()
        .find(|f| f.path.ends_with("package.json"))
        .expect("wasm scaffold must emit package.json");
    assert!(
        pkg_json.content.contains("\"@scope/foo-wasm\""),
        "expected @scope/foo-wasm, got:\n{}",
        pkg_json.content
    );
    assert!(
        !pkg_json.content.contains("foo-node-wasm"),
        "must not emit foo-node-wasm, got:\n{}",
        pkg_json.content
    );
}

#[test]
fn wasm_package_name_strips_node_suffix_from_unscoped_package() {
    // foo-node  →  foo-wasm  (not foo-node-wasm)
    let config = test_config_from_toml(
        r#"
[crates.node]
package_name = "foo-node"
"#,
    );
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Wasm]).unwrap();
    let pkg_json = files
        .iter()
        .find(|f| f.path.ends_with("package.json"))
        .expect("wasm scaffold must emit package.json");
    assert!(
        pkg_json.content.contains("\"foo-wasm\""),
        "expected foo-wasm, got:\n{}",
        pkg_json.content
    );
}

#[test]
fn wasm_package_name_fallback_when_no_node_suffix() {
    // foo  →  foo-wasm  (no -node suffix present, no stripping)
    let config = test_config();
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Wasm]).unwrap();
    let pkg_json = files
        .iter()
        .find(|f| f.path.ends_with("package.json"))
        .expect("wasm scaffold must emit package.json");
    // Default node_package_name for crate "my-lib" is "my-lib" (no -node suffix).
    // Stripping "-node" is a no-op → wasm name is "my-lib-wasm".
    assert!(
        pkg_json.content.contains("\"my-lib-wasm\""),
        "expected my-lib-wasm, got:\n{}",
        pkg_json.content
    );
}

#[test]
fn wasm_package_name_uses_explicit_wasm_config() {
    let config = test_config_from_toml(
        r#"
[crates.node]
package_name = "@scope/foo-node"

[crates.wasm]
package_name = "@scope/foo-web"
"#,
    );
    let api = test_api();
    let files = scaffold(&api, &config, &[Language::Wasm]).unwrap();
    let pkg_json = files
        .iter()
        .find(|f| f.path.ends_with("package.json"))
        .expect("wasm scaffold must emit package.json");
    let parsed: serde_json::Value = serde_json::from_str(&pkg_json.content).expect("valid wasm package.json");
    assert_eq!(parsed["name"], "@scope/foo-web");
    assert_eq!(parsed["publishConfig"]["access"], "public");
    assert_eq!(parsed["main"], "pkg/nodejs/my_lib_wasm.js");
    assert_eq!(parsed["types"], "pkg/nodejs/my_lib_wasm.d.ts");
}

#[test]
fn test_scaffold_r_authors_r_parses_name_and_email() {
    let config = test_config_from_toml(
        r#"
[crates.package_metadata]
authors = ["Ada Lovelace <ada@example.com>"]
"#,
    );
    let api = test_api();
    let all_files = scaffold(&api, &config, &[Language::R]).unwrap();
    let files = language_files(&all_files);
    let description = files.iter().find(|f| f.path.ends_with("DESCRIPTION")).unwrap();

    assert!(
        description
            .content
            .contains(r#"Authors@R: person("Ada", "Lovelace", email = "ada@example.com", role = c("aut", "cre"))"#),
        "DESCRIPTION must split Authors@R name/email; content:\n{}",
        description.content
    );
}
