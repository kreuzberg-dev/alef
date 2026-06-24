use alef::core::config::{
    alef_config_schema, check_alef_config_schema, render_alef_config_schema, write_alef_config_schema,
};

#[test]
fn generated_schema_contains_versioned_release_metadata() {
    let schema = alef_config_schema("1.2.3").expect("schema generation succeeds");

    assert_eq!(
        schema.get("$id").and_then(serde_json::Value::as_str),
        Some("https://github.com/xberg-io/alef/releases/download/v1.2.3/alef.schema.json")
    );
    assert_eq!(schema.get("version").and_then(serde_json::Value::as_str), Some("1.2.3"));
    assert_eq!(
        schema.get("x-alef-version").and_then(serde_json::Value::as_str),
        Some("1.2.3")
    );
    assert_eq!(
        schema.get("$schema").and_then(serde_json::Value::as_str),
        Some("https://json-schema.org/draft/2020-12/schema")
    );
}

#[test]
fn schema_check_fails_when_file_is_stale() {
    let dir = tempfile::tempdir().expect("tempdir");
    let schema_path = dir.path().join("alef.schema.json");
    write_alef_config_schema(&schema_path, "1.2.3").expect("schema writes");

    let error = check_alef_config_schema(&schema_path, "1.2.4").expect_err("stale schema should fail");

    assert!(
        error.to_string().contains("is stale"),
        "expected stale schema error, got: {error}"
    );
}

#[test]
fn repository_alef_toml_validates_against_generated_schema() {
    let schema = alef_config_schema(env!("CARGO_PKG_VERSION")).expect("schema generation succeeds");
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let toml_value: toml::Value = toml::from_str(include_str!("../alef.toml")).expect("alef.toml parses");
    let json_value = serde_json::to_value(toml_value).expect("TOML value converts to JSON");

    assert!(
        validator.is_valid(&json_value),
        "repository alef.toml must validate against the generated schema"
    );
}

#[test]
fn committed_schema_matches_current_package_version() {
    let expected = render_alef_config_schema(env!("CARGO_PKG_VERSION")).expect("schema renders");
    let actual = include_str!("../schemas/alef.schema.json");

    assert_eq!(actual, expected);
}
