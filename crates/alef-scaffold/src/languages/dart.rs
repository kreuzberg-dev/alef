use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::ir::ApiSurface;
use alef_core::template_versions::{cargo, pub_dev, toolchain};
use std::path::PathBuf;

pub(crate) fn scaffold_dart(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let version = &api.version;
    let pubspec_name = config.dart_pubspec_name();

    let flutter_rust_bridge = cargo::FLUTTER_RUST_BRIDGE;
    let dart_sdk = toolchain::DART_SDK_CONSTRAINT;
    let test_package = pub_dev::TEST_PACKAGE;
    let lints = pub_dev::LINTS;

    // flutter_rust_bridge is listed under `dependencies:` because the generated
    // Dart wrapper imports its runtime types. For pure-Dart (non-Flutter)
    // consumers the FRB pub package is plain Dart and pulls no Flutter SDK; for
    // Flutter consumers the same dep resolves to the Flutter-augmented variant.
    // No conditional dep block is needed — the package author can override
    // by setting `[dart] frb_version` to a `git:` reference if a forked variant
    // is required.
    let pubspec_yaml = format!(
        r#"name: {name}
description: Generated Dart bindings via flutter_rust_bridge
version: {version}
environment:
  sdk: '{dart_sdk}'
dependencies:
  # FRB runtime is pure-Dart; works in both Flutter and server-Dart contexts.
  flutter_rust_bridge: '{flutter_rust_bridge}'
dev_dependencies:
  test: '{test_package}'
  lints: '{lints}'
"#,
        name = pubspec_name,
        version = version,
    );

    let analysis_options_yaml = "include: package:lints/recommended.yaml\n";

    let gitignore = ".dart_tool/\nbuild/\npubspec.lock\n";

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from("packages/dart/pubspec.yaml"),
            content: pubspec_yaml,
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/dart/analysis_options.yaml"),
            content: analysis_options_yaml.to_string(),
            generated_header: false,
        },
        GeneratedFile {
            path: PathBuf::from("packages/dart/.gitignore"),
            content: gitignore.to_string(),
            generated_header: false,
        },
    ])
}
