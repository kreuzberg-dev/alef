//! Python e2e test code generator.
//!
//! Generates `e2e/python/conftest.py` and `tests/test_{category}.py` files from
//! JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

use crate::config::E2eConfig;
use crate::escape::{escape_python, sanitize_filename, sanitize_ident};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::{ToPascalCase, ToSnakeCase};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

/// Python e2e test code generator.
pub struct PythonE2eCodegen;

impl super::E2eCodegen for PythonE2eCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        _alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();
        let output_base = PathBuf::from(&e2e_config.output).join("python");

        // conftest.py
        files.push(GeneratedFile {
            path: output_base.join("conftest.py"),
            content: render_conftest(e2e_config),
            generated_header: true,
        });

        // tests/__init__.py
        files.push(GeneratedFile {
            path: output_base.join("tests").join("__init__.py"),
            content: String::new(),
            generated_header: false,
        });

        // Per-category test files.
        for group in groups {
            let fixtures: Vec<&Fixture> = group.fixtures.iter().filter(|f| !is_skipped(f, "python")).collect();

            if fixtures.is_empty() {
                continue;
            }

            let filename = format!("test_{}.py", sanitize_filename(&group.category));
            let content = render_test_file(&group.category, &fixtures, e2e_config);

            files.push(GeneratedFile {
                path: output_base.join("tests").join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "python"
    }
}

// ---------------------------------------------------------------------------
// Config resolution helpers
// ---------------------------------------------------------------------------

fn resolve_function_name(e2e_config: &E2eConfig) -> String {
    e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.function.clone())
        .unwrap_or_else(|| e2e_config.call.function.clone())
}

fn resolve_module(e2e_config: &E2eConfig) -> String {
    e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.module.clone())
        .unwrap_or_else(|| e2e_config.call.module.replace('-', "_"))
}

fn resolve_options_type(e2e_config: &E2eConfig) -> Option<String> {
    e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.options_type.clone())
}

/// Resolve how json_object args are passed: "kwargs" (default), "dict", or "json".
fn resolve_options_via(e2e_config: &E2eConfig) -> &str {
    e2e_config
        .call
        .overrides
        .get("python")
        .and_then(|o| o.options_via.as_deref())
        .unwrap_or("kwargs")
}

/// Resolve enum field mappings from the Python override config.
fn resolve_enum_fields(e2e_config: &E2eConfig) -> &HashMap<String, String> {
    static EMPTY: std::sync::LazyLock<HashMap<String, String>> = std::sync::LazyLock::new(HashMap::new);
    e2e_config
        .call
        .overrides
        .get("python")
        .map(|o| &o.enum_fields)
        .unwrap_or(&EMPTY)
}

fn is_skipped(fixture: &Fixture, language: &str) -> bool {
    fixture.skip.as_ref().is_some_and(|s| s.should_skip(language))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_conftest(e2e_config: &E2eConfig) -> String {
    let module = resolve_module(e2e_config);
    format!(
        r#""""Pytest configuration for e2e tests."""
# Ensure the package is importable.
# The {module} package is expected to be installed in the current environment.
"#
    )
}

fn render_test_file(category: &str, fixtures: &[&Fixture], e2e_config: &E2eConfig) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "\"\"\"E2e tests for category: {category}.");
    let _ = writeln!(out, "\"\"\"");
    let _ = writeln!(out, "# ruff: noqa: S101");

    let module = resolve_module(e2e_config);
    let function_name = resolve_function_name(e2e_config);
    let options_type = resolve_options_type(e2e_config);
    let options_via = resolve_options_via(e2e_config);
    let enum_fields = resolve_enum_fields(e2e_config);
    let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);

    let has_error_test = fixtures
        .iter()
        .any(|f| f.assertions.iter().any(|a| a.assertion_type == "error"));

    if has_error_test {
        let _ = writeln!(out, "import pytest");
    }

    // "json" mode needs `import json`.
    let needs_json_import = options_via == "json"
        && fixtures.iter().any(|f| {
            e2e_config
                .call
                .args
                .iter()
                .any(|arg| arg.arg_type == "json_object" && !resolve_field(&f.input, &arg.field).is_null())
        });

    if needs_json_import {
        let _ = writeln!(out, "import json");
    }

    // Only import options_type when using "kwargs" mode.
    let needs_options_type = options_via == "kwargs"
        && options_type.is_some()
        && fixtures.iter().any(|f| {
            e2e_config
                .call
                .args
                .iter()
                .any(|arg| arg.arg_type == "json_object" && !resolve_field(&f.input, &arg.field).is_null())
        });

    // Collect enum types actually used across all fixtures in this file.
    let mut used_enum_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if needs_options_type && !enum_fields.is_empty() {
        for fixture in fixtures.iter() {
            for arg in &e2e_config.call.args {
                if arg.arg_type == "json_object" {
                    let value = resolve_field(&fixture.input, &arg.field);
                    if let Some(obj) = value.as_object() {
                        for key in obj.keys() {
                            if let Some(enum_type) = enum_fields.get(key) {
                                used_enum_types.insert(enum_type.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    if let (true, Some(opts_type)) = (needs_options_type, &options_type) {
        let _ = writeln!(out, "from {module} import {function_name}, {opts_type}");
        // Import enum types from enum_module (if specified) or main module.
        if !used_enum_types.is_empty() {
            let enum_mod = e2e_config
                .call
                .overrides
                .get("python")
                .and_then(|o| o.enum_module.as_deref())
                .unwrap_or(&module);
            let enum_names: Vec<&String> = used_enum_types.iter().collect();
            let _ = writeln!(
                out,
                "from {enum_mod} import {}",
                enum_names.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            );
        }
    } else {
        let _ = writeln!(out, "from {module} import {function_name}");
    }
    let _ = writeln!(out);

    for fixture in fixtures {
        render_test_function(
            &mut out,
            fixture,
            e2e_config,
            options_type.as_deref(),
            options_via,
            enum_fields,
            &field_resolver,
        );
        let _ = writeln!(out);
    }

    out
}

fn render_test_function(
    out: &mut String,
    fixture: &Fixture,
    e2e_config: &E2eConfig,
    options_type: Option<&str>,
    options_via: &str,
    enum_fields: &HashMap<String, String>,
    field_resolver: &FieldResolver,
) {
    let fn_name = sanitize_ident(&fixture.id);
    let description = &fixture.description;
    let function_name = resolve_function_name(e2e_config);
    let result_var = &e2e_config.call.result_var;

    let desc_with_period = if description.ends_with('.') {
        description.to_string()
    } else {
        format!("{description}.")
    };

    let _ = writeln!(out, "def test_{fn_name}() -> None:");
    let _ = writeln!(out, "    \"\"\"{desc_with_period}\"\"\"");

    // Check if any assertion is an error assertion.
    let has_error_assertion = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    // Build argument expressions from config.
    let mut arg_bindings = Vec::new();
    let mut kwarg_exprs = Vec::new();
    for arg in &e2e_config.call.args {
        let value = resolve_field(&fixture.input, &arg.field);
        let var_name = &arg.name;

        if value.is_null() && arg.optional {
            continue;
        }

        // For json_object args, use the configured options_via strategy.
        if arg.arg_type == "json_object" && !value.is_null() {
            match options_via {
                "dict" => {
                    // Pass as a plain Python dict literal.
                    let literal = json_to_python_literal(value);
                    arg_bindings.push(format!("    {var_name} = {literal}"));
                    kwarg_exprs.push(format!("{var_name}={var_name}"));
                    continue;
                }
                "json" => {
                    // Pass via json.loads() with the raw JSON string.
                    let json_str = serde_json::to_string(value).unwrap_or_default();
                    let escaped = escape_python(&json_str);
                    arg_bindings.push(format!("    {var_name} = json.loads(\"{escaped}\")"));
                    kwarg_exprs.push(format!("{var_name}={var_name}"));
                    continue;
                }
                _ => {
                    // "kwargs" (default): construct OptionsType(key=val, ...).
                    if let (Some(opts_type), Some(obj)) = (options_type, value.as_object()) {
                        let kwargs: Vec<String> = obj
                            .iter()
                            .map(|(k, v)| {
                                let snake_key = k.to_snake_case();
                                let py_val = if let Some(enum_type) = enum_fields.get(k) {
                                    // Map string value to enum constant.
                                    if let Some(s) = v.as_str() {
                                        let pascal_val = s.to_pascal_case();
                                        format!("{enum_type}.{pascal_val}")
                                    } else {
                                        json_to_python_literal(v)
                                    }
                                } else {
                                    json_to_python_literal(v)
                                };
                                format!("{snake_key}={py_val}")
                            })
                            .collect();
                        let constructor = format!("{opts_type}({})", kwargs.join(", "));
                        arg_bindings.push(format!("    {var_name} = {constructor}"));
                        kwarg_exprs.push(format!("{var_name}={var_name}"));
                        continue;
                    }
                }
            }
        }

        let literal = json_to_python_literal(value);
        arg_bindings.push(format!("    {var_name} = {literal}"));
        kwarg_exprs.push(format!("{var_name}={var_name}"));
    }

    for binding in &arg_bindings {
        let _ = writeln!(out, "{binding}");
    }

    let call_args = kwarg_exprs.join(", ");
    let call_expr = format!("{function_name}({call_args})");

    if has_error_assertion {
        // Find error assertion for optional message check.
        let error_assertion = fixture.assertions.iter().find(|a| a.assertion_type == "error");
        let has_message = error_assertion
            .and_then(|a| a.value.as_ref())
            .and_then(|v| v.as_str())
            .is_some();

        if has_message {
            let _ = writeln!(out, "    with pytest.raises(Exception) as exc_info:");
            let _ = writeln!(out, "        {call_expr}");
            if let Some(msg) = error_assertion.and_then(|a| a.value.as_ref()).and_then(|v| v.as_str()) {
                let escaped = escape_python(msg);
                let _ = writeln!(out, "    assert \"{escaped}\" in str(exc_info.value)");
            }
        } else {
            let _ = writeln!(out, "    with pytest.raises(Exception):");
            let _ = writeln!(out, "        {call_expr}");
        }

        // Render any non-error assertions (unlikely but handle gracefully).
        for assertion in &fixture.assertions {
            if assertion.assertion_type != "error" {
                render_assertion(out, assertion, result_var, field_resolver);
            }
        }
        return;
    }

    // Non-error path.
    let _ = writeln!(out, "    {result_var} = {call_expr}");

    for assertion in &fixture.assertions {
        if assertion.assertion_type == "not_error" {
            // The call already raises on error in Python.
            continue;
        }
        render_assertion(out, assertion, result_var, field_resolver);
    }
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

fn json_to_python_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("\"{}\"", escape_python(s)),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_python_literal).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", escape_python(k), json_to_python_literal(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// Assertion rendering
// ---------------------------------------------------------------------------

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_access = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "python", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "error" | "not_error" => {
            // Handled at call site.
        }
        "equals" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {field_access}.strip() == {expected}");
            }
        }
        "contains" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {expected} in {field_access}");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let expected = value_to_python_string(val);
                    let _ = writeln!(out, "    assert {expected} in {field_access}");
                }
            }
        }
        "not_contains" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {expected} not in {field_access}");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "    assert {field_access}");
        }
        "is_empty" => {
            let _ = writeln!(out, "    assert not {field_access}");
        }
        "starts_with" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {field_access}.startswith({expected})");
            }
        }
        "ends_with" => {
            if let Some(val) = &assertion.value {
                let expected = value_to_python_string(val);
                let _ = writeln!(out, "    assert {field_access}.endswith({expected})");
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    assert len({field_access}) >= {n}");
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    assert len({field_access}) <= {n}");
                }
            }
        }
        other => {
            let _ = writeln!(out, "    # TODO: unsupported assertion type: {other}");
        }
    }
}

fn value_to_python_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_python(s)),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "None".to_string(),
        other => format!("\"{}\"", escape_python(&other.to_string())),
    }
}
