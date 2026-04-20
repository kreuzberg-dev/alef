//! Elixir e2e test generator using ExUnit.

use crate::config::E2eConfig;
use crate::escape::{escape_elixir, sanitize_filename, sanitize_ident};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::ToSnakeCase;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// Elixir e2e code generator.
pub struct ElixirCodegen;

impl E2eCodegen for ElixirCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        _alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(e2e_config.effective_output()).join(lang);

        let mut files = Vec::new();

        // Resolve call config with overrides.
        let call = &e2e_config.call;
        let overrides = call.overrides.get(lang);
        let raw_module = overrides
            .and_then(|o| o.module.as_ref())
            .cloned()
            .unwrap_or_else(|| call.module.clone());
        // Convert module path to Elixir PascalCase if it looks like snake_case
        // (e.g., "html_to_markdown" -> "HtmlToMarkdown").
        // If the override already contains "." (e.g., "Elixir.HtmlToMarkdown"), use as-is.
        let module_path = if raw_module.contains('.') || raw_module.chars().next().is_some_and(|c| c.is_uppercase()) {
            raw_module.clone()
        } else {
            elixir_module_name(&raw_module)
        };
        let base_function_name = overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call.function.clone());
        // Elixir facade exports async variants with `_async` suffix when the call is async.
        // Append the suffix only if not already present.
        let function_name = if call.r#async && !base_function_name.ends_with("_async") {
            format!("{base_function_name}_async")
        } else {
            base_function_name
        };
        let options_type = overrides.and_then(|o| o.options_type.clone());
        let options_default_fn = overrides.and_then(|o| o.options_via.clone());
        let empty_enum_fields = HashMap::new();
        let enum_fields = overrides.map(|o| &o.enum_fields).unwrap_or(&empty_enum_fields);
        let handle_struct_type = overrides.and_then(|o| o.handle_struct_type.clone());
        let empty_atom_fields = std::collections::HashSet::new();
        let handle_atom_list_fields = overrides
            .map(|o| &o.handle_atom_list_fields)
            .unwrap_or(&empty_atom_fields);
        let result_var = &call.result_var;

        // Resolve package config.
        let elixir_pkg = e2e_config.resolve_package("elixir");
        let pkg_path = elixir_pkg
            .as_ref()
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../packages/elixir".to_string());
        // The dep atom must be a valid snake_case Elixir atom (e.g., :html_to_markdown),
        // derived from the call module name, not the PascalCase module path.
        let dep_atom = elixir_pkg
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| raw_module.to_snake_case());
        let dep_version = elixir_pkg
            .as_ref()
            .and_then(|p| p.version.as_ref())
            .cloned()
            .unwrap_or_else(|| "0.1.0".to_string());

        // Generate mix.exs.
        files.push(GeneratedFile {
            path: output_base.join("mix.exs"),
            content: render_mix_exs(&dep_atom, &pkg_path, &dep_version, e2e_config.dep_mode),
            generated_header: false,
        });

        // Generate lib/e2e_elixir.ex — required so the mix project compiles.
        files.push(GeneratedFile {
            path: output_base.join("lib").join("e2e_elixir.ex"),
            content: "defmodule E2eElixir do\n  @moduledoc false\nend\n".to_string(),
            generated_header: false,
        });

        // Generate test_helper.exs.
        files.push(GeneratedFile {
            path: output_base.join("test").join("test_helper.exs"),
            content: "ExUnit.start()\n".to_string(),
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

            let filename = format!("{}_test.exs", sanitize_filename(&group.category));
            let field_resolver = FieldResolver::new(
                &e2e_config.fields,
                &e2e_config.fields_optional,
                &e2e_config.result_fields,
                &e2e_config.fields_array,
            );
            let content = render_test_file(
                &group.category,
                &active,
                &module_path,
                &function_name,
                result_var,
                &e2e_config.call.args,
                &field_resolver,
                options_type.as_deref(),
                options_default_fn.as_deref(),
                enum_fields,
                handle_struct_type.as_deref(),
                handle_atom_list_fields,
            );
            files.push(GeneratedFile {
                path: output_base.join("test").join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "elixir"
    }
}

fn render_mix_exs(
    dep_atom: &str,
    pkg_path: &str,
    dep_version: &str,
    dep_mode: crate::config::DependencyMode,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "defmodule E2eElixir.MixProject do");
    let _ = writeln!(out, "  use Mix.Project");
    let _ = writeln!(out);
    let _ = writeln!(out, "  def project do");
    let _ = writeln!(out, "    [");
    let _ = writeln!(out, "      app: :e2e_elixir,");
    let _ = writeln!(out, "      version: \"0.1.0\",");
    let _ = writeln!(out, "      elixir: \"~> 1.14\",");
    let _ = writeln!(out, "      deps: deps()");
    let _ = writeln!(out, "    ]");
    let _ = writeln!(out, "  end");
    let _ = writeln!(out);
    let _ = writeln!(out, "  defp deps do");
    let _ = writeln!(out, "    [");
    // Use a bare atom for the dep name (e.g., :html_to_markdown), not a quoted atom.
    let dep_line = match dep_mode {
        crate::config::DependencyMode::Registry => {
            format!("      {{:{dep_atom}, \"{dep_version}\"}}")
        }
        crate::config::DependencyMode::Local => {
            format!("      {{:{dep_atom}, path: \"{pkg_path}\"}}")
        }
    };
    let _ = writeln!(out, "{dep_line}");
    let _ = writeln!(out, "    ]");
    let _ = writeln!(out, "  end");
    let _ = writeln!(out, "end");
    out
}

#[allow(clippy::too_many_arguments)]
fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    module_path: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    options_type: Option<&str>,
    options_default_fn: Option<&str>,
    enum_fields: &HashMap<String, String>,
    handle_struct_type: Option<&str>,
    handle_atom_list_fields: &std::collections::HashSet<String>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# This file is auto-generated by alef. DO NOT EDIT.");
    let _ = writeln!(out, "# E2e tests for category: {category}");
    let _ = writeln!(out, "defmodule E2e.{}Test do", elixir_module_name(category));
    let _ = writeln!(out, "  use ExUnit.Case, async: true");
    let _ = writeln!(out);

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_case(
            &mut out,
            fixture,
            module_path,
            function_name,
            result_var,
            args,
            field_resolver,
            options_type,
            options_default_fn,
            enum_fields,
            handle_struct_type,
            handle_atom_list_fields,
        );
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "end");
    out
}

#[allow(clippy::too_many_arguments)]
fn render_test_case(
    out: &mut String,
    fixture: &Fixture,
    module_path: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    options_type: Option<&str>,
    options_default_fn: Option<&str>,
    enum_fields: &HashMap<String, String>,
    handle_struct_type: Option<&str>,
    handle_atom_list_fields: &std::collections::HashSet<String>,
) {
    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('"', "\\\"");

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let (setup_lines, args_str) = build_args_and_setup(
        &fixture.input,
        args,
        module_path,
        options_type,
        options_default_fn,
        enum_fields,
        &fixture.id,
        handle_struct_type,
        handle_atom_list_fields,
    );

    let _ = writeln!(out, "  describe \"{test_name}\" do");
    let _ = writeln!(out, "    test \"{description}\" do");

    for line in &setup_lines {
        let _ = writeln!(out, "      {line}");
    }

    if expects_error {
        let _ = writeln!(
            out,
            "      assert {{:error, _}} = {module_path}.{function_name}({args_str})"
        );
        let _ = writeln!(out, "    end");
        let _ = writeln!(out, "  end");
        return;
    }

    let _ = writeln!(
        out,
        "      {{:ok, {result_var}}} = {module_path}.{function_name}({args_str})"
    );

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "    end");
    let _ = writeln!(out, "  end");
}

/// Build setup lines (e.g. handle creation) and the argument list for the function call.
///
/// Returns `(setup_lines, args_string)`.
#[allow(clippy::too_many_arguments)]
fn build_args_and_setup(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    module_path: &str,
    options_type: Option<&str>,
    options_default_fn: Option<&str>,
    enum_fields: &HashMap<String, String>,
    fixture_id: &str,
    _handle_struct_type: Option<&str>,
    _handle_atom_list_fields: &std::collections::HashSet<String>,
) -> (Vec<String>, String) {
    if args.is_empty() {
        return (Vec::new(), json_to_elixir(input));
    }

    let mut setup_lines: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    for arg in args {
        if arg.arg_type == "mock_url" {
            setup_lines.push(format!(
                "{} = System.get_env(\"MOCK_SERVER_URL\") <> \"/fixtures/{fixture_id}\"",
                arg.name,
            ));
            parts.push(arg.name.clone());
            continue;
        }

        if arg.arg_type == "handle" {
            // Generate a create_{name} call using {:ok, name} = ... pattern.
            // The NIF now accepts config as an optional JSON string (not a NifStruct/NifMap)
            // so that partial maps work: serde_json::from_str respects #[serde(default)].
            let constructor_name = format!("create_{}", arg.name.to_snake_case());
            let config_value = input.get(&arg.field).unwrap_or(&serde_json::Value::Null);
            let name = &arg.name;
            if config_value.is_null()
                || config_value.is_object() && config_value.as_object().is_some_and(|o| o.is_empty())
            {
                setup_lines.push(format!("{{:ok, {name}}} = {module_path}.{constructor_name}(nil)"));
            } else {
                // Serialize the config map to a JSON string with Jason so that Rust can
                // deserialize it with serde_json and apply field defaults for missing keys.
                let json_str = serde_json::to_string(config_value).unwrap_or_else(|_| "{}".to_string());
                let escaped = escape_elixir(&json_str);
                setup_lines.push(format!("{name}_config = \"{escaped}\""));
                setup_lines.push(format!(
                    "{{:ok, {name}}} = {module_path}.{constructor_name}({name}_config)",
                ));
            }
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
                // For json_object args with options_type, build a proper struct.
                if arg.arg_type == "json_object" && !v.is_null() {
                    if let (Some(_opts_type), Some(options_fn), Some(obj)) =
                        (options_type, options_default_fn, v.as_object())
                    {
                        // Add setup line to initialize options from default function.
                        let options_var = "options";
                        setup_lines.push(format!("{options_var} = {module_path}.{options_fn}()"));

                        // For each field in the options object, add a struct update line.
                        for (k, vv) in obj.iter() {
                            let snake_key = k.to_snake_case();
                            let elixir_val = if let Some(_enum_type) = enum_fields.get(k) {
                                if let Some(s) = vv.as_str() {
                                    let snake_val = s.to_snake_case();
                                    // Use atom for enum values, not string
                                    format!(":{snake_val}")
                                } else {
                                    json_to_elixir(vv)
                                }
                            } else {
                                json_to_elixir(vv)
                            };
                            setup_lines.push(format!(
                                "{options_var} = %{{{options_var} | {snake_key}: {elixir_val}}}"
                            ));
                        }

                        // Push the variable name as the argument.
                        parts.push(options_var.to_string());
                        continue;
                    }
                }
                parts.push(json_to_elixir(v));
            }
        }
    }

    (setup_lines, parts.join(", "))
}

/// Returns true if the field expression is a numeric/integer expression
/// (e.g., a `length(...)` call) rather than a string.
fn is_numeric_expr(field_expr: &str) -> bool {
    field_expr.starts_with("length(")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    // Skip assertions on fields that don't exist on the result type.
    if let Some(f) = &assertion.field {
        if !f.is_empty() && !field_resolver.is_valid_for_result(f) {
            let _ = writeln!(out, "      # skipped: field '{f}' not available on result type");
            return;
        }
    }

    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "elixir", result_var),
        _ => result_var.to_string(),
    };

    // Only wrap in String.trim/0 when the expression is actually a string.
    // Numeric expressions (e.g., length(...)) must not be wrapped.
    let is_numeric = is_numeric_expr(&field_expr);
    let trimmed_field_expr = if is_numeric {
        field_expr.clone()
    } else {
        format!("String.trim({field_expr})")
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                // Apply String.trim only for string comparisons, not numeric ones.
                let is_string_expected = expected.is_string();
                if is_string_expected && !is_numeric {
                    let _ = writeln!(out, "      assert {trimmed_field_expr} == {elixir_val}");
                } else {
                    let _ = writeln!(out, "      assert {field_expr} == {elixir_val}");
                }
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                // Use to_string() to handle atoms (enums) as well as strings
                let _ = writeln!(
                    out,
                    "      assert String.contains?(to_string({field_expr}), {elixir_val})"
                );
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let elixir_val = json_to_elixir(val);
                    let _ = writeln!(
                        out,
                        "      assert String.contains?(to_string({field_expr}), {elixir_val})"
                    );
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(
                    out,
                    "      refute String.contains?(to_string({field_expr}), {elixir_val})"
                );
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "      assert {field_expr} != \"\"");
        }
        "is_empty" => {
            if is_numeric {
                // length(...) == 0
                let _ = writeln!(out, "      assert {field_expr} == 0");
            } else {
                // Handle nil (None) as empty
                let _ = writeln!(out, "      assert is_nil({field_expr}) or {trimmed_field_expr} == \"\"");
            }
        }
        "contains_any" => {
            if let Some(values) = &assertion.values {
                let items: Vec<String> = values.iter().map(json_to_elixir).collect();
                let list_str = items.join(", ");
                let _ = writeln!(
                    out,
                    "      assert Enum.any?([{list_str}], fn v -> String.contains?(to_string({field_expr}), v) end)"
                );
            }
        }
        "greater_than" => {
            if let Some(val) = &assertion.value {
                let elixir_val = json_to_elixir(val);
                let _ = writeln!(out, "      assert {field_expr} > {elixir_val}");
            }
        }
        "less_than" => {
            if let Some(val) = &assertion.value {
                let elixir_val = json_to_elixir(val);
                let _ = writeln!(out, "      assert {field_expr} < {elixir_val}");
            }
        }
        "greater_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let elixir_val = json_to_elixir(val);
                let _ = writeln!(out, "      assert {field_expr} >= {elixir_val}");
            }
        }
        "less_than_or_equal" => {
            if let Some(val) = &assertion.value {
                let elixir_val = json_to_elixir(val);
                let _ = writeln!(out, "      assert {field_expr} <= {elixir_val}");
            }
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(out, "      assert String.starts_with?({field_expr}, {elixir_val})");
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(out, "      assert String.ends_with?({field_expr}, {elixir_val})");
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "      assert String.length({field_expr}) >= {n}");
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "      assert String.length({field_expr}) <= {n}");
                }
            }
        }
        "count_min" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "      assert length({field_expr}) >= {n}");
                }
            }
        }
        "count_equals" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "      assert length({field_expr}) == {n}");
                }
            }
        }
        "is_true" => {
            let _ = writeln!(out, "      assert {field_expr} == true");
        }
        "not_error" => {
            // Already handled — the call would fail if it returned {:error, _}.
        }
        "error" => {
            // Handled at the test level.
        }
        other => {
            let _ = writeln!(out, "      # TODO: unsupported assertion type: {other}");
        }
    }
}

/// Convert a category name to an Elixir module-safe PascalCase name.
fn elixir_module_name(category: &str) -> String {
    use heck::ToUpperCamelCase;
    category.to_upper_camel_case()
}

/// Convert a `serde_json::Value` to an Elixir literal string.
fn json_to_elixir(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_elixir(s)),
        serde_json::Value::Bool(true) => "true".to_string(),
        serde_json::Value::Bool(false) => "false".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "nil".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_elixir).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\" => {}", k.to_snake_case(), json_to_elixir(v)))
                .collect();
            format!("%{{{}}}", entries.join(", "))
        }
    }
}
