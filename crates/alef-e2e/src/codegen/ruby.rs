//! Ruby e2e test generator using RSpec.
//!
//! Generates `e2e/ruby/Gemfile` and `spec/{category}_spec.rb` files from
//! JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

use crate::config::E2eConfig;
use crate::escape::{escape_ruby, sanitize_filename, sanitize_ident};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// Ruby e2e code generator.
pub struct RubyCodegen;

impl E2eCodegen for RubyCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        alef_config: &AlefConfig,
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
        let class_name = overrides.and_then(|o| o.class.as_ref()).cloned();
        let result_var = &call.result_var;

        // Resolve package config.
        let ruby_pkg = e2e_config.packages.get("ruby");
        let gem_name = ruby_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| alef_config.crate_config.name.replace('-', "_"));
        let gem_path = ruby_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../packages/ruby".to_string());

        // Generate Gemfile.
        files.push(GeneratedFile {
            path: output_base.join("Gemfile"),
            content: render_gemfile(&gem_name, &gem_path),
            generated_header: false,
        });

        // Generate spec files per category.
        let spec_base = output_base.join("spec");

        for group in groups {
            let active: Vec<&Fixture> = group
                .fixtures
                .iter()
                .filter(|f| f.skip.as_ref().is_none_or(|s| !s.should_skip(lang)))
                .collect();

            if active.is_empty() {
                continue;
            }

            let filename = format!("{}_spec.rb", sanitize_filename(&group.category));
            let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);
            let content = render_spec_file(
                &group.category,
                &active,
                &module_path,
                &function_name,
                class_name.as_deref(),
                result_var,
                &gem_name,
                &e2e_config.call.args,
                &field_resolver,
            );
            files.push(GeneratedFile {
                path: spec_base.join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "ruby"
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_gemfile(gem_name: &str, gem_path: &str) -> String {
    format!(
        r#"# frozen_string_literal: true

source "https://rubygems.org"

gem "{gem_name}", path: "{gem_path}"
gem "rspec", "~> 3.13"
"#
    )
}

#[allow(clippy::too_many_arguments)]
fn render_spec_file(
    category: &str,
    fixtures: &[&Fixture],
    module_path: &str,
    function_name: &str,
    class_name: Option<&str>,
    result_var: &str,
    gem_name: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# frozen_string_literal: true");
    let _ = writeln!(out);

    // Require the gem.
    let require_name = if module_path.is_empty() { gem_name } else { module_path };
    let _ = writeln!(out, "require \"{}\"", require_name.replace('-', "_"));
    let _ = writeln!(out);

    let _ = writeln!(out, "RSpec.describe \"{category}\" do");

    for (i, fixture) in fixtures.iter().enumerate() {
        render_example(&mut out, fixture, function_name, class_name, result_var, args, field_resolver);
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "end");
    out
}

fn render_example(
    out: &mut String,
    fixture: &Fixture,
    function_name: &str,
    class_name: Option<&str>,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) {
    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('"', "\\\"");
    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let args_str = build_args_string(&fixture.input, args);

    let call_expr = match class_name {
        Some(cls) => format!("{cls}.{function_name}({args_str})"),
        None => format!("{function_name}({args_str})"),
    };

    let _ = writeln!(out, "  it \"{test_name}: {description}\" do");

    if expects_error {
        let _ = writeln!(out, "    expect {{ {call_expr} }}.to raise_error");
        let _ = writeln!(out, "  end");
        return;
    }

    let _ = writeln!(out, "    {result_var} = {call_expr}");

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "  end");
}

fn build_args_string(input: &serde_json::Value, args: &[crate::config::ArgMapping]) -> String {
    if args.is_empty() {
        return json_to_ruby(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            Some(json_to_ruby(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "ruby", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let rb_val = json_to_ruby(expected);
                let _ = writeln!(out, "    expect({field_expr}.strip).to eq({rb_val})");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let rb_val = json_to_ruby(expected);
                let _ = writeln!(out, "    expect({field_expr}).to include({rb_val})");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let rb_val = json_to_ruby(val);
                    let _ = writeln!(out, "    expect({field_expr}).to include({rb_val})");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let rb_val = json_to_ruby(expected);
                let _ = writeln!(out, "    expect({field_expr}).not_to include({rb_val})");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "    expect({field_expr}).not_to be_empty");
        }
        "is_empty" => {
            let _ = writeln!(out, "    expect({field_expr}).to be_empty");
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let rb_val = json_to_ruby(expected);
                let _ = writeln!(out, "    expect({field_expr}).to start_with({rb_val})");
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let rb_val = json_to_ruby(expected);
                let _ = writeln!(out, "    expect({field_expr}).to end_with({rb_val})");
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    expect({field_expr}.length).to be >= {n}");
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(out, "    expect({field_expr}.length).to be <= {n}");
                }
            }
        }
        "not_error" => {
            // Already handled by the call succeeding without exception.
        }
        "error" => {
            // Handled at the example level.
        }
        other => {
            let _ = writeln!(out, "    # TODO: unsupported assertion type: {other}");
        }
    }
}

/// Convert a `serde_json::Value` to a Ruby literal string.
fn json_to_ruby(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_ruby(s)),
        serde_json::Value::Bool(true) => "true".to_string(),
        serde_json::Value::Bool(false) => "false".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "nil".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_ruby).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\" => {}", escape_ruby(k), json_to_ruby(v)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
    }
}
