use crate::{cargo_package_header, core_dep_features, detect_workspace_inheritance, render_extra_deps, scaffold_meta};
use alef_core::backend::GeneratedFile;
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

pub(crate) fn scaffold_wasm(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();
    let ws = detect_workspace_inheritance(config.crate_config.workspace_root.as_deref());
    let pkg_header = cargo_package_header(
        &format!("{core_crate_dir}-wasm"),
        version,
        "2024",
        &meta.license,
        &meta.description,
        &meta.keywords,
        &ws,
    );

    let extra_deps = render_extra_deps(config, Language::Wasm);
    let extra_deps_section = if extra_deps.is_empty() {
        String::new()
    } else {
        format!("\n{extra_deps}")
    };

    let content = format!(
        r#"{pkg_header}
repository = "{repository}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
js-sys = "0.3"
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
serde-wasm-bindgen = "0.6"
serde_json = "1"{extra_deps_section}

[package.metadata.wasm-pack.profile.release]
wasm-opt = false

[package.metadata.cargo-machete]
ignored = ["wasm-bindgen-futures"]
"#,
        pkg_header = pkg_header,
        repository = meta.repository,
        crate_name = &config.crate_config.name,
        core_crate_dir = core_crate_dir,
        features = core_dep_features(config, Language::Wasm),
        extra_deps_section = extra_deps_section,
    );

    let mut files = vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-wasm/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }];

    // Generate package.json for npm publishing.
    // Uses the node package name with -wasm suffix for the npm scope.
    let node_pkg = config.node_package_name();
    let wasm_pkg_name = format!("{node_pkg}-wasm");
    let pkg_json = format!(
        r#"{{
  "name": "{wasm_pkg_name}",
  "version": "{version}",
  "private": false,
  "description": "{description}",
  "license": "{license}",
  "repository": {{
    "type": "git",
    "url": "{repository}",
    "directory": "crates/{core_crate_dir}-wasm"
  }},
  "type": "module",
  "files": [
    "pkg",
    "*.wasm",
    "*.d.ts",
    "README.md"
  ],
  "main": "pkg/nodejs/{core_crate_dir}_wasm.js",
  "module": "pkg/web/{core_crate_dir}_wasm.js",
  "types": "pkg/nodejs/{core_crate_dir}_wasm.d.ts",
  "scripts": {{
    "build": "wasm-pack build --target nodejs --out-dir pkg/nodejs",
    "build:ci": "wasm-pack build --release --target nodejs --out-dir pkg/nodejs",
    "build:wasm:web": "wasm-pack build --release --target web --out-dir pkg/web",
    "build:wasm:bundler": "wasm-pack build --release --target bundler --out-dir pkg/bundler",
    "build:wasm:nodejs": "wasm-pack build --release --target nodejs --out-dir pkg/nodejs",
    "build:wasm:deno": "wasm-pack build --release --target deno --out-dir pkg/deno",
    "build:all": "npm run build:wasm:web && npm run build:wasm:bundler && npm run build:wasm:nodejs && npm run build:wasm:deno && find pkg -name .gitignore -delete",
    "test": "vitest run",
    "test:watch": "vitest watch",
    "test:coverage": "vitest run --coverage",
    "clean": "rm -rf pkg dist"
  }}
}}
"#,
        wasm_pkg_name = wasm_pkg_name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        core_crate_dir = core_crate_dir,
    );

    files.push(GeneratedFile {
        path: PathBuf::from(format!("crates/{}-wasm/package.json", core_crate_dir)),
        content: pkg_json,
        generated_header: false,
    });

    Ok(files)
}
