//! WebAssembly e2e test generator using vitest.
//!
//! Similar to the TypeScript generator but imports from a wasm package
//! and uses `language_name` "wasm".

use crate::config::E2eConfig;
use crate::escape::{escape_js, sanitize_filename, sanitize_ident};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::ToLowerCamelCase;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// WebAssembly e2e code generator.
pub struct WasmCodegen;

impl E2eCodegen for WasmCodegen {
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
        let options_type = overrides.and_then(|o| o.options_type.clone());
        let empty_enum_fields = HashMap::new();
        let enum_fields = overrides.map(|o| &o.enum_fields).unwrap_or(&empty_enum_fields);
        let result_var = &call.result_var;
        let is_async = call.r#async;

        // Resolve package config.
        let wasm_pkg = e2e_config.packages.get("wasm");
        let pkg_path = wasm_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../crates/html-to-markdown-wasm/pkg".to_string());
        let pkg_name = wasm_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());

        // Generate package.json.
        files.push(GeneratedFile {
            path: output_base.join("package.json"),
            content: render_package_json(&pkg_name, &pkg_path),
            generated_header: false,
        });

        // Generate vitest.config.ts.
        files.push(GeneratedFile {
            path: output_base.join("vitest.config.ts"),
            content: render_vitest_config(),
            generated_header: true,
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

            let filename = format!("{}.test.ts", sanitize_filename(&group.category));
            let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);
            let content = render_test_file(
                &group.category,
                &active,
                &pkg_name,
                &function_name,
                result_var,
                is_async,
                &e2e_config.call.args,
                &field_resolver,
                options_type.as_deref(),
                enum_fields,
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
        "wasm"
    }
}

fn render_package_json(pkg_name: &str, pkg_path: &str) -> String {
    format!(
        r#"{{
  "name": "{pkg_name}-e2e-wasm",
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
    pkg_name: &str,
    function_name: &str,
    result_var: &str,
    is_async: bool,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    options_type: Option<&str>,
    enum_fields: &HashMap<String, String>,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "import {{ describe, it, expect }} from 'vitest';");

    // Check if any fixture uses a json_object arg that needs the options type import.
    let needs_options_import = options_type.is_some()
        && fixtures.iter().any(|f| {
            args.iter()
                .any(|arg| arg.arg_type == "json_object" && f.input.get(&arg.field).is_some_and(|v| !v.is_null()))
        });

    if let (true, Some(opts_type)) = (needs_options_import, options_type) {
        let _ = writeln!(out, "import {{ {function_name}, {opts_type} }} from '{pkg_name}';");
    } else {
        let _ = writeln!(out, "import {{ {function_name} }} from '{pkg_name}';");
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "describe('{category}', () => {{");

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_case(
            &mut out,
            fixture,
            function_name,
            result_var,
            is_async,
            args,
            field_resolver,
            options_type,
            enum_fields,
        );
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "}});");
    out
}

#[allow(clippy::too_many_arguments)]
fn render_test_case(
    out: &mut String,
    fixture: &Fixture,
    function_name: &str,
    result_var: &str,
    is_async: bool,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
    options_type: Option<&str>,
    enum_fields: &HashMap<String, String>,
) {
    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('\'', "\\'");
    let async_kw = if is_async { "async " } else { "" };
    let await_kw = if is_async { "await " } else { "" };

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    if expects_error {
        let _ = writeln!(out, "  it('{test_name}: {description}', {async_kw}() => {{");
        let args_str = build_args_string(&fixture.input, args, options_type, enum_fields);
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

    // Check if we need to emit options setup code.
    let has_options_setup = options_type.is_some()
        && args
            .iter()
            .any(|arg| arg.arg_type == "json_object" && fixture.input.get(&arg.field).is_some_and(|v| !v.is_null()));

    if has_options_setup {
        // Emit options construction via default + setter pattern.
        if let Some(opts_type) = options_type {
            for arg in args {
                if arg.arg_type == "json_object" {
                    if let Some(val) = fixture.input.get(&arg.field) {
                        if !val.is_null() {
                            if let Some(obj) = val.as_object() {
                                let _ = writeln!(out, "    const options = {opts_type}.default();");
                                for (k, v) in obj {
                                    let camel_key = k.to_lower_camel_case();
                                    let js_val = if enum_fields.contains_key(k) {
                                        json_to_js(v)
                                    } else {
                                        json_to_js(v)
                                    };
                                    let _ = writeln!(out, "    options.{camel_key} = {js_val};");
                                }
                            }
                        }
                    }
                }
            }
        }
        // Build call args, replacing the json_object arg with the `options` variable.
        let call_args: Vec<String> = args
            .iter()
            .filter_map(|arg| {
                let val = fixture.input.get(&arg.field)?;
                if val.is_null() && arg.optional {
                    return None;
                }
                if arg.arg_type == "json_object" && !val.is_null() && options_type.is_some() {
                    return Some("options".to_string());
                }
                Some(json_to_js(val))
            })
            .collect();
        let args_str = call_args.join(", ");
        let _ = writeln!(out, "    const {result_var} = {await_kw}{function_name}({args_str});");
    } else {
        let args_str = build_args_string(&fixture.input, args, options_type, enum_fields);
        let _ = writeln!(out, "    const {result_var} = {await_kw}{function_name}({args_str});");
    }

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "  }});");
}

fn build_args_string(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    _options_type: Option<&str>,
    _enum_fields: &HashMap<String, String>,
) -> String {
    if args.is_empty() {
        return json_to_js(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            Some(json_to_js(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "wasm", result_var),
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
            // No-op — if we got here, the call succeeded.
        }
        "error" => {
            // Handled at the test level.
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
