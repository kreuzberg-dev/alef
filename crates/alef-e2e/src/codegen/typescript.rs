//! TypeScript e2e test generator using vitest.

use crate::config::E2eConfig;
use crate::escape::{escape_js, sanitize_filename, sanitize_ident};
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// TypeScript e2e code generator.
pub struct TypeScriptCodegen;

impl E2eCodegen for TypeScriptCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        _alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(&e2e_config.output).join(lang);
        let tests_base = output_base.join("tests");

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
        let result_var = &call.result_var;
        let is_async = call.r#async;

        // Resolve package config.
        let node_pkg = e2e_config.packages.get("node");
        let pkg_path = node_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../packages/typescript".to_string());
        let pkg_name = node_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());

        // Generate package.json.
        files.push(GeneratedFile {
            path: output_base.join("package.json"),
            content: render_package_json(&pkg_name, &pkg_path),
            generated_header: false,
        });

        // Generate tsconfig.json.
        files.push(GeneratedFile {
            path: output_base.join("tsconfig.json"),
            content: render_tsconfig(),
            generated_header: false,
        });

        // Generate vitest.config.ts.
        files.push(GeneratedFile {
            path: output_base.join("vitest.config.ts"),
            content: render_vitest_config(),
            generated_header: true,
        });

        // Resolve options_type from override.
        let options_type = overrides.and_then(|o| o.options_type.clone());

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

            let filename = format!("{}.test.ts", sanitize_filename(&group.category));
            let content = render_test_file(
                &group.category,
                &active,
                &module_path,
                &function_name,
                result_var,
                is_async,
                &e2e_config.call.args,
                options_type.as_deref(),
            );
            files.push(GeneratedFile {
                path: tests_base.join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "typescript"
    }
}

fn render_package_json(pkg_name: &str, pkg_path: &str) -> String {
    format!(
        r#"{{
  "name": "{pkg_name}-e2e-typescript",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {{
    "test": "vitest run"
  }},
  "devDependencies": {{
    "{pkg_name}": "file:{pkg_path}",
    "vitest": "^3.0.0"
  }}
}}
"#
    )
}

fn render_tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "noUncheckedIndexedAccess": true
  },
  "include": ["tests/**/*.ts", "vitest.config.ts"]
}
"#
    .to_string()
}

fn render_vitest_config() -> String {
    r#"import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
  },
});
"#
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    module_path: &str,
    function_name: &str,
    result_var: &str,
    is_async: bool,
    args: &[crate::config::ArgMapping],
    options_type: Option<&str>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "import {{ describe, it, expect }} from 'vitest';");

    // Check if any fixture uses a json_object arg that needs the options type import.
    let needs_options_import = options_type.is_some()
        && fixtures.iter().any(|f| {
            args.iter().any(|arg| {
                arg.arg_type == "json_object" && f.input.get(&arg.field).is_some_and(|v| !v.is_null())
            })
        });

    if let (true, Some(opts_type)) = (needs_options_import, options_type) {
        let _ = writeln!(
            out,
            "import {{ {function_name}, type {opts_type} }} from '{module_path}';"
        );
    } else {
        let _ = writeln!(out, "import {{ {function_name} }} from '{module_path}';");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "describe('{category}', () => {{");

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_case(&mut out, fixture, function_name, result_var, is_async, args, options_type);
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "}});");
    out
}

fn render_test_case(
    out: &mut String,
    fixture: &Fixture,
    function_name: &str,
    result_var: &str,
    is_async: bool,
    args: &[crate::config::ArgMapping],
    options_type: Option<&str>,
) {
    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('\'', "\\'");
    let async_kw = if is_async { "async " } else { "" };
    let await_kw = if is_async { "await " } else { "" };

    // Check if this is an error-expecting test.
    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    if expects_error {
        let _ = writeln!(out, "  it('{test_name}: {description}', {async_kw}() => {{");
        let args_str = build_args_string(&fixture.input, args, options_type);
        if is_async {
            let _ = writeln!(
                out,
                "    await expect({async_kw}() => {await_kw}{function_name}({args_str})).rejects.toThrow();"
            );
        } else {
            let _ = writeln!(out, "    expect(() => {function_name}({args_str})).toThrow();");
        }
        let _ = writeln!(out, "  }});");
        return;
    }

    let _ = writeln!(out, "  it('{test_name}: {description}', {async_kw}() => {{");

    // Build function call arguments from input fields.
    let args_str = build_args_string(&fixture.input, args, options_type);

    // Emit variable declarations for input args (for readability in complex cases).
    let _ = writeln!(out, "    const {result_var} = {await_kw}{function_name}({args_str});");

    // Emit assertions.
    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var);
    }

    let _ = writeln!(out, "  }});");
}

fn build_args_string(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    options_type: Option<&str>,
) -> String {
    if args.is_empty() {
        // If no args mapping, pass the whole input as a single argument.
        return json_to_js(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            // For json_object args with options_type, cast the object literal.
            if arg.arg_type == "json_object" {
                if let Some(opts_type) = options_type {
                    return Some(format!("{} as {opts_type}", json_to_js(val)));
                }
            }
            Some(json_to_js(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => format!("{result_var}.{f}"),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let js_val = json_to_js(expected);
                let _ = writeln!(out, "    expect({field_expr}.trim()).toBe({js_val});");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let js_val = json_to_js(expected);
                let _ = writeln!(out, "    expect({field_expr}).toContain({js_val});");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let js_val = json_to_js(val);
                    let _ = writeln!(out, "    expect({field_expr}).toContain({js_val});");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let js_val = json_to_js(expected);
                let _ = writeln!(out, "    expect({field_expr}).not.toContain({js_val});");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "    expect({field_expr}.length).toBeGreaterThan(0);");
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let js_val = json_to_js(expected);
                let _ = writeln!(out, "    expect({field_expr}.startsWith({js_val})).toBe(true);");
            }
        }
        "not_error" => {
            // No-op — if we got here, the call succeeded (it would have thrown).
        }
        "error" => {
            // Handled at the test level (early return above).
        }
        other => {
            let _ = writeln!(out, "    // TODO: unsupported assertion type: {other}");
        }
    }
}

/// Convert a `serde_json::Value` to a JavaScript literal string.
fn json_to_js(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_js(s)),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_js).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            let entries: Vec<String> = map.iter().map(|(k, v)| format!("{}: {}", k, json_to_js(v))).collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}
