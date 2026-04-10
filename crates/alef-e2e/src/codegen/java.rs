//! Java e2e test generator using JUnit 5.
//!
//! Generates `e2e/java/pom.xml` and `src/test/java/dev/kreuzberg/e2e/{Category}Test.java`
//! files from JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

use crate::config::E2eConfig;
use crate::escape::{escape_java, sanitize_filename};
use crate::field_access::FieldResolver;
use crate::fixture::{Assertion, Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use anyhow::Result;
use heck::ToUpperCamelCase;
use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use super::E2eCodegen;

/// Java e2e code generator.
pub struct JavaCodegen;

impl E2eCodegen for JavaCodegen {
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
        let class_name = overrides
            .and_then(|o| o.class.as_ref())
            .cloned()
            .unwrap_or_else(|| alef_config.crate_config.name.to_upper_camel_case());
        let result_var = &call.result_var;

        // Resolve package config.
        let java_pkg = e2e_config.packages.get("java");
        let pkg_name = java_pkg
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| alef_config.crate_config.name.clone());

        // Generate pom.xml.
        files.push(GeneratedFile {
            path: output_base.join("pom.xml"),
            content: render_pom_xml(&pkg_name),
            generated_header: false,
        });

        // Generate test files per category.
        let test_base = output_base
            .join("src")
            .join("test")
            .join("java")
            .join("dev")
            .join("kreuzberg")
            .join("e2e");

        // Resolve options_type from override.
        let options_type = overrides.and_then(|o| o.options_type.clone());
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

            let class_file_name = format!("{}Test.java", sanitize_filename(&group.category).to_upper_camel_case());
            let content = render_test_file(
                &group.category,
                &active,
                &module_path,
                &class_name,
                &function_name,
                result_var,
                &e2e_config.call.args,
                options_type.as_deref(),
                &field_resolver,
            );
            files.push(GeneratedFile {
                path: test_base.join(class_file_name),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "java"
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_pom_xml(pkg_name: &str) -> String {
    let artifact_id = format!("{pkg_name}-e2e-java");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>dev.kreuzberg</groupId>
    <artifactId>{artifact_id}</artifactId>
    <version>0.1.0</version>

    <properties>
        <maven.compiler.source>21</maven.compiler.source>
        <maven.compiler.target>21</maven.compiler.target>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
        <junit.version>5.11.4</junit.version>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>${{junit.version}}</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>org.codehaus.mojo</groupId>
                <artifactId>build-helper-maven-plugin</artifactId>
                <version>3.6.0</version>
                <executions>
                    <execution>
                        <id>add-test-source</id>
                        <phase>generate-test-sources</phase>
                        <goals>
                            <goal>add-test-source</goal>
                        </goals>
                        <configuration>
                            <sources>
                                <source>src/test/java</source>
                            </sources>
                        </configuration>
                    </execution>
                </executions>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-surefire-plugin</artifactId>
                <version>3.5.2</version>
                <configuration>
                    <argLine>--enable-preview --enable-native-access=ALL-UNNAMED -Djava.library.path=../../target/release</argLine>
                </configuration>
            </plugin>
        </plugins>
    </build>
</project>
"#
    )
}

#[allow(clippy::too_many_arguments)]
fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    import_class: &str,
    class_name: &str,
    function_name: &str,
    result_var: &str,
    args: &[crate::config::ArgMapping],
    options_type: Option<&str>,
    field_resolver: &FieldResolver,
) -> String {
    let mut out = String::new();
    let test_class_name = format!("{}Test", sanitize_filename(category).to_upper_camel_case());

    let _ = writeln!(out, "package dev.kreuzberg.e2e;");
    let _ = writeln!(out);

    // Check if any fixture uses a json_object arg with options_type (needs ObjectMapper).
    let needs_object_mapper = options_type.is_some()
        && fixtures.iter().any(|f| {
            args.iter()
                .any(|arg| arg.arg_type == "json_object" && f.input.get(&arg.field).is_some_and(|v| !v.is_null()))
        });

    let _ = writeln!(out, "import org.junit.jupiter.api.Test;");
    let _ = writeln!(out, "import static org.junit.jupiter.api.Assertions.*;");
    if !import_class.is_empty() {
        let _ = writeln!(out, "import {import_class};");
    }
    if needs_object_mapper {
        let _ = writeln!(out, "import com.fasterxml.jackson.databind.ObjectMapper;");
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "/** E2e tests for category: {category}. */");
    let _ = writeln!(out, "class {test_class_name} {{");

    if needs_object_mapper {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "    private static final ObjectMapper MAPPER = new ObjectMapper();"
        );
    }

    for fixture in fixtures {
        render_test_method(
            &mut out,
            fixture,
            class_name,
            function_name,
            result_var,
            args,
            options_type,
            field_resolver,
        );
        let _ = writeln!(out);
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
    options_type: Option<&str>,
    field_resolver: &FieldResolver,
) {
    let method_name = fixture.id.to_upper_camel_case();
    let description = &fixture.description;
    let expects_error = fixture.assertions.iter().any(|a| a.assertion_type == "error");

    // Check if this test needs ObjectMapper deserialization for json_object args.
    let needs_deser = options_type.is_some()
        && args
            .iter()
            .any(|arg| arg.arg_type == "json_object" && fixture.input.get(&arg.field).is_some_and(|v| !v.is_null()));

    let throws_clause = if needs_deser { " throws Exception" } else { "" };

    let _ = writeln!(out, "    @Test");
    let _ = writeln!(out, "    void test{method_name}(){throws_clause} {{");
    let _ = writeln!(out, "        // {description}");

    // Emit ObjectMapper deserialization bindings for json_object args.
    if let (true, Some(opts_type)) = (needs_deser, options_type) {
        for arg in args {
            if arg.arg_type == "json_object" {
                if let Some(val) = fixture.input.get(&arg.field) {
                    if !val.is_null() {
                        let json_str = serde_json::to_string(val).unwrap_or_default();
                        let var_name = &arg.name;
                        let _ = writeln!(
                            out,
                            "        var {var_name} = MAPPER.readValue(\"{}\", {opts_type}.class);",
                            escape_java(&json_str)
                        );
                    }
                }
            }
        }
    }

    let args_str = build_args_string(&fixture.input, args, options_type);

    if expects_error {
        let _ = writeln!(
            out,
            "        assertThrows(Exception.class, () -> {class_name}.{function_name}({args_str}));"
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

fn build_args_string(
    input: &serde_json::Value,
    args: &[crate::config::ArgMapping],
    options_type: Option<&str>,
) -> String {
    if args.is_empty() {
        return json_to_java(input);
    }

    let parts: Vec<String> = args
        .iter()
        .filter_map(|arg| {
            let val = input.get(&arg.field)?;
            if val.is_null() && arg.optional {
                return None;
            }
            // For json_object args with options_type, use the pre-deserialized variable.
            if arg.arg_type == "json_object" && options_type.is_some() {
                return Some(arg.name.clone());
            }
            Some(json_to_java(val))
        })
        .collect();

    parts.join(", ")
}

fn render_assertion(out: &mut String, assertion: &Assertion, result_var: &str, field_resolver: &FieldResolver) {
    let field_expr = match &assertion.field {
        Some(f) if !f.is_empty() => field_resolver.accessor(f, "java", result_var),
        _ => result_var.to_string(),
    };

    match assertion.assertion_type.as_str() {
        "equals" => {
            if let Some(expected) = &assertion.value {
                let java_val = json_to_java(expected);
                let _ = writeln!(out, "        assertEquals({java_val}, {field_expr}.strip());");
            }
        }
        "contains" => {
            if let Some(expected) = &assertion.value {
                let java_val = json_to_java(expected);
                let _ = writeln!(
                    out,
                    "        assertTrue({field_expr}.contains({java_val}), \"expected to contain: \" + {java_val});"
                );
            }
        }
        "contains_all" => {
            if let Some(values) = &assertion.values {
                for val in values {
                    let java_val = json_to_java(val);
                    let _ = writeln!(
                        out,
                        "        assertTrue({field_expr}.contains({java_val}), \"expected to contain: \" + {java_val});"
                    );
                }
            }
        }
        "not_contains" => {
            if let Some(expected) = &assertion.value {
                let java_val = json_to_java(expected);
                let _ = writeln!(
                    out,
                    "        assertFalse({field_expr}.contains({java_val}), \"expected NOT to contain: \" + {java_val});"
                );
            }
        }
        "not_empty" => {
            let _ = writeln!(
                out,
                "        assertFalse({field_expr}.isEmpty(), \"expected non-empty value\");"
            );
        }
        "is_empty" => {
            let _ = writeln!(
                out,
                "        assertTrue({field_expr}.isEmpty(), \"expected empty value\");"
            );
        }
        "starts_with" => {
            if let Some(expected) = &assertion.value {
                let java_val = json_to_java(expected);
                let _ = writeln!(
                    out,
                    "        assertTrue({field_expr}.startsWith({java_val}), \"expected to start with: \" + {java_val});"
                );
            }
        }
        "ends_with" => {
            if let Some(expected) = &assertion.value {
                let java_val = json_to_java(expected);
                let _ = writeln!(
                    out,
                    "        assertTrue({field_expr}.endsWith({java_val}), \"expected to end with: \" + {java_val});"
                );
            }
        }
        "min_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "        assertTrue({field_expr}.length() >= {n}, \"expected length >= {n}\");"
                    );
                }
            }
        }
        "max_length" => {
            if let Some(val) = &assertion.value {
                if let Some(n) = val.as_u64() {
                    let _ = writeln!(
                        out,
                        "        assertTrue({field_expr}.length() <= {n}, \"expected length <= {n}\");"
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

/// Convert a `serde_json::Value` to a Java literal string.
fn json_to_java(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_java(s)),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                format!("{}d", n)
            } else {
                n.to_string()
            }
        }
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(json_to_java).collect();
            format!("java.util.List.of({})", items.join(", "))
        }
        serde_json::Value::Object(_) => {
            let json_str = serde_json::to_string(value).unwrap_or_default();
            format!("\"{}\"", escape_java(&json_str))
        }
    }
}
