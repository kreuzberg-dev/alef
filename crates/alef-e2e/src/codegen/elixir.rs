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
        let output_base = PathBuf::from(&e2e_config.output).join(lang);

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
            raw_module
        } else {
            elixir_module_name(&raw_module)
        };
        let function_name = overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call.function.clone());
        let options_type = overrides.and_then(|o| o.options_type.clone());
        let options_default_fn = overrides.and_then(|o| o.options_via.clone());
        let empty_enum_fields = HashMap::new();
        let enum_fields = overrides.map(|o| &o.enum_fields).unwrap_or(&empty_enum_fields);
        let result_var = &call.result_var;

        // Resolve package config.
        let elixir_pkg = e2e_config.packages.get("elixir");
        let pkg_path = elixir_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../packages/elixir".to_string());
        let pkg_name = elixir_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());

        // Generate mix.exs.
        files.push(GeneratedFile {
            path: output_base.join("mix.exs"),
            content: render_mix_exs(&pkg_name, &pkg_path),
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
            let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);
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

fn render_mix_exs(pkg_name: &str, pkg_path: &str) -> String {
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
    let dep_line = format!("      {{:\"{pkg_name}\", path: \"{pkg_path}\"}}");
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
) -> String {
    let mut out = String::new();
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
) {
    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('"', "\\\"");

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let args_str = build_args_string(
        &fixture.input,
        args,
        module_path,
        options_type,
        options_default_fn,
        enum_fields,
    );

    if expects_error {
        let _ = writeln!(out, "  describe \"{test_name}\" do");
        let _ = writeln!(out, "    test \"{description}\" do");
        let _ = writeln!(out, "      assert_raise RuntimeError, fn ->");
        let _ = writeln!(out, "        {module_path}.{function_name}({args_str})");
        let _ = writeln!(out, "      end");
        let _ = writeln!(out, "    end");
        let _ = writeln!(out, "  end");
        return;
    }

    let _ = writeln!(out, "  describe \"{test_name}\" do");
    let _ = writeln!(out, "    test \"{description}\" do");
    let _ = writeln!(out, "      {result_var} = {module_path}.{function_name}({args_str})");

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "    end");
    let _ = writeln!(out, "  end");
}

fn build_args_string(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    module_path: &str,
    options_type: Option<&str>,
    options_default_fn: Option<&str>,
    enum_fields: &HashMap<String, String>,
) -> String {
    if args.is_empty() {
        return json_to_elixir(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            // For json_object args with options_type, use struct update on default.
            if arg.arg_type == "json_object" && !val.is_null() {
                if let (Some(_opts_type), Some(obj)) = (options_type, val.as_object()) {
                    let default_fn = options_default_fn.unwrap_or("conversionoptions_default");
                    let fields: Vec<String> = obj
                        .iter()
                        .map(|(k, v)| {
                            let snake_key = k.to_snake_case();
                            let elixir_val = if let Some(_enum_type) = enum_fields.get(k) {
                                // Map string value to Elixir atom.
                                if let Some(s) = v.as_str() {
                                    let snake_val = s.to_snake_case();
                                    format!(":{snake_val}")
                                } else {
                                    json_to_elixir(v)
                                }
                            } else {
                                json_to_elixir(v)
                            };
                            format!("{snake_key}: {elixir_val}")
                        })
                        .collect();
                    return Some(format!("%{{{module_path}.{default_fn}() | {}}}", fields.join(", ")));
                }
            }
            Some(json_to_elixir(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "elixir", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(out, "      assert String.trim({field_expr}) == {elixir_val}");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(out, "      assert String.contains?({field_expr}, {elixir_val})");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let elixir_val = json_to_elixir(val);
                    let _ = writeln!(out, "      assert String.contains?({field_expr}, {elixir_val})");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let elixir_val = json_to_elixir(expected);
                let _ = writeln!(out, "      refute String.contains?({field_expr}, {elixir_val})");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "      assert {field_expr} != \"\"");
        }
        "is_empty" => {
            let _ = writeln!(out, "      assert {field_expr} == \"\"");
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
        "not_error" => {
            // Already handled — the call would raise if it failed.
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
                .map(|(k, v)| format!("{}: {}", k, json_to_elixir(v)))
                .collect();
            format!("%{{{}}}", entries.join(", "))
        }
    }
}
