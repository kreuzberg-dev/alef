use super::*;

/// Regression test: `sync_versions` must re-apply the `v__ALEF_SWIFT_VERSION__`
/// placeholder substitution in `Package.swift` AFTER `regenerate_scaffold_after_sync`
/// overwrites it with the template form. Without this second pass, the URL in the
/// version tag's Package.swift contains a literal `v__ALEF_SWIFT_VERSION__` rather
/// than the resolved `v{version}`, causing SwiftPM resolution to 404.
///
/// This test sets `no_regen = false` (the default), enabling automatic scaffold
/// regeneration after version sync. It verifies that even though scaffold regen
/// writes the placeholder back, the sync flow re-applies the substitution and the
/// final `Package.swift` contains the resolved version string.
#[test]
fn sync_versions_reapplies_swift_version_placeholder_after_scaffold_regen() {
    use crate::core::config::NewAlefConfig;
    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let original_cwd = std::env::current_dir().expect("cwd");

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Setup Cargo.toml with version 1.5.0
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"1.5.0\"\n\n[workspace]\nresolver = \"2\"\nmembers = []\n",
    )
    .expect("write Cargo.toml");

    // Create a root Package.swift with the version placeholder.
    // This simulates what scaffold_swift emits — the file with __ALEF_SWIFT_VERSION__
    // so it stays stable across VCS and version bumps.
    let initial_pkg = concat!(
        "// swift-tools-version: 6.0\n",
        "import PackageDescription\n",
        "let package = Package(\n",
        "  name: \"RustLib\",\n",
        "  targets: [\n",
        "    .binaryTarget(\n",
        "      name: \"RustBridge\",\n",
        "      url: \"https://github.com/example/rustlib/releases/download/v__ALEF_SWIFT_VERSION__/RustLib-rs.artifactbundle.zip\",\n",
        "      checksum: \"abc123def456\"\n",
        "    ),\n",
        "  ]\n",
        ")\n",
    );
    std::fs::write(root.join("Package.swift"), initial_pkg).expect("write initial Package.swift");

    // Create a Swift binding crate directory and Cargo.toml so the swift binding check passes.
    let swift_crate_dir = root.join("crates/rustlib-swift");
    std::fs::create_dir_all(&swift_crate_dir).expect("mkdir crates/rustlib-swift");
    std::fs::write(
        swift_crate_dir.join("Cargo.toml"),
        "[package]\nname = \"rustlib-swift\"\nversion = \"1.5.0\"\n",
    )
    .expect("write swift crate Cargo.toml");

    // Create a scaffold template that will be used when regen runs.
    // This simulates the template that scaffold_swift would write back.
    let scaffold_dir = root.join("src/scaffold/templates/swift");
    std::fs::create_dir_all(&scaffold_dir).expect("mkdir scaffold template dir");
    std::fs::write(
        scaffold_dir.join("Package.swift.jinja"),
        concat!(
            "// swift-tools-version: 6.0\n",
            "import PackageDescription\n",
            "let package = Package(\n",
            "  name: \"RustLib\",\n",
            "  targets: [\n",
            "    .binaryTarget(\n",
            "      name: \"RustBridge\",\n",
            "      url: \"https://github.com/example/rustlib/releases/download/v__ALEF_SWIFT_VERSION__/RustLib-rs.artifactbundle.zip\",\n",
            "      checksum: \"abc123def456\"\n",
            "    ),\n",
            "  ]\n",
            ")\n",
        ),
    )
    .expect("write scaffold template");

    // Write alef.toml with swift configured.
    // regenerate_scaffold_after_sync will run when no_regen=false (the default),
    // regardless of e2e config, since scaffold emission doesn't depend on e2e.
    let alef_toml = format!(
        concat!(
            "[workspace]\n",
            "languages = [\"swift\"]\n",
            "[[crates]]\n",
            "name = \"rustlib\"\n",
            "sources = []\n",
            "version_from = \"{}\"\n",
        ),
        root.join("Cargo.toml").display().to_string().replace('\\', "/")
    );
    let alef_toml_path = root.join("alef.toml");
    std::fs::write(&alef_toml_path, &alef_toml).expect("write alef.toml");

    let cfg: NewAlefConfig = toml::from_str(&alef_toml).expect("parse alef.toml");
    let mut resolved = cfg.resolve().expect("resolve config");
    let resolved_cfg = resolved.remove(0);

    // Call sync_versions with no_regen=false (the default), which triggers scaffold regen.
    // This is the key test scenario: scaffold regen will write back the placeholder,
    // then the second-pass substitution must fix it.
    std::env::set_current_dir(root).expect("set_current_dir");
    let sync_result = sync_versions(&resolved_cfg, &alef_toml_path, None, false, true, None);
    let _ = std::env::set_current_dir(&original_cwd);
    sync_result.expect("sync_versions must succeed");

    // Read the final Package.swift.
    let final_pkg =
        std::fs::read_to_string(root.join("Package.swift")).expect("read final Package.swift");

    // The URL must contain the resolved version v1.5.0, NOT the placeholder.
    assert!(
        final_pkg.contains("v1.5.0"),
        "Package.swift must contain resolved version v1.5.0, got:\n{final_pkg}"
    );

    // The placeholder must be completely gone.
    assert!(
        !final_pkg.contains("v__ALEF_SWIFT_VERSION__"),
        "Package.swift must not contain the literal placeholder v__ALEF_SWIFT_VERSION__, got:\n{final_pkg}"
    );
    assert!(
        !final_pkg.contains("__ALEF_SWIFT_VERSION__"),
        "Package.swift must not contain the placeholder __ALEF_SWIFT_VERSION__, got:\n{final_pkg}"
    );
}
