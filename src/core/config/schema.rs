//! JSON Schema generation for `alef.toml`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use schemars::schema_for;
use serde_json::{Value, json};

use super::NewAlefConfig;

pub const DEFAULT_SCHEMA_PATH: &str = "schemas/alef.schema.json";
const SCHEMA_TITLE: &str = "Alef configuration";
const SCHEMA_DESCRIPTION: &str = "JSON Schema for the JSON representation of alef.toml.";

/// Build the versioned JSON Schema for `alef.toml`.
pub fn alef_config_schema(version: &str) -> Result<Value> {
    let mut schema =
        serde_json::to_value(schema_for!(NewAlefConfig)).context("failed to serialize Alef config schema")?;
    let object = schema
        .as_object_mut()
        .context("schemars produced a non-object root schema")?;

    object.insert(
        "$id".to_string(),
        json!(format!(
            "https://github.com/xberg-io/alef/releases/download/v{version}/alef.schema.json"
        )),
    );
    object.insert("title".to_string(), json!(SCHEMA_TITLE));
    object.insert("description".to_string(), json!(SCHEMA_DESCRIPTION));
    object.insert("version".to_string(), json!(version));
    object.insert("x-alef-version".to_string(), json!(version));

    Ok(schema)
}

/// Render the versioned schema as pretty JSON with a trailing newline.
pub fn render_alef_config_schema(version: &str) -> Result<String> {
    let schema = alef_config_schema(version)?;
    let mut rendered = serde_json::to_string_pretty(&schema).context("failed to render Alef config schema")?;
    rendered.push('\n');
    Ok(rendered)
}

/// Write the schema file, creating parent directories as needed.
pub fn write_alef_config_schema(path: &Path, version: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create schema directory {}", parent.display()))?;
    }
    let rendered = render_alef_config_schema(version)?;
    std::fs::write(path, rendered).with_context(|| format!("failed to write schema {}", path.display()))
}

/// Verify that an existing schema file matches the generated schema.
pub fn check_alef_config_schema(path: &Path, version: &str) -> Result<()> {
    let expected = render_alef_config_schema(version)?;
    let actual = std::fs::read_to_string(path).with_context(|| format!("failed to read schema {}", path.display()))?;
    if actual != expected {
        bail!(
            "{} is stale; regenerate it with `alef schema --output {}`",
            path.display(),
            path.display()
        );
    }
    Ok(())
}
