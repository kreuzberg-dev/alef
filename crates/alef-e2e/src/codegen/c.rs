//! C e2e test generator using assert.h and a Makefile.
//!
//! Generates `e2e/c/Makefile`, per-category `test_{category}.c` files,
//! a `main.c` test runner, and a `test_runner.h` header.

use crate::config::E2eConfig;
use crate::escape::{escape_c, sanitize_filename, sanitize_ident};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// C e2e code generator.
pub struct CCodegen;

impl E2eCodegen for CCodegen {
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
        let function_name = overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call.function.clone());
        let result_var = &call.result_var;
        let prefix = overrides.and_then(|o| o.prefix.as_ref()).cloned().unwrap_or_default();
        let header = overrides
            .and_then(|o| o.header.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("{}.h", call.module));

        // Resolve package config.
        let c_pkg = e2e_config.packages.get("c");
        let include_path = c_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../crates/ffi/include".to_string());
        let lib_path = c_pkg
            .and_then(|p| p.module.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../target/release".to_string());
        let lib_name = c_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| call.module.clone());

        // Filter active groups (with non-skipped fixtures).
        let active_groups: Vec<(&FixtureGroup, Vec<&Fixture>)> = groups
            .iter()
            .filter_map(|group| {
                let active: Vec<&Fixture> = group
                    .fixtures
                    .iter()
                    .filter(|f| f.skip.as_ref().is_none_or(|s| !s.should_skip(lang)))
                    .collect();
                if active.is_empty() { None } else { Some((group, active)) }
            })
            .collect();

        // Generate Makefile.
        let category_names: Vec<String> = active_groups
            .iter()
            .map(|(g, _)| sanitize_filename(&g.category))
            .collect();
        files.push(GeneratedFile {
            path: output_base.join("Makefile"),
            content: render_makefile(&category_names, &include_path, &lib_path, &lib_name),
            generated_header: true,
        });

        // Generate test_runner.h.
        files.push(GeneratedFile {
            path: output_base.join("test_runner.h"),
            content: render_test_runner_header(&active_groups),
            generated_header: true,
        });

        // Generate main.c.
        files.push(GeneratedFile {
            path: output_base.join("main.c"),
            content: render_main_c(&active_groups),
            generated_header: true,
        });

        let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);

        // Generate per-category test files.
        for (group, active) in &active_groups {
            let filename = format!("test_{}.c", sanitize_filename(&group.category));
            let content = render_test_file(
                &group.category,
                active,
                &header,
                &prefix,
                &function_name,
                result_var,
                &e2e_config.call.args,
                &field_resolver,
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
        "c"
    }
}

fn render_makefile(categories: &[String], include_path: &str, lib_path: &str, lib_name: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "CC = gcc");
    let _ = writeln!(out, "CFLAGS = -Wall -Wextra -I{include_path}");
    let _ = writeln!(out, "LDFLAGS = -L{lib_path} -l{lib_name}");
    let _ = writeln!(out);

    let src_files: Vec<String> = categories.iter().map(|c| format!("test_{c}.c")).collect();
    let srcs = src_files.join(" ");

    let _ = writeln!(out, "SRCS = main.c {srcs}");
    let _ = writeln!(out, "TARGET = run_tests");
    let _ = writeln!(out);
    let _ = writeln!(out, ".PHONY: all clean test");
    let _ = writeln!(out);
    let _ = writeln!(out, "all: $(TARGET)");
    let _ = writeln!(out);
    let _ = writeln!(out, "$(TARGET): $(SRCS)");
    let _ = writeln!(out, "\t$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)");
    let _ = writeln!(out);
    let _ = writeln!(out, "test: $(TARGET)");
    let _ = writeln!(out, "\t./$(TARGET)");
    let _ = writeln!(out);
    let _ = writeln!(out, "clean:");
    let _ = writeln!(out, "\trm -f $(TARGET)");
    out
}

fn render_test_runner_header(active_groups: &[(&FixtureGroup, Vec<&Fixture>)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#ifndef TEST_RUNNER_H");
    let _ = writeln!(out, "#define TEST_RUNNER_H");
    let _ = writeln!(out);

    for (group, fixtures) in active_groups {
        let _ = writeln!(out, "/* Tests for category: {} */", group.category);
        for fixture in fixtures {
            let fn_name = sanitize_ident(&fixture.id);
            let _ = writeln!(out, "void test_{fn_name}(void);");
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "#endif /* TEST_RUNNER_H */");
    out
}

fn render_main_c(active_groups: &[(&FixtureGroup, Vec<&Fixture>)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#include <stdio.h>");
    let _ = writeln!(out, "#include \"test_runner.h\"");
    let _ = writeln!(out);
    let _ = writeln!(out, "int main(void) {{");
    let _ = writeln!(out, "    int passed = 0;");
    let _ = writeln!(out, "    int failed = 0;");
    let _ = writeln!(out);

    for (group, fixtures) in active_groups {
        let _ = writeln!(out, "    /* Category: {} */", group.category);
        for fixture in fixtures {
            let fn_name = sanitize_ident(&fixture.id);
            let _ = writeln!(out, "    printf(\"  Running test_{fn_name}...\");");
            let _ = writeln!(out, "    test_{fn_name}();");
            let _ = writeln!(out, "    printf(\" PASSED\\n\");");
            let _ = writeln!(out, "    passed++;");
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(
        out,
        "    printf(\"\\nResults: %d passed, %d failed\\n\", passed, failed);"
    );
    let _ = writeln!(out, "    return failed > 0 ? 1 : 0;");
    let _ = writeln!(out, "}}");
    out
}

fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    header: &str,
    prefix: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "/* E2e tests for category: {category} */");
    let _ = writeln!(out);
    let _ = writeln!(out, "#include <assert.h>");
    let _ = writeln!(out, "#include <string.h>");
    let _ = writeln!(out, "#include <stdio.h>");
    let _ = writeln!(out, "#include <stdlib.h>");
    let _ = writeln!(out, "#include \"{header}\"");
    let _ = writeln!(out);

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_function(&mut out, fixture, prefix, function_name, result_var, args, field_resolver);
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    out
}

fn render_test_function(
    out: &mut String,
    fixture: &Fixture,
    prefix: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) {
    let fn_name = sanitize_ident(&fixture.id);
    let description = &fixture.description;

    let prefixed_fn = if prefix.is_empty() {
        function_name.to_string()
    } else {
        format!("{prefix}_{function_name}")
    };

    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let args_str = build_args_string(&fixture.input, args);

    let _ = writeln!(out, "void test_{fn_name}(void) {{");
    let _ = writeln!(out, "    /* {description} */");

    if expects_error {
        let _ = writeln!(out, "    const char* {result_var} = {prefixed_fn}({args_str});");
        let _ = writeln!(out, "    assert({result_var} == NULL && \"expected call to fail\");");
        let _ = writeln!(out, "}}");
        return;
    }

    let _ = writeln!(out, "    const char* {result_var} = {prefixed_fn}({args_str});");
    let _ = writeln!(out, "    assert({result_var} != NULL && \"expected call to succeed\");");

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "}}");
}

fn build_args_string(input: &serde_json::Value, args: &[crate::config::ArgMapping]) -> String {
    if args.is_empty() {
        return json_to_c(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            Some(json_to_c(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "c", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let c_val = json_to_c(expected);
                let _ = writeln!(
                    out,
                    "    assert(strcmp({field_expr}, {c_val}) == 0 && \"equals assertion failed\");"
                );
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let c_val = json_to_c(expected);
                let _ = writeln!(
                    out,
                    "    assert(strstr({field_expr}, {c_val}) != NULL && \"expected to contain substring\");"
                );
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let c_val = json_to_c(val);
                    let _ = writeln!(
                        out,
                        "    assert(strstr({field_expr}, {c_val}) != NULL && \"expected to contain substring\");"
                    );
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let c_val = json_to_c(expected);
                let _ = writeln!(
                    out,
                    "    assert(strstr({field_expr}, {c_val}) == NULL && \"expected NOT to contain substring\");"
                );
            }
        }
        "not_empty" => {
            let _ = writeln!(
                out,
                "    assert(strlen({field_expr}) > 0 && \"expected non-empty value\");"
            );
        }
        "is_empty" => {
            let _ = writeln!(
                out,
                "    assert(strlen({field_expr}) == 0 && \"expected empty value\");"
            );
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let c_val = json_to_c(expected);
                let _ = writeln!(
                    out,
                    "    assert(strncmp({field_expr}, {c_val}, strlen({c_val})) == 0 && \"expected to start with\");"
                );
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let c_val = json_to_c(expected);
                let _ = writeln!(out, "    assert(strlen({field_expr}) >= strlen({c_val}) && ");
                let _ = writeln!(
                    out,
                    "           strcmp({field_expr} + strlen({field_expr}) - strlen({c_val}), {c_val}) == 0 && \"expected to end with\");"
                );
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "    assert(strlen({field_expr}) >= {n} && \"expected minimum length\");"
                    );
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "    assert(strlen({field_expr}) <= {n} && \"expected maximum length\");"
                    );
                }
            }
        }
        "not_error" => {
            // Already handled — the NULL check above covers this.
        }
        "error" => {
            // Handled at the test function level.
        }
        other => {
            let _ = writeln!(out, "    /* TODO: unsupported assertion type: {other} */");
        }
    }
}

/// Convert a `serde_json::Value` to a C literal string.
fn json_to_c(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_c(s)),
        serde_json::Value::Bool(true) => "1".to_string(),
        serde_json::Value::Bool(false) => "0".to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        other => format!("\"{}\"", escape_c(&other.to_string())),
    }
}
