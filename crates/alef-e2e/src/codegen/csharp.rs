//! C# e2e test generator using xUnit.
//!
//! Generates `e2e/csharp/E2eTests.csproj` and `tests/{Category}Tests.cs`
//! files from JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

use crate::config::E2eConfig;
use crate::escape::{escape_csharp, sanitize_filename};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::ToUpperCamelCase;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// C# e2e code generator.
pub struct CSharpCodegen;

impl E2eCodegen for CSharpCodegen {
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
        let function_name = overrides
            .and_then(|o| o.function.as_ref())
            .cloned()
            .unwrap_or_else(|| call.function.to_upper_camel_case());
        let class_name = overrides
            .and_then(|o| o.class.as_ref())
            .cloned()
            .unwrap_or_else(|| alef_config.crate_config.name.to_upper_camel_case());
        let namespace = overrides.and_then(|o| o.module.as_ref()).cloned().unwrap_or_else(|| {
            if call.module.is_empty() {
                "Kreuzberg".to_string()
            } else {
                call.module.to_upper_camel_case()
            }
        });
        let result_var = &call.result_var;

        // Resolve package config.
        let cs_pkg = e2e_config.packages.get("csharp");
        let pkg_name = cs_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| alef_config.crate_config.name.to_upper_camel_case());
        let pkg_path = cs_pkg
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("../../packages/csharp/{pkg_name}.csproj"));

        // Generate E2eTests.csproj.
        files.push(GeneratedFile {
            path: output_base.join("E2eTests.csproj"),
            content: render_csproj(&pkg_name, &pkg_path),
            generated_header: false,
        });

        // Generate test files per category.
        let tests_base = output_base.join("tests");
        let field_resolver = FieldResolver::new(&e2e_config.fields, &e2e_config.fields_optional);

        for group in groups {
            let active: Vec<&Fixture> = group
                .fixtures
                .iter()
                .filter(|f| f.skip.as_ref().is_none_or(|s| !s.should_skip(lang)))
                .collect();

            if active.is_empty() {
                continue;
            }

            let test_class = format!("{}Tests", sanitize_filename(&group.category).to_upper_camel_case());
            let filename = format!("{test_class}.cs");
            let content = render_test_file(
                &group.category,
                &active,
                &namespace,
                &class_name,
                &function_name,
                result_var,
                &test_class,
                &e2e_config.call.args,
                &field_resolver,
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
        "csharp"
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_csproj(_pkg_name: &str, pkg_path: &str) -> String {
    format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <IsPackable>false</IsPackable>
    <IsTestProject>true</IsTestProject>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.12.0" />
    <PackageReference Include="xunit" Version="2.9.3" />
    <PackageReference Include="xunit.runner.visualstudio" Version="2.8.2" />
  </ItemGroup>

  <ItemGroup>
    <ProjectReference Include="{pkg_path}" />
  </ItemGroup>
</Project>
"#
    )
}

#[allow(clippy::too_many_arguments)]
fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    namespace: &str,
    class_name: &str,
    function_name: &str,
    result_var: &str,
    test_class: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "using Xunit;");
    let _ = writeln!(out, "using {namespace};");
    let _ = writeln!(out);
    let _ = writeln!(out, "namespace Kreuzberg.E2e;");
    let _ = writeln!(out);
    let _ = writeln!(out, "/// <summary>E2e tests for category: {category}.</summary>");
    let _ = writeln!(out, "public class {test_class}");
    let _ = writeln!(out, "{{");

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_method(&mut out, fixture, class_name, function_name, result_var, args, field_resolver);
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "}}");
    out
}

fn render_test_method(
    out: &mut String,
    fixture: &Fixture,
    class_name: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    field_resolver: &FieldResolver,
) {
    let method_name = fixture.id.to_upper_camel_case();
    let description = &fixture.description;
    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    let args_str = build_args_string(&fixture.input, args);

    let _ = writeln!(out, "    [Fact]");
    let _ = writeln!(out, "    public void Test_{method_name}()");
    let _ = writeln!(out, "    {{");
    let _ = writeln!(out, "        // {description}");

    if expects_error {
        let _ = writeln!(
            out,
            "        Assert.Throws<Exception>(() => {class_name}.{function_name}({args_str}));"
        );
        let _ = writeln!(out, "    }}");
        return;
    }

    let _ = writeln!(
        out,
        "        var {result_var} = {class_name}.{function_name}({args_str});"
    );

    for assertion in &fixture.assertions {
        render_assertion(out, assertion, result_var, field_resolver);
    }

    let _ = writeln!(out, "    }}");
}

fn build_args_string(input: &serde_json::Value, args: &[crate::config::ArgMapping]) -> String {
    if args.is_empty() {
        return json_to_csharp(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            Some(json_to_csharp(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "csharp", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let cs_val = json_to_csharp(expected);
                let _ = writeln!(out, "        Assert.Equal({cs_val}, {field_expr}.Trim());");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let cs_val = json_to_csharp(expected);
                let _ = writeln!(out, "        Assert.Contains({cs_val}, {field_expr});");
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let cs_val = json_to_csharp(val);
                    let _ = writeln!(out, "        Assert.Contains({cs_val}, {field_expr});");
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let cs_val = json_to_csharp(expected);
                let _ = writeln!(out, "        Assert.DoesNotContain({cs_val}, {field_expr});");
            }
        }
        "not_empty" => {
            let _ = writeln!(out, "        Assert.NotEmpty({field_expr});");
        }
        "is_empty" => {
            let _ = writeln!(out, "        Assert.Empty({field_expr});");
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let cs_val = json_to_csharp(expected);
                let _ = writeln!(out, "        Assert.StartsWith({cs_val}, {field_expr});");
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let cs_val = json_to_csharp(expected);
                let _ = writeln!(out, "        Assert.EndsWith({cs_val}, {field_expr});");
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "        Assert.True({field_expr}.Length >= {n}, \"expected length >= {n}\");"
                    );
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "        Assert.True({field_expr}.Length <= {n}, \"expected length <= {n}\");"
                    );
                }
            }
        }
        "not_error" => {
            // Already handled by the call succeeding without exception.
        }
        "error" => {
            // Handled at the test method level.
        }
        other => {
            let _ = writeln!(out, "        // TODO: unsupported assertion type: {other}");
        }
    }
}

/// Convert a `serde_json::Value` to a C# literal string.
fn json_to_csharp(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_csharp(s)),
        serde_json::Value::Bool(true) => "true".to_string(),
        serde_json::Value::Bool(false) => "false".to_string(),
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                format!("{}d", n)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_csharp).collect();
            format!("new[] {{ {} }}", items.join(", "))
        }
        serde_json::Value::Object(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            format!("\"{}\"", escape_csharp(&json_str))
        }
    }
}
