use alef::core::config::{Language, PackageMetadataConfig, ResolvedCrateConfig};
use alef::core::ir::ApiSurface;
use alef::scaffold::scaffold;
use std::path::PathBuf;

#[test]
fn scaffold_elixir_nif_cargo_includes_brotli_allocator_patch() {
    // Regression test for spikard CI run 27503128389 (Elixir NIF compile failure).
    //
    // The Rustler NIF Cargo.toml must include a [patch.crates-io] section
    // pinning alloc-no-stdlib, alloc-stdlib, and brotli-decompressor to resolve
    // the transitive dependency conflict where brotli 8.0.x pulls alloc-no-stdlib 3.0
    // but other dependencies need 2.x, causing duplicate Allocator<T> trait definitions.
    //
    // See: https://github.com/kreuzberg-dev/alef/issues/XXXX
    let api = ApiSurface {
        crate_name: "demo".into(),
        version: "0.1.0".into(),
        types: vec![],
        functions: vec![],
        enums: vec![],
        errors: vec![],
        excluded_type_paths: Default::default(),
        excluded_trait_names: Default::default(),
        services: vec![],
        handler_contracts: vec![],
        unsupported_public_items: vec![],
        ..Default::default()
    };

    let config = ResolvedCrateConfig {
        name: "demo".to_string(),
        languages: vec![Language::Elixir],
        workspace_root: Some(PathBuf::from("/workspace")),
        package_metadata: Some(PackageMetadataConfig {
            license: Some("MIT".to_string()),
            ..PackageMetadataConfig::default()
        }),
        explicit_output: Default::default(),
        ..ResolvedCrateConfig::default()
    };

    let result = scaffold(&api, &config, &[Language::Elixir]).expect("scaffold failed");
    let cargo_toml_file = result
        .iter()
        .find(|file| file.path.to_string_lossy().ends_with("native/demo_nif/Cargo.toml"))
        .expect("Elixir scaffold should generate native/<nif>/Cargo.toml");

    let content = &cargo_toml_file.content;

    // Verify the [patch.crates-io] section is present
    assert!(
        content.contains("[patch.crates-io]"),
        "NIF Cargo.toml must include [patch.crates-io] section"
    );

    // Verify each crate pin is present with the correct version
    assert!(
        content.contains("alloc-no-stdlib = { version = \"=2.0.4\" }"),
        "NIF Cargo.toml must pin alloc-no-stdlib = 2.0.4"
    );

    assert!(
        content.contains("alloc-stdlib = { version = \"=0.2.2\" }"),
        "NIF Cargo.toml must pin alloc-stdlib = 0.2.2"
    );

    assert!(
        content.contains("brotli-decompressor = { version = \"=5.0.1\" }"),
        "NIF Cargo.toml must pin brotli-decompressor = 5.0.1"
    );

    // Verify the patch section appears after [dependencies]
    let deps_pos = content
        .find("[dependencies]")
        .expect("[dependencies] section must exist");
    let patch_pos = content
        .find("[patch.crates-io]")
        .expect("[patch.crates-io] section must exist");
    assert!(
        patch_pos > deps_pos,
        "[patch.crates-io] must appear after [dependencies]"
    );
}
