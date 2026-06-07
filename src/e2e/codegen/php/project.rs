//! PHP e2e project-level file and bootstrap renderers.
//!
//! These helpers were previously defined in `php.rs` and are preserved here for
//! modularization.

use crate::core::hash::{self, CommentStyle};
use crate::core::template_versions as tv;
use crate::e2e::config::E2eConfig;
use crate::e2e::fixture::FixtureGroup;

pub(super) fn render_composer_json(
    e2e_pkg_name: &str,
    e2e_autoload_ns: &str,
    _extension_name: &str,
    pkg_name: &str,
    pkg_path: &str,
    _pkg_version: &str,
    dep_mode: crate::e2e::config::DependencyMode,
) -> String {
    let (require_section, autoload_section) = match dep_mode {
        crate::e2e::config::DependencyMode::Registry => {
            // Registry-mode test_apps run `install.sh` before `composer install`.
            // That script boots the PIE extension and installs the extension binary
            // into the system's extension_dir. Once PIE has installed the extension,
            // it will be loaded when PHP starts (via default php.ini or explicit
            // `-dextension=`).
            //
            // The test_app's composer.json does NOT declare `ext-<name>` as a
            // requirement because:
            // 1. The extension is installed via PIE, not Composer (Composer can't
            //    install system binaries).
            // 2. Declaring `ext-<name>: "*"` in composer.json causes Composer's
            //    platform resolver to check `php -m` for the extension. If the
            //    extension hasn't been loaded into the running PHP process yet (which
            //    it won't be until a fresh PHP invocation with the extension loaded),
            //    Composer fails with:
            //    "Root composer.json requires PHP extension ext-<name> * but it is
            //    missing".
            // 3. The extension is guaranteed to be loaded before tests run
            //    (install.sh ensures this).
            //
            // `php: ">=8.2"` is sufficient — Composer verifies the PHP version at
            // runtime (always satisfied on CI runners) and development dependencies
            // (phpunit, guzzle) are the only packages Composer needs to manage.
            let require = format!(
                r#"  "require": {{
    "php": ">=8.2"
  }},
  "require-dev": {{
    "phpunit/phpunit": "{phpunit}",
    "guzzlehttp/guzzle": "{guzzle}"
  }},"#,
                phpunit = tv::packagist::PHPUNIT,
                guzzle = tv::packagist::GUZZLE,
            );
            (require, String::new())
        }
        crate::e2e::config::DependencyMode::Local => {
            let require = format!(
                r#"  "require-dev": {{
    "phpunit/phpunit": "{phpunit}",
    "guzzlehttp/guzzle": "{guzzle}"
  }},"#,
                phpunit = tv::packagist::PHPUNIT,
                guzzle = tv::packagist::GUZZLE,
            );
            // For local mode, add autoload for the local package source.
            // Extract the namespace from pkg_name (org/module) and map it to src/.
            let pkg_namespace = pkg_name
                .split('/')
                .nth(1)
                .unwrap_or(pkg_name)
                .split('-')
                .map(heck::ToUpperCamelCase::to_upper_camel_case)
                .collect::<Vec<_>>()
                .join("\\");
            let autoload = format!(
                r#"
  "autoload": {{
    "psr-4": {{
      "{}\\": "{}/src/"
    }}
  }},"#,
                pkg_namespace.replace('\\', "\\\\"),
                pkg_path
            );
            (require, autoload)
        }
    };

    crate::e2e::template_env::render(
        "php/composer.json.jinja",
        minijinja::context! {
            e2e_pkg_name => e2e_pkg_name,
            e2e_autoload_ns => e2e_autoload_ns,
            require_section => require_section,
            autoload_section => autoload_section,
        },
    )
}

/// Render the `install.sh` script placed next to `composer.json` in registry mode.
///
/// The script bootstraps `php/pie` globally (if absent or older than 1.3.7),
/// runs `pie install <pkg>:<version>`, and verifies the extension binary loads.
/// The pinned version is baked in at generate time; callers run `bash install.sh`
/// with no arguments. The default `alef test-apps run` command for PHP invokes
/// this script before `composer install`.
/// Strip leading composer-style version constraints (^, >=, ~, etc.) from a version string.
/// Accepts "1.2.3", ">=1.2.3", "^1.2.3", "~1.2", or any constraint and returns the base version.
pub(super) fn strip_version_constraint(version: &str) -> &str {
    version.trim_start_matches(['^', '~', '>', '<', '='])
}

pub(super) fn render_install_sh(pkg_name: &str, extension_name: &str, pkg_version: &str) -> String {
    let clean_version = strip_version_constraint(pkg_version);
    format!(
        r#"#!/usr/bin/env bash
# alef-generated installer for registry-mode PHP test_app.
# Installs the {pkg_name} extension via PIE before `composer install` runs.
# Requires `php` on PATH; downloads and runs PIE if needed.
# Version is alef-injected at generate time so the script is self-contained.
set -euo pipefail

# Version override: pass as $1 to test an arbitrary tag; defaults to the
# alef-pinned version from `[crates.e2e.registry.packages.php].version`.
VERSION="${{1:-{clean_version}}}"

# PIE >= 1.3.7 supports the array-form `php-ext.download-url-method`
# our composer.json emits; 1.4.0+ is preferred. Download PIE if we don't
# already have a recent enough version.
need_pie_install=true
if command -v pie >/dev/null 2>&1; then
  current="$(pie --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo '0.0.0')"
  if printf '%s\n%s\n' "1.3.7" "$current" | sort -V -C; then
    need_pie_install=false
  fi
fi
if [[ "$need_pie_install" == "true" ]]; then
  # Download PIE PHAR from latest GitHub release if not already installed.
  pie_dir="${{HOME}}/.local/bin"
  mkdir -p "$pie_dir"
  curl -fL --output "$pie_dir/pie" "https://github.com/php/pie/releases/latest/download/pie.phar" 2>/dev/null || {{
    echo "::error::Failed to download PIE from GitHub; ensure network access or pre-install PIE." >&2
    exit 1
  }}
  chmod +x "$pie_dir/pie"
  PIE="$pie_dir/pie"
  # Ensure newly downloaded PIE is on PATH for this script.
  export PATH="$pie_dir:$PATH"
else
  PIE="pie"
fi

# Install the extension binary into the running PHP's extension dir.
"$PIE" install "{pkg_name}:$VERSION" --skip-enable-extension

# Verify the .so loads.
EXT_DIR="$(php -r 'echo ini_get("extension_dir");')"
test -f "$EXT_DIR/{extension_name}.so" || test -f "$EXT_DIR/{extension_name}.dylib" || test -f "$EXT_DIR/{extension_name}.dll"

# Export the installed extension path for downstream test runners (composer test).
# The test app's run_tests.php checks for PIE_INSTALLED_EXTENSION_PATH and loads the extension via `-d`.
export PIE_INSTALLED_EXTENSION_PATH="$EXT_DIR/{extension_name}.so"
if [[ "$OSTYPE" == "darwin"* ]]; then
  export PIE_INSTALLED_EXTENSION_PATH="$EXT_DIR/{extension_name}.dylib"
fi

# Verify the extension loads via explicit `-d` flag (same mechanism run_tests.php uses).
if ! php -d extension={extension_name} -m | grep -qi "{extension_name}"; then
  echo "::error::{extension_name} extension failed to load after PIE install" >&2
  exit 1
fi
echo "{extension_name} extension installed and loaded"
"#
    )
}

pub(super) fn render_phpunit_xml() -> String {
    crate::e2e::template_env::render("php/phpunit.xml.jinja", minijinja::context! {})
}

/// Render the app harness script for server-pattern HTTP fixtures.
///
/// The harness script spawns the SUT app and registers handlers per fixture,
/// returning canned expected responses. It's driven by bootstrap.php's subprocess
/// launcher.
pub(super) fn render_app_harness(e2e_config: &E2eConfig, groups: &[FixtureGroup], pkg_path: &str) -> String {
    use serde_json::json;

    // Collect all HTTP fixtures from all groups.
    let mut fixtures_map = serde_json::Map::new();

    for group in groups {
        for fixture in &group.fixtures {
            if fixture.http.is_none() {
                continue;
            }
            // Convert the fixture to JSON for the harness to load.
            // We only need the http field, handler, request, and expected_response.
            let http_data = fixture.http.as_ref().unwrap();
            let fixture_json = json!({
                "http": {
                    "handler": {
                        "route": &http_data.handler.route,
                        "method": &http_data.handler.method,
                        "body_schema": http_data.handler.body_schema.clone(),
                    },
                    "request": {
                        "path": &http_data.request.path,
                    },
                    "expected_response": {
                        "status_code": http_data.expected_response.status_code,
                        "body": &http_data.expected_response.body,
                        "headers": &http_data.expected_response.headers,
                    }
                }
            });
            fixtures_map.insert(fixture.id.clone(), fixture_json);
        }
    }

    let fixtures_json = serde_json::to_string(&fixtures_map).unwrap_or_default();

    let imports = &e2e_config.harness.imports;
    let app_class = e2e_config.harness.app_class_for_lang("php");
    // PHP wraps via ext-php-rs which historically emits snake_case method names
    // from the IR. `register_method_idiomatic` keeps snake_case for PHP so the
    // call site matches what the service-API codegen emits.
    let register_route_method = e2e_config
        .harness
        .register_method_idiomatic("php")
        .unwrap_or_else(|| "route".to_string());
    let body_schema_setter = &e2e_config.harness.body_schema_setter;
    let method_enum = &e2e_config.harness.method_enum;
    let run_method = e2e_config.harness.run_method_for_lang("php");
    let host = &e2e_config.harness.host;
    let port = e2e_config.harness.port;

    let header = hash::header(CommentStyle::DoubleSlash);

    // Derive route_builder_import from imports[0] → PHP namespace.
    // E.g. imports[0] = "my_pkg" → namespace MyPkg\Php
    let route_builder_import = if !imports.is_empty() {
        let module_name = &imports[0];
        // Normalize module name to PHP namespace (my_pkg → MyPkg, sample_core → SampleCore)
        module_name
            .split('_')
            .map(|p| {
                let mut chars = p.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join("\\")
            + "\\Php"
    } else {
        "App\\Php".to_string()
    };
    let method_enum_import = route_builder_import.clone();

    let ctx = minijinja::context! {
        header => header,
        imports => imports,
        app_class => app_class.as_deref().unwrap_or("App"),
        route_builder_import => route_builder_import,
        route_builder_class => "RouteBuilder",
        register_route_method => register_route_method.as_str(),
        route_builder_schema_setter => body_schema_setter.as_deref().unwrap_or("request_schema_json"),
        method_enum_import => method_enum_import,
        method_enum_class => method_enum.as_deref().unwrap_or("Method"),
        run_method => run_method.as_deref().unwrap_or("run"),
        response_body_field => e2e_config.harness.response_body_field.as_str(),
        host => host,
        port => port,
        pkg_path => pkg_path,
        fixtures_json => fixtures_json,
    };

    crate::e2e::template_env::render("php/app_harness.php.jinja", ctx)
}

pub(super) fn render_bootstrap(
    pkg_path: &str,
    has_mock_server_fixtures: bool,
    has_file_fixtures: bool,
    test_documents_path: &str,
    uses_server_harness: bool,
    harness_host: &str,
    harness_port: u16,
) -> String {
    let header = hash::header(CommentStyle::DoubleSlash);
    crate::e2e::template_env::render(
        "php/bootstrap.php.jinja",
        minijinja::context! {
            header => header,
            pkg_path => pkg_path,
            has_mock_server_fixtures => has_mock_server_fixtures,
            has_file_fixtures => has_file_fixtures,
            test_documents_path => test_documents_path,
            uses_server_harness => uses_server_harness,
            harness_host => harness_host,
            harness_port => harness_port,
        },
    )
}

pub(super) fn render_run_tests_php(extension_name: &str, cargo_crate_name: Option<&str>) -> String {
    let header = hash::header(CommentStyle::DoubleSlash);
    let ext_lib_name = if let Some(crate_name) = cargo_crate_name {
        // Cargo replaces hyphens with underscores for lib names, and the crate name
        // already includes the _php suffix.
        format!("lib{}", crate_name.replace('-', "_"))
    } else {
        format!("lib{extension_name}_php")
    };
    format!(
        r#"#!/usr/bin/env php
<?php
{header}
declare(strict_types=1);

// Determine platform-specific extension suffix.
$extSuffix = match (PHP_OS_FAMILY) {{
    'Darwin' => '.dylib',
    default => '.so',
}};
$extPath = __DIR__ . '/../../target/release/{ext_lib_name}' . $extSuffix;

// Check for PIE-installed extension path (set by install.sh in registry mode).
// In registry mode, the extension is installed system-wide via PIE and passed
// via the PIE_INSTALLED_EXTENSION_PATH environment variable.
$pieInstalledExtPath = getenv('PIE_INSTALLED_EXTENSION_PATH');
if ($pieInstalledExtPath && file_exists($pieInstalledExtPath)) {{
    $extPath = $pieInstalledExtPath;
}}

// If the extension exists (locally-built or PIE-installed) and we have not already
// restarted with it, re-exec PHP with the extension loaded explicitly via `-d extension=`.
// The system php.ini is kept (no `-n`) so PHPUnit's required extensions — dom,
// json, libxml, mbstring, tokenizer, xml, xmlwriter — remain available. `-n`
// drops every shared module, which breaks PHPUnit on distributions that ship those
// as shared extensions (e.g. Debian/Ubuntu); they only survive `-n` where
// compiled statically.
if (file_exists($extPath) && !getenv('ALEF_PHP_EXT_LOADED')) {{
    putenv('ALEF_PHP_EXT_LOADED=1');
    $php = PHP_BINARY;
    $phpunitPath = __DIR__ . '/vendor/bin/phpunit';

    $cmd = array_merge(
        [$php, '-d', 'extension=' . $extPath],
        [$phpunitPath],
        array_slice($GLOBALS['argv'], 1)
    );

    passthru(implode(' ', array_map('escapeshellarg', $cmd)), $exitCode);
    exit($exitCode);
}}

// Extension is now loaded (via the restart above).
// Invoke PHPUnit normally.
$phpunitPath = __DIR__ . '/vendor/bin/phpunit';
if (!file_exists($phpunitPath)) {{
    echo "PHPUnit not found at $phpunitPath. Run 'composer install' first.\\n";
    exit(1);
}}

require $phpunitPath;
"#
    )
}
