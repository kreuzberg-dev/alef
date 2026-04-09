//! Rust e2e test code generator.
//!
//! Generates `e2e/rust/Cargo.toml` and `tests/{category}_test.rs` files from
//! JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

use crate::config::E2eConfig;
use crate::escape::{escape_rust, rust_raw_string, sanitize_filename, sanitize_ident};
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

/// Rust e2e test code generator.
pub struct RustE2eCodegen;

impl super::E2eCodegen for RustE2eCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();
        let output_base = PathBuf::from(&e2e_config.output).join("rust");

        // Resolve crate name and path from config.
        let crate_name = resolve_crate_name(e2e_config, alef_config);
        let crate_path = resolve_crate_path(e2e_config, &crate_name);
        let dep_name = crate_name.replace('-', "_");

        // Cargo.toml
        files.push(GeneratedFile {
            path: output_base.join("Cargo.toml"),
            content: render_cargo_toml(&crate_name, &dep_name, &crate_path),
            generated_header: true,
        });

        // Per-category test files.
        for group in groups {
            let fixtures: Vec<&Fixture> = group.fixtures.iter().filter(|f| !is_skipped(f, "rust")).collect();

            if fixtures.is_empty() {
                continue;
            }

            let filename = format!("{}_test.rs", sanitize_filename(&group.category));
            let content = render_test_file(&group.category, &fixtures, e2e_config, &dep_name);

            files.push(GeneratedFile {
                path: output_base.join("tests").join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "rust"
    }
}

// ---------------------------------------------------------------------------
// Config resolution helpers
// ---------------------------------------------------------------------------

fn resolve_crate_name(_e2e_config: &E2eConfig, alef_config: &AlefConfig) -> String {
    // Always use the Cargo package name (with hyphens) from alef.toml [crate].
    // The `crate_name` override in [e2e.call.overrides.rust] is for the Rust
    // import identifier, not the Cargo package name.
    alef_config.crate_config.name.clone()
}

fn resolve_crate_path(e2e_config: &E2eConfig, crate_name: &str) -> String {
    e2e_config
        .packages
        .get("rust")
        .and_then(|p| p.path.clone())
        .unwrap_or_else(|| format!("../../crates/{crate_name}"))
}

fn resolve_function_name(e2e_config: &E2eConfig) -> String {
    e2e_config
        .call
        .overrides
        .get("rust")
        .and_then(|o| o.function.clone())
        .unwrap_or_else(|| e2e_config.call.function.clone())
}

fn resolve_module(e2e_config: &E2eConfig, dep_name: &str) -> String {
    e2e_config
        .call
        .overrides
        .get("rust")
        .and_then(|o| o.module.clone())
        .unwrap_or_else(|| {
            if e2e_config.call.module.is_empty() {
                dep_name.to_string()
            } else {
                e2e_config.call.module.replace('-', "_")
            }
        })
}

fn is_skipped(fixture: &Fixture, language: &str) -> bool {
    fixture.skip.as_ref().is_some_and(|s| s.should_skip(language))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_cargo_toml(crate_name: &str, dep_name: &str, crate_path: &str) -> String {
    let e2e_name = format!("{dep_name}-e2e-rust");
    // When the crate name has hyphens, Cargo needs `package = "name-with-hyphens"`
    // because the dep key uses underscores (Rust identifier).
    let dep_spec = if crate_name != dep_name {
        format!("{dep_name} = {{ package = \"{crate_name}\", path = \"{crate_path}\" }}")
    } else {
        format!("{dep_name} = {{ path = \"{crate_path}\" }}")
    };
    format!(
        r#"[package]
name = "{e2e_name}"
version = "0.1.0"
edition = "2021"
publish = false

# Standalone crate — not part of the workspace to avoid circular dependency.
[workspace]

[dependencies]
{dep_spec}
serde_json = "1"
"#
    )
}

fn render_test_file(category: &str, fixtures: &[&Fixture], e2e_config: &E2eConfig, dep_name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "//! E2e tests for category: {category}");
    let _ = writeln!(out);

    let module = resolve_module(e2e_config, dep_name);
    let function_name = resolve_function_name(e2e_config);

    let _ = writeln!(out, "use {module}::{function_name};");
    let _ = writeln!(out);

    for fixture in fixtures {
        render_test_function(&mut out, fixture, e2e_config, dep_name);
        let _ = writeln!(out);
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_test_function(out: &mut String, fixture: &Fixture, e2e_config: &E2eConfig, dep_name: &str) {
    let fn_name = sanitize_ident(&fixture.id);
    let description = &fixture.description;
    let function_name = resolve_function_name(e2e_config);
    let result_var = &e2e_config.call.result_var;

    let _ = writeln!(out, "#[test]");
    let _ = writeln!(out, "fn test_{fn_name}() {{");
    let _ = writeln!(out, "    // {description}");

    // Check if any assertion is an error assertion.
    let has_error_assertion = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    // Emit input variable bindings from args config.
    let mut arg_exprs: Vec<String> = Vec::new();
    for arg in &e2e_config.call.args {
        let value = resolve_field(&fixture.input, &arg.field);
        let var_name = &arg.name;
        let (binding, expr) = render_rust_arg(var_name, value, &arg.arg_type, arg.optional);
        let _ = writeln!(out, "    {binding}");
        arg_exprs.push(expr);
    }

    let args_str = arg_exprs.join(", ");

    if has_error_assertion {
        let _ = writeln!(out, "    let {result_var} = {function_name}({args_str});");
        // Render error assertions.
        for assertion in &fixture.assertions {
            render_assertion(out, assertion, result_var, dep_name, true);
        }
        let _ = writeln!(out, "}}");
        return;
    }

    // Non-error path: unwrap the result.
    let has_not_error = fixture.assertions.iter().any(|a| a.assertion_type == "not_error");

    if has_not_error || !fixture.assertions.is_empty() {
        let _ = writeln!(
            out,
            "    let {result_var} = {function_name}({args_str}).expect(\"should succeed\");"
        );
    } else {
        let _ = writeln!(out, "    let {result_var} = {function_name}({args_str});");
    }

    // Render assertions.
    for assertion in &fixture.assertions {
        if assertion.assertion_type == "not_error" {
            // Already handled by .expect() above.
            continue;
        }
        render_assertion(out, assertion, result_var, dep_name, false);
    }

    let _ = writeln!(out, "}}");
}

// ---------------------------------------------------------------------------
// Argument rendering
// ---------------------------------------------------------------------------

fn resolve_field<'a>(input: &'a serde_json::Value, field_path: &str) -> &'a serde_json::Value {
    let mut current = input;
    for part in field_path.split('.') {
        current = current.get(part).unwrap_or(&serde_json::Value::Null);
    }
    current
}

fn render_rust_arg(name: &str, value: &serde_json::Value, arg_type: &str, optional: bool) -> (String, String) {
    let literal = json_to_rust_literal(value, arg_type);
    if optional && value.is_null() {
        (format!("let {name} = None;"), name.to_string())
    } else if optional {
        (format!("let {name} = Some({literal});"), name.to_string())
    } else {
        (format!("let {name} = {literal};"), name.to_string())
    }
}

fn json_to_rust_literal(value: &serde_json::Value, arg_type: &str) -> String {
    match value {
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(b) => format!("{b}"),
        serde_json::Value::Number(n) => {
            if arg_type.contains("float") || arg_type.contains("f64") || arg_type.contains("f32") {
                if let Some(f) = n.as_f64() {
                    return format!("{f}_f64");
                }
            }
            n.to_string()
        }
        serde_json::Value::String(s) => rust_raw_string(s),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            let literal = rust_raw_string(&json_str);
            format!("serde_json::from_str({literal}).unwrap()")
        }
    }
}

// ---------------------------------------------------------------------------
// Assertion rendering
// ---------------------------------------------------------------------------

fn render_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_var: &str,
    _dep_name: &str,
    is_error_context: bool,
) {
    let field_access = match &assertion.field {
        Some(f) if !f.is_empty() => format!("{result_var}.{f}"),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "error" => {
            let _ = writeln!(out, "    assert!({result_var}.is_err(), \"expected call to fail\");");
            if let Some(serde_json::Value::String(msg)) = &assertion.value {
                let escaped = escape_rust(msg);
                let _ = writeln!(
                    out,
                    "    assert!({result_var}.as_ref().unwrap_err().to_string().contains(\"{escaped}\"), \"error message mismatch\");"
                );
            }
        }
        "not_error" => {
            // Handled at call site; nothing extra needed here.
        }
        "equals" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_rust_string(val);
                if is_error_context {
                    return;
                }
                let _ = writeln!(
                    out,
                    "    assert_eq!({field_access}.trim(), {expected}, \"equals assertion failed\");"
                );
            }
        }
        "contains" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_rust_string(val);
                let _ = writeln!(
                    out,
                    "    assert!({field_access}.contains({expected}), \"expected to contain: {{}}\", {expected});"
                );
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let expected = value_to_rust_string(val);
                    let _ = writeln!(
                        out,
                        "    assert!({field_access}.contains({expected}), \"expected to contain: {{}}\", {expected});"
                    );
                }
            }
        }
        "not_contains" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_rust_string(val);
                let _ = writeln!(
                    out,
                    "    assert!(!{field_access}.contains({expected}), \"expected NOT to contain: {{}}\", {expected});"
                );
            }
        }
        "not_empty" => {
            let _ = writeln!(
                out,
                "    assert!(!{field_access}.is_empty(), \"expected non-empty value\");"
            );
        }
        "is_empty" => {
            let _ = writeln!(out, "    assert!({field_access}.is_empty(), \"expected empty value\");");
        }
        "starts_with" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_rust_string(val);
                let _ = writeln!(
                    out,
                    "    assert!({field_access}.starts_with({expected}), \"expected to start with: {{}}\", {expected});"
                );
            }
        }
        "ends_with" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_rust_string(val);
                let _ = writeln!(
                    out,
                    "    assert!({field_access}.ends_with({expected}), \"expected to end with: {{}}\", {expected});"
                );
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "    assert!({field_access}.len() >= {n}, \"expected length >= {n}, got {{}}\", {field_access}.len());"
                    );
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "    assert!({field_access}.len() <= {n}, \"expected length <= {n}, got {{}}\", {field_access}.len());"
                    );
                }
            }
        }
        other => {
            let _ = writeln!(out, "    // TODO: unsupported assertion type: {other}");
        }
    }
}

fn value_to_rust_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => rust_raw_string(s),
        other => {
            let s = other.to_string();
            format!("\"{s}\"")
        }
    }
}
