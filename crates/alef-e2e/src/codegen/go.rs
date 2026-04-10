//! Go e2e test generator using testing.T.

use crate::config::E2eConfig;
use crate::escape::{go_string_literal, sanitize_filename};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::ToUpperCamelCase;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// Go e2e code generator.
pub struct GoCodegen;

impl E2eCodegen for GoCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        _alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(&e2e_config.output).join(lang);

        let mut files = Vec::new();

        // Resolve call config with overrides.
        let call = &e2e_config.call;
        let overrides = call.overrides.get(lang);
        let module_path = overrides
            .and_then(|o| o.module.as_ref())
            .cloned()
            .unwrap_or_else(|| call.module.clone());
        let function_name = overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call.function.clone());
        let import_alias = overrides
            .and_then(|o| o.alias.as_ref())
            .cloned()
            .unwrap_or_else(|| "pkg".to_string());
        let result_var = &call.result_var;

        // Resolve package config.
        let go_pkg = e2e_config.packages.get("go");
        let go_module_path = go_pkg
            .and_then(|p| p.module.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());
        let replace_path = go_pkg.and_then(|p| p.path.as_ref()).cloned();
        let go_version = go_pkg
            .and_then(|p| p.version.as_ref())
            .cloned()
            .unwrap_or_else(|| "v0.0.0".to_string());
        let field_resolver = FieldResolver::new(
            &e2e_config.fields,
            &e2e_config.fields_optional,
            &e2e_config.result_fields,
        );

        // Generate go.mod.
        files.push(GeneratedFile {
            path: output_base.join("go.mod"),
            content: render_go_mod(&go_module_path, replace_path.as_deref(), &go_version),
            generated_header: false,
        });

        // Generate test files per category.
        for group in groups {
            let active: Vec<&Fixture> = group
                .fixtures
                .iter()
                .filter(|f| f.skip.as_ref().is_none_or(|s| !s.should_skip(lang)))
                .collect();

            if active.is_empty() {
                continue;
            }

            let filename = format!("{}_test.go", sanitize_filename(&group.category));
            let content = render_test_file(
                &group.category,
                &active,
                &module_path,
                &import_alias,
                &function_name,
                result_var,
                &e2e_config.call.args,
                &field_resolver,
                e2e_config,
            );
            files.push(GeneratedFile {
                path: output_base.join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "go"
    }
}

fn render_go_mod(go_module_path: &str, replace_path: Option<&str>, version: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "module e2e_go");
    let _ = writeln!(out);
    let _ = writeln!(out, "go 1.23");
    let _ = writeln!(out);
    let _ = writeln!(out, "require {go_module_path} {version}");

    if let Some(path) = replace_path {
        let _ = writeln!(out);
        let _ = writeln!(out, "replace {go_module_path} => {path}");
    }

    out
}

fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    go_module_path: &str,
    import_alias: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    e2e_config: &crate::config::E2eConfig,
) -> String {
    let mut out = String::new();

    // Determine if we need the "strings" import.
    let needs_strings = fixtures.iter().any(|f| {
        f.assertions.iter().any(|a| {
            matches!(
                a.assertion_type.as_str(),
                "equals" | "contains" | "contains_all" | "not_contains" | "starts_with"
            )
        })
    });

    let _ = writeln!(out, "// E2e tests for category: {category}");
    let _ = writeln!(out, "package e2e_test");
    let _ = writeln!(out);
    let _ = writeln!(out, "import (");
    if needs_strings {
        let _ = writeln!(out, "\t\"strings\"");
    }
    let _ = writeln!(out, "\t\"testing\"");
    let _ = writeln!(out);
    let _ = writeln!(out, "\t{import_alias} \"{go_module_path}\"");
    let _ = writeln!(out, ")");
    let _ = writeln!(out);

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_function(
            &mut out,
            fixture,
            import_alias,
            function_name,
            result_var,
            args,
            field_resolver,
            e2e_config,
        );
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    // Clean up trailing newlines.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_test_function(
    out: &mut String,
    fixture: &Fixture,
    import_alias: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    e2e_config: &crate::config::E2eConfig,
) {
    let fn_name = fixture.id.to_upper_camel_case();
    let description = &fixture.description;

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let (setup_lines, args_str) = build_args_and_setup(&fixture.input, args, import_alias, e2e_config);

    let _ = writeln!(out, "func Test_{fn_name}(t *testing.T) {{");
    let _ = writeln!(out, "\t// {description}");

    for line in &setup_lines {
        let _ = writeln!(out, "\t{line}");
    }

    if expects_error {
        let _ = writeln!(out, "\t_, err := {import_alias}.{function_name}({args_str})");
        let _ = writeln!(out, "\tif err == nil {{");
        let _ = writeln!(out, "\t\tt.Errorf(\"expected an error, but call succeeded\")");
        let _ = writeln!(out, "\t}}");
        let _ = writeln!(out, "}}");
        return;
    }

    // Normal call: check for error assertions first.
    let _ = writeln!(out, "\t{result_var}, err := {import_alias}.{function_name}({args_str})");
    let _ = writeln!(out, "\tif err != nil {{");
    let _ = writeln!(out, "\t\tt.Fatalf(\"call failed: %v\", err)");
    let _ = writeln!(out, "\t}}");

    // Collect optional fields referenced by assertions and emit nil-safe
    // dereference blocks so that assertions can use plain string locals.
    let mut optional_locals: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for assertion in &fixture.assertions {
        if let Some(f) = &assertion.field {
            if !f.is_empty() {
                let resolved = field_resolver.resolve(f);
                if field_resolver.is_optional(resolved) && !optional_locals.contains_key(f.as_str()) {
                    let field_expr = field_resolver.accessor(f, "go", result_var);
                    let local_var = resolved.replace(['.', '['], "_").replace(']', "");
                    if field_resolver.has_map_access(f) {
                        // Go map access returns a value type (string), not a pointer.
                        // Use the value directly — empty string means not present.
                        let _ = writeln!(out, "\t{local_var} := {field_expr}");
                    } else {
                        let _ = writeln!(out, "\tvar {local_var} string");
                        let _ = writeln!(out, "\tif {field_expr} != nil {{");
                        let _ = writeln!(out, "\t\t{local_var} = *{field_expr}");
                        let _ = writeln!(out, "\t}}");
                    }
                    optional_locals.insert(f.clone(), local_var);
                }
            }
        }
    }

    // Emit assertions, wrapping in nil guards when an intermediate path segment is optional.
    for assertion in &fixture.assertions {
        if let Some(f) = &assertion.field {
            if !f.is_empty() && !optional_locals.contains_key(f.as_str()) {
                // Check if any prefix of the dotted path is optional (pointer in Go).
                // e.g., "document.nodes" — if "document" is optional, guard the whole block.
                let parts: Vec<&str> = f.split('.').collect();
                let mut guard_expr: Option<String> = None;
                for i in 1..parts.len() {
                    let prefix = parts[..i].join(".");
                    let resolved_prefix = field_resolver.resolve(&prefix);
                    if field_resolver.is_optional(resolved_prefix) {
                        let accessor = field_resolver.accessor(&prefix, "go", result_var);
                        guard_expr = Some(accessor);
                        break;
                    }
                }
                if let Some(guard) = guard_expr {
                    let _ = writeln!(out, "\tif {guard} != nil {{");
                    render_assertion(out, assertion, result_var, field_resolver, &optional_locals);
                    let _ = writeln!(out, "\t}}");
                    continue;
                }
            }
        }
        render_assertion(out, assertion, result_var, field_resolver, &optional_locals);
    }

    let _ = writeln!(out, "}}");
}

/// Build setup lines (e.g. handle creation) and the argument list for the function call.
///
/// Returns `(setup_lines, args_string)`.
fn build_args_and_setup(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    import_alias: &str,
    e2e_config: &crate::config::E2eConfig,
) -> (Vec<String>, String) {
    use heck::ToUpperCamelCase;

    if args.is_empty() {
        return (Vec::new(), json_to_go(input));
    }

    let overrides = e2e_config.call.overrides.get("go");
    let options_type = overrides.and_then(|o| o.options_type.as_deref());

    let mut setup_lines: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    for arg in args {
        if arg.arg_type == "handle" {
            // Generate a CreateEngine (or equivalent) call and pass the variable.
            // Call with no args — optional params are variadic in Go.
            let constructor_name = format!("Create{}", arg.name.to_upper_camel_case());
            setup_lines.push(format!(
                "{name}, createErr := {import_alias}.{constructor_name}()\n\tif createErr != nil {{\n\t\tt.Fatalf(\"create handle failed: %v\", createErr)\n\t}}",
                name = arg.name,
            ));
            parts.push(arg.name.clone());
            continue;
        }

        let val = input.get(&arg.field);
        match val {
            None | Some(serde_json::Value::Null) if arg.optional => {
                // Optional arg with no fixture value: skip entirely.
                continue;
            }
            None | Some(serde_json::Value::Null) => {
                // Required arg with no fixture value: pass a language-appropriate default.
                let default_val = match arg.arg_type.as_str() {
                    "string" => "\"\"".to_string(),
                    "int" | "integer" => "0".to_string(),
                    "float" | "number" => "0.0".to_string(),
                    "bool" | "boolean" => "false".to_string(),
                    _ => "nil".to_string(),
                };
                parts.push(default_val);
            }
            Some(v) => {
                // For json_object args with options_type: construct using functional options.
                if arg.arg_type == "json_object" && options_type.is_some() {
                    if let Some(obj) = v.as_object() {
                        let with_calls: Vec<String> = obj
                            .iter()
                            .map(|(k, vv)| {
                                let func_name = format!("With{}{}", options_type.unwrap(), k.to_upper_camel_case());
                                let go_val = json_to_go(vv);
                                format!("htmd.{func_name}({go_val})")
                            })
                            .collect();
                        let new_fn = format!("New{}", options_type.unwrap());
                        parts.push(format!("htmd.{new_fn}({})", with_calls.join(", ")));
                        continue;
                    }
                }
                parts.push(json_to_go(v));
            }
        }
    }

    (setup_lines, parts.join(", "))
}

fn render_assertion(
    out: &mut String,
    assertion: &Assertion,
    result_var: &str,
    field_resolver: &FieldResolver,
    optional_locals: &std::collections::HashMap<String, String>,
) {
    // Skip assertions on fields that don't exist on the result type.
    if let Some(f) = &assertion.field {
        if !f.is_empty() && !field_resolver.is_valid_for_result(f) {
            let _ = writeln!(out, "\t// skipped: field '{f}' not available on result type");
            return;
        }
    }

    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => {
            // Use the local variable if the field was dereferenced above.
            if let Some(local_var) = optional_locals.get(f.as_str()) {
                local_var.clone()
            } else {
                field_resolver.accessor(f, "go", result_var)
            }
        }
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let go_val = json_to_go(expected);
                let _ = writeln!(out, "\tif {field_expr} != {go_val} {{");
                let _ = writeln!(out, "\t\tt.Errorf(\"equals mismatch: got %q\", {field_expr})");
                let _ = writeln!(out, "\t}}");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let go_val = json_to_go(expected);
                let _ = writeln!(out, "\tif !strings.Contains({field_expr}, {go_val}) {{");
                let _ = writeln!(
                    out,
                    "\t\tt.Errorf(\"expected to contain %s, got %q\", {go_val}, {field_expr})"
                );
                let _ = writeln!(out, "\t}}");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let go_val = json_to_go(val);
                    let _ = writeln!(out, "\tif !strings.Contains({field_expr}, {go_val}) {{");
                    let _ = writeln!(out, "\t\tt.Errorf(\"expected to contain %s\", {go_val})");
                    let _ = writeln!(out, "\t}}");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let go_val = json_to_go(expected);
                let _ = writeln!(out, "\tif strings.Contains({field_expr}, {go_val}) {{");
                let _ = writeln!(
                    out,
                    "\t\tt.Errorf(\"expected NOT to contain %s, got %q\", {go_val}, {field_expr})"
                );
                let _ = writeln!(out, "\t}}");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "\tif len({field_expr}) == 0 {{");
            let _ = writeln!(out, "\t\tt.Errorf(\"expected non-empty value\")");
            let _ = writeln!(out, "\t}}");
        }
        "is_empty" => {
            let _ = writeln!(out, "\tif len({field_expr}) != 0 {{");
            let _ = writeln!(out, "\t\tt.Errorf(\"expected empty value, got %q\", {field_expr})");
            let _ = writeln!(out, "\t}}");
        }
        "contains_any" => {
            if let Some(values) = &assertion.values {
                let _ = writeln!(out, "\t{{");
                let _ = writeln!(out, "\t\tfound := false");
                for val in values {
                    let go_val = json_to_go(val);
                    let _ = writeln!(
                        out,
                        "\t\tif strings.Contains({field_expr}, {go_val}) {{ found = true }}"
                    );
                }
                let _ = writeln!(out, "\t\tif !found {{");
                let _ = writeln!(
                    out,
                    "\t\t\tt.Errorf(\"expected to contain at least one of the specified values\")"
                );
                let _ = writeln!(out, "\t\t}}");
                let _ = writeln!(out, "\t}}");
            }
        }
        "greater_than" => {
            if let Some(val) = &assertion.value {
                let go_val = json_to_go(val);
                let _ = writeln!(out, "\tif {field_expr} <= {go_val} {{");
                let _ = writeln!(out, "\t\tt.Errorf(\"expected > {go_val}, got %v\", {field_expr})");
                let _ = writeln!(out, "\t}}");
            }
        }
        "less_than" => {
            if let Some(val) = &assertion.value {
                let go_val = json_to_go(val);
                let _ = writeln!(out, "\tif {field_expr} >= {go_val} {{");
                let _ = writeln!(out, "\t\tt.Errorf(\"expected < {go_val}, got %v\", {field_expr})");
                let _ = writeln!(out, "\t}}");
            }
        }
        "greater_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let go_val = json_to_go(val);
                let _ = writeln!(out, "\tif {field_expr} < {go_val} {{");
                let _ = writeln!(out, "\t\tt.Errorf(\"expected >= {go_val}, got %v\", {field_expr})");
                let _ = writeln!(out, "\t}}");
            }
        }
        "less_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let go_val = json_to_go(val);
                let _ = writeln!(out, "\tif {field_expr} > {go_val} {{");
                let _ = writeln!(out, "\t\tt.Errorf(\"expected <= {go_val}, got %v\", {field_expr})");
                let _ = writeln!(out, "\t}}");
            }
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let go_val = json_to_go(expected);
                let _ = writeln!(out, "\tif !strings.HasPrefix({field_expr}, {go_val}) {{");
                let _ = writeln!(
                    out,
                    "\t\tt.Errorf(\"expected to start with %s, got %q\", {go_val}, {field_expr})"
                );
                let _ = writeln!(out, "\t}}");
            }
        }
        "count_min" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "\tassert.GreaterOrEqual(t, len({field_expr}), {n}, \"expected at least {n} elements\")"
                    );
                }
            }
        }
        "not_error" => {
            // Already handled by the `if err != nil` check above.
        }
        "error" => {
            // Handled at the test function level.
        }
        other => {
            let _ = writeln!(out, "\t// TODO: unsupported assertion type: {other}");
        }
    }
}

/// Convert a `serde_json::Value` to a Go literal string.
fn json_to_go(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => go_string_literal(s),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "nil".to_string(),
        // For complex types, serialize to JSON string and pass as literal.
        other => go_string_literal(&other.to_string()),
    }
}
