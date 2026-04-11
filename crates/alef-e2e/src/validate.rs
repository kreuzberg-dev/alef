//! JSON Schema validation for e2e fixture files.

use anyhow::{Context, Result};
use std::fmt;
use std::path::Path;

static FIXTURE_SCHEMA: &str = include_str!("../schema/fixture.schema.json");

/// A validation error with its source file and message.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Relative path of the fixture file that failed validation.
    pub file: String,
    /// Human-readable error message.
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

/// Validate all JSON fixture files in a directory against the fixture schema.
///
/// Returns a list of validation errors. An empty list means all fixtures are valid.
pub fn validate_fixtures(fixtures_dir: &Path) -> Result<Vec<ValidationError>> {
    let schema_value: serde_json::Value =
        serde_json::from_str(FIXTURE_SCHEMA).context("failed to parse embedded fixture schema")?;
    let validator = jsonschema::validator_for(&schema_value).context("failed to compile fixture schema")?;

    let mut errors = Vec::new();
    validate_recursive(fixtures_dir, fixtures_dir, &validator, &mut errors)?;
    Ok(errors)
}

fn validate_recursive(
    base: &Path,
    dir: &Path,
    validator: &jsonschema::Validator,
    errors: &mut Vec<ValidationError>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("failed to read directory: {}", dir.display()))?;

    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            validate_recursive(base, &path, validator, errors)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip schema files and files starting with _
            if filename == "schema.json" || filename.starts_with('_') {
                continue;
            }

            let relative = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().to_string();

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(ValidationError {
                        file: relative,
                        message: format!("failed to read file: {e}"),
                    });
                    continue;
                }
            };

            let value: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(ValidationError {
                        file: relative,
                        message: format!("invalid JSON: {e}"),
                    });
                    continue;
                }
            };

            for error in validator.iter_errors(&value) {
                errors.push(ValidationError {
                    file: relative.clone(),
                    message: format!("{} at {}", error, error.instance_path()),
                });
            }
        }
    }
    Ok(())
}
