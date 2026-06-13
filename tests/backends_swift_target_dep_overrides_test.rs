use alef::backends::swift::gen_rust_crate;
use alef::core::config::new_config::NewAlefConfig;
use alef::core::ir::ApiSurface;

fn api() -> ApiSurface {
    ApiSurface {
        crate_name: "demo-core".into(),
        version: "1.2.3".into(),
        ..Default::default()
    }
}

fn generated_cargo_toml(toml: &str) -> String {
    let cfg: NewAlefConfig = toml::from_str(toml).expect("test config must parse");
    let config = cfg.resolve().expect("test config must resolve").remove(0);
    let files = gen_rust_crate::emit(&api(), &config).expect("Swift Rust crate generation succeeds");

    files
        .into_iter()
        .find(|file| file.path.ends_with("Cargo.toml"))
        .expect("Cargo.toml is emitted")
        .content
}

#[test]
fn swift_target_dep_overrides_move_core_dep_to_target_tables() {
    let cargo = generated_cargo_toml(
        r#"
[workspace]
languages = ["swift"]

[[crates]]
name = "demo-core"
sources = ["src/lib.rs"]
features = ["full", "heic"]

[[crates.swift.target_dep_overrides]]
cfg = "target_os = \"ios\""
features = ["mobile"]
default_features = false
"#,
    );
    let default_dep = concat!(
        "demo_core = { version = \"1.2.3\", path = \"../../..\", ",
        "features = [\"full\", \"heic\"], package = \"demo-core\" }"
    );
    let override_dep = concat!(
        "demo_core = { version = \"1.2.3\", path = \"../../..\", ",
        "features = [\"mobile\"], default-features = false, package = \"demo-core\" }"
    );

    assert!(
        cargo.contains("[target.'cfg(not(target_os = \"ios\"))'.dependencies]"),
        "default Swift core dependency should be target-gated, got:\n{cargo}"
    );
    assert!(
        cargo.contains(default_dep),
        "default target branch should keep the configured Swift feature set, got:\n{cargo}"
    );
    assert!(
        cargo.contains("[target.'cfg(target_os = \"ios\")'.dependencies]"),
        "Swift override branch should use the configured cfg predicate, got:\n{cargo}"
    );
    assert!(
        cargo.contains(override_dep),
        "Swift override branch should emit the replacement feature set, got:\n{cargo}"
    );
    assert!(
        !cargo.contains("\n[dependencies]\ndemo_core ="),
        "core dependency should not stay in unconditional dependencies when target overrides are configured, got:\n{cargo}"
    );
}
