//! WebAssembly e2e test generator using vitest.
//!
//! Similar to the TypeScript generator but imports from a wasm package
//! and uses `language_name` "wasm".

use crate::config::E2eConfig;
use crate::escape::{escape_js, sanitize_filename, sanitize_ident};
use crate::fixture::{Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::AlefConfig;
use alef_core::hash::{self, CommentStyle};
use alef_core::template_versions as tv;
use anyhow::Result;
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
        alef_config: &AlefConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(e2e_config.effective_output()).join(lang);
        let tests_base = output_base.join("tests");

        let mut files = Vec::new();

        // Resolve call config with overrides.
        let call = &e2e_config.call;
        let overrides = call.overrides.get(lang);
        let module_path = overrides
            .and_then(|o| o.module.as_ref())
            .cloned()
            .unwrap_or_else(|| call.module.clone());

        // Resolve package config.
        let wasm_pkg = e2e_config.resolve_package("wasm");
        let pkg_path = wasm_pkg
            .as_ref()
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("../../crates/{}-wasm/pkg", alef_config.crate_config.name));
        let pkg_name = wasm_pkg
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());
        let pkg_version = wasm_pkg
            .as_ref()
            .and_then(|p| p.version.as_ref())
            .cloned()
            .unwrap_or_else(|| "0.1.0".to_string());

        // Generate package.json.
        files.push(GeneratedFile {
            path: output_base.join("package.json"),
            content: render_package_json(&pkg_name, &pkg_path, &pkg_version, e2e_config.dep_mode),
            generated_header: false,
        });

        // Generate vitest.config.ts.
        files.push(GeneratedFile {
            path: output_base.join("vitest.config.ts"),
            content: render_vitest_config(),
            generated_header: true,
        });

        // Generate globalSetup.ts for spawning the mock server.
        files.push(GeneratedFile {
            path: output_base.join("globalSetup.ts"),
            content: render_global_setup(),
            generated_header: true,
        });

        // Generate tsconfig.json (prevents Vite from walking up to root tsconfig).
        files.push(GeneratedFile {
            path: output_base.join("tsconfig.json"),
            content: render_tsconfig(),
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

            let filename = format!("{}.test.ts", sanitize_filename(&group.category));
            let content = render_test_file(&group.category, &active);
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

fn render_package_json(
    pkg_name: &str,
    pkg_path: &str,
    pkg_version: &str,
    dep_mode: crate::config::DependencyMode,
) -> String {
    let dep_value = match dep_mode {
        crate::config::DependencyMode::Registry => pkg_version.to_string(),
        crate::config::DependencyMode::Local => format!("file:{pkg_path}"),
    };
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
    "{pkg_name}": "{dep_value}",
    "vite-plugin-top-level-await": "{vite_plugin_top_level_await}",
    "vite-plugin-wasm": "{vite_plugin_wasm}",
    "vitest": "{vitest}"
  }}
}}
"#,
        vite_plugin_top_level_await = tv::npm::VITE_PLUGIN_TOP_LEVEL_AWAIT,
        vite_plugin_wasm = tv::npm::VITE_PLUGIN_WASM,
        vitest = tv::npm::VITEST,
    )
}

fn render_vitest_config() -> String {
    let header = hash::header(CommentStyle::DoubleSlash);
    format!(
        r#"{header}import {{ defineConfig }} from 'vitest/config';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

export default defineConfig({{
  plugins: [wasm(), topLevelAwait()],
  test: {{
    include: ['tests/**/*.test.ts'],
    globalSetup: './globalSetup.ts',
  }},
}});
"#
    )
}

fn render_global_setup() -> String {
    let header = hash::header(CommentStyle::DoubleSlash);
    format!(
        r#"{header}import {{ spawn }} from 'child_process';
import {{ resolve }} from 'path';

let serverProcess;

export async function setup() {{
  // Mock server binary must be pre-built (e.g. by CI or `cargo build --manifest-path e2e/rust/Cargo.toml --bin mock-server --release`)
  serverProcess = spawn(
    resolve(__dirname, '../rust/target/release/mock-server'),
    [resolve(__dirname, '../../fixtures')],
    {{ stdio: ['pipe', 'pipe', 'inherit'] }}
  );

  const url = await new Promise((resolve, reject) => {{
    serverProcess.stdout.on('data', (data) => {{
      const match = data.toString().match(/MOCK_SERVER_URL=(.*)/);
      if (match) resolve(match[1].trim());
    }});
    setTimeout(() => reject(new Error('Mock server startup timeout')), 30000);
  }});

  process.env.MOCK_SERVER_URL = url;
}}

export async function teardown() {{
  if (serverProcess) {{
    serverProcess.stdin.end();
    serverProcess.kill();
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
    "strictNullChecks": false,
    "esModuleInterop": true,
    "skipLibCheck": true
  },
  "include": ["tests/**/*.ts", "vitest.config.ts"]
}
"#
    .to_string()
}

fn render_test_file(category: &str, fixtures: &[&Fixture]) -> String {
    let mut out = String::new();
    out.push_str(&hash::header(CommentStyle::DoubleSlash));
    let _ = writeln!(out, "import {{ describe, expect, it }} from 'vitest';");
    let _ = writeln!(out);
    let _ = writeln!(out, "describe('{category}', () => {{");

    for (i, fixture) in fixtures.iter().enumerate() {
        render_http_test_case(&mut out, fixture);
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "}});");
    out
}

/// Render a vitest `it` block for an HTTP server fixture via fetch.
///
/// Wasm e2e tests run under vitest+node, so they can use global `fetch` to hit the mock server.
fn render_http_test_case(out: &mut String, fixture: &Fixture) {
    let Some(http) = &fixture.http else {
        return;
    };

    let test_name = sanitize_ident(&fixture.id);
    let description = fixture.description.replace('\'', "\\'");

    // HTTP 101 (WebSocket upgrade) — fetch cannot handle upgrade responses.
    if http.expected_response.status_code == 101 {
        let _ = writeln!(out, "  it.skip('{test_name}: {description}', async () => {{");
        let _ = writeln!(out, "    // HTTP 101 WebSocket upgrade cannot be tested via fetch");
        let _ = writeln!(out, "  }});");
        return;
    }

    let method = http.request.method.to_uppercase();

    // Build the init object for `fetch(url, init)`.
    let mut init_entries: Vec<String> = Vec::new();
    init_entries.push(format!("method: '{method}'"));
    // Do not follow redirects — tests that assert on 3xx status codes need the original response.
    init_entries.push("redirect: 'manual'".to_string());

    // Headers
    if !http.request.headers.is_empty() {
        let entries: Vec<String> = http
            .request
            .headers
            .iter()
            .map(|(k, v)| {
                let expanded_v = v.clone();
                format!("      \"{}\": \"{}\"", escape_js(k), escape_js(&expanded_v))
            })
            .collect();
        init_entries.push(format!("headers: {{\n{},\n    }}", entries.join(",\n")));
    }

    // Body
    if let Some(body) = &http.request.body {
        let js_body = json_to_js(body);
        init_entries.push(format!("body: JSON.stringify({js_body})"));
    }

    let fixture_id = escape_js(&fixture.id);
    let _ = writeln!(out, "  it('{test_name}: {description}', async () => {{");
    let _ = writeln!(
        out,
        "    const baseUrl = process.env.MOCK_SERVER_URL ?? \"http://localhost:8080\";"
    );
    let _ = writeln!(out, "    const mockUrl = `${{baseUrl}}/fixtures/{fixture_id}`;");

    let init_str = init_entries.join(", ");
    let _ = writeln!(out, "    const response = await fetch(mockUrl, {{ {init_str} }});");

    // Status code assertion.
    let status = http.expected_response.status_code;
    let _ = writeln!(out, "    expect(response.status).toBe({status});");

    // Body assertions.
    if let Some(expected_body) = &http.expected_response.body {
        // Empty-string sentinel ("") and null mean no body — skip assertion.
        if !(expected_body.is_null() || expected_body.is_string() && expected_body.as_str() == Some("")) {
            if let serde_json::Value::String(s) = expected_body {
                // Plain-string body: mock server returns raw text, compare as text.
                let escaped = escape_js(s);
                let _ = writeln!(out, "    const text = await response.text();");
                let _ = writeln!(out, "    expect(text).toBe('{escaped}');");
            } else {
                let js_val = json_to_js(expected_body);
                let _ = writeln!(out, "    const data = await response.json();");
                let _ = writeln!(out, "    expect(data).toEqual({js_val});");
            }
        }
    } else if let Some(partial) = &http.expected_response.body_partial {
        let _ = writeln!(out, "    const data = await response.json();");
        if let Some(obj) = partial.as_object() {
            for (key, val) in obj {
                let js_key = escape_js(key);
                let js_val = json_to_js(val);
                let _ = writeln!(
                    out,
                    "    expect((data as Record<string, unknown>)['{js_key}']).toEqual({js_val});"
                );
            }
        }
    }

    // Header assertions.
    for (header_name, header_value) in &http.expected_response.headers {
        let lower_name = header_name.to_lowercase();
        // The mock server strips content-encoding headers because it returns uncompressed bodies.
        if lower_name == "content-encoding" {
            continue;
        }
        let escaped_name = escape_js(&lower_name);
        match header_value.as_str() {
            "<<present>>" => {
                let _ = writeln!(
                    out,
                    "    expect(response.headers.get('{escaped_name}')).not.toBeNull();"
                );
            }
            "<<absent>>" => {
                let _ = writeln!(out, "    expect(response.headers.get('{escaped_name}')).toBeNull();");
            }
            "<<uuid>>" => {
                let _ = writeln!(
                    out,
                    "    expect(response.headers.get('{escaped_name}')).toMatch(/^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$/);"
                );
            }
            exact => {
                let escaped_val = escape_js(exact);
                let _ = writeln!(
                    out,
                    "    expect(response.headers.get('{escaped_name}')).toBe('{escaped_val}');"
                );
            }
        }
    }

    // Validation error assertions — skip when a full body assertion is already generated
    // (redundant, and response.json() can only be called once per response).
    let body_has_content = matches!(&http.expected_response.body, Some(v)
        if !(v.is_null() || (v.is_string() && v.as_str() == Some(""))));
    if let Some(validation_errors) = &http.expected_response.validation_errors {
        if !validation_errors.is_empty() && !body_has_content {
            let _ = writeln!(
                out,
                "    const body = await response.json() as {{ errors?: unknown[] }};"
            );
            let _ = writeln!(out, "    const errors = body.errors ?? [];");
            for ve in validation_errors {
                let loc_js: Vec<String> = ve.loc.iter().map(|s| format!("\"{}\"", escape_js(s))).collect();
                let loc_str = loc_js.join(", ");
                let escaped_msg = escape_js(&ve.msg);
                let _ = writeln!(
                    out,
                    "    expect((errors as Array<Record<string, unknown>>).some((e) => JSON.stringify(e[\"loc\"]) === JSON.stringify([{loc_str}]) && String(e[\"msg\"]).includes(\"{escaped_msg}\"))).toBe(true);"
                );
            }
        }
    }

    let _ = writeln!(out, "  }});");
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
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key = if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                        && !k.starts_with(|c: char| c.is_ascii_digit())
                    {
                        k.clone()
                    } else {
                        format!("\"{}\"", escape_js(k))
                    };
                    format!("{key}: {}", json_to_js(v))
                })
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}
