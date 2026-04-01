//! Package scaffolding generator for skif.

use skif_core::backend::GeneratedFile;
use skif_core::config::{Language, SkifConfig};
use skif_core::ir::ApiSurface;
use std::path::PathBuf;

/// Generate package scaffolding files for the given languages.
pub fn scaffold(api: &ApiSurface, config: &SkifConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    let mut files = vec![];
    for &lang in languages {
        files.extend(scaffold_language(api, config, lang)?);
    }
    Ok(files)
}

fn scaffold_language(api: &ApiSurface, config: &SkifConfig, lang: Language) -> anyhow::Result<Vec<GeneratedFile>> {
    match lang {
        Language::Python => scaffold_python(api, config),
        Language::Node => scaffold_node(api, config),
        Language::Ffi => scaffold_ffi(api, config),
        Language::Go => scaffold_go(api, config),
        Language::Java => scaffold_java(api, config),
        Language::Csharp => scaffold_csharp(api, config),
        Language::Ruby => scaffold_ruby(api, config),
        Language::Php => scaffold_php(api, config),
        Language::Elixir => scaffold_elixir(api, config),
        Language::Wasm => scaffold_wasm(api, config),
    }
}

/// Helper to get scaffold metadata with defaults.
struct ScaffoldMeta {
    description: String,
    license: String,
    repository: String,
    #[allow(dead_code)]
    homepage: String,
    authors: Vec<String>,
    keywords: Vec<String>,
}

fn scaffold_meta(config: &SkifConfig) -> ScaffoldMeta {
    let scaffold = config.scaffold.as_ref();
    ScaffoldMeta {
        description: scaffold
            .and_then(|s| s.description.clone())
            .unwrap_or_else(|| format!("Bindings for {}", config.crate_config.name)),
        license: scaffold
            .and_then(|s| s.license.clone())
            .unwrap_or_else(|| "MIT".to_string()),
        repository: scaffold
            .and_then(|s| s.repository.clone())
            .unwrap_or_else(|| format!("https://github.com/kreuzberg-dev/{}", config.crate_config.name)),
        homepage: scaffold.and_then(|s| s.homepage.clone()).unwrap_or_default(),
        authors: scaffold.map(|s| s.authors.clone()).unwrap_or_default(),
        keywords: scaffold.map(|s| s.keywords.clone()).unwrap_or_default(),
    }
}

fn scaffold_python(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let name = &config.crate_config.name;
    let version = &api.version;
    let module_name = config.python_module_name();

    let authors_toml = if meta.authors.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = meta
            .authors
            .iter()
            .map(|a| format!("    {{ name = \"{}\" }}", a))
            .collect();
        format!("authors = [\n{}\n]\n", entries.join(",\n"))
    };

    let keywords_toml = if meta.keywords.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = meta.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
        format!("keywords = [{}]\n", entries.join(", "))
    };

    let content = format!(
        r#"[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "{name}"
version = "{version}"
description = "{description}"
license = "{license}"
requires-python = ">=3.9"
{authors}{keywords}
[tool.maturin]
module-name = "{module_name}"
features = ["pyo3/extension-module"]
"#,
        name = name,
        version = version,
        description = meta.description,
        license = meta.license,
        authors = authors_toml,
        keywords = keywords_toml,
        module_name = module_name,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/python/pyproject.toml"),
        content,
        generated_header: true,
    }])
}

fn scaffold_node(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let package_name = config.node_package_name();
    let name = &config.crate_config.name;
    let version = &api.version;

    let keywords_json = if meta.keywords.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = meta.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
        format!(",\n  \"keywords\": [{}]", entries.join(", "))
    };

    let content = format!(
        r#"{{
  "name": "{package_name}",
  "version": "{version}",
  "description": "{description}",
  "license": "{license}",
  "main": "index.js",
  "types": "index.d.ts",
  "repository": "{repository}",
  "napi": {{
    "name": "{name}"
  }}{keywords}
}}
"#,
        package_name = package_name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        name = name,
        keywords = keywords_json,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/typescript/package.json"),
        content,
        generated_header: false,
    }])
}

fn scaffold_ruby(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let gem_name = config.ruby_gem_name();
    let version = &api.version;

    let authors_ruby = if meta.authors.is_empty() {
        "[]".to_string()
    } else {
        let entries: Vec<String> = meta.authors.iter().map(|a| format!("\"{}\"", a)).collect();
        format!("[{}]", entries.join(", "))
    };

    let content = format!(
        r#"Gem::Specification.new do |spec|
  spec.name          = "{gem_name}"
  spec.version       = "{version}"
  spec.authors       = {authors}
  spec.summary       = "{description}"
  spec.description   = "{description}"
  spec.homepage      = "{repository}"
  spec.license       = "{license}"

  spec.files         = Dir["lib/**/*", "ext/**/*"]
  spec.require_paths = ["lib"]

  spec.extensions    = ["ext/{gem_name}/extconf.rb"]
end
"#,
        gem_name = gem_name,
        version = version,
        authors = authors_ruby,
        description = meta.description,
        repository = meta.repository,
        license = meta.license,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("packages/ruby/{}.gemspec", gem_name)),
        content,
        generated_header: true,
    }])
}

fn scaffold_php(_api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let ext_name = config.php_extension_name();
    let name = &config.crate_config.name;

    let keywords_json = if meta.keywords.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = meta.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
        format!(",\n  \"keywords\": [{}]", entries.join(", "))
    };

    let content = format!(
        r#"{{
  "name": "kreuzberg-dev/{name}",
  "description": "{description}",
  "license": "{license}",
  "type": "php-ext",
  "require": {{
    "php": ">=8.1"
  }},
  "extra": {{
    "ext-name": "{ext_name}"
  }}{keywords}
}}
"#,
        name = name,
        description = meta.description,
        license = meta.license,
        ext_name = ext_name,
        keywords = keywords_json,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/php/composer.json"),
        content,
        generated_header: false,
    }])
}

fn scaffold_elixir(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let app_name = config.elixir_app_name();
    let version = &api.version;

    let content = format!(
        r#"defmodule {module}.MixProject do
  use Mix.Project

  def project do
    [
      app: :{app_name},
      version: "{version}",
      elixir: "~> 1.14",
      description: "{description}",
      package: package(),
      deps: deps()
    ]
  end

  defp package do
    [
      licenses: ["{license}"],
      links: %{{"GitHub" => "{repository}"}}
    ]
  end

  defp deps do
    [
      {{:rustler, "~> 0.34"}}
    ]
  end
end
"#,
        module = capitalize_first(&app_name),
        app_name = app_name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/elixir/mix.exs"),
        content,
        generated_header: true,
    }])
}

fn scaffold_go(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let go_module = config.go_module();
    let version = &api.version;
    let _ = version; // go.mod doesn't embed the package version

    let content = format!("module {module}\n\ngo 1.21\n", module = go_module,);

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/go/go.mod"),
        content,
        generated_header: false,
    }])
}

fn scaffold_java(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let java_package = config.java_package();
    let name = &config.crate_config.name;
    let version = &api.version;

    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>{package}</groupId>
    <artifactId>{name}</artifactId>
    <version>{version}</version>
    <packaging>jar</packaging>

    <name>{name}</name>
    <description>{description}</description>
    <url>{repository}</url>

    <licenses>
        <license>
            <name>{license}</name>
        </license>
    </licenses>

    <properties>
        <maven.compiler.source>21</maven.compiler.source>
        <maven.compiler.target>21</maven.compiler.target>
    </properties>
</project>
"#,
        package = java_package,
        name = name,
        version = version,
        description = meta.description,
        repository = meta.repository,
        license = meta.license,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/java/pom.xml"),
        content,
        generated_header: true,
    }])
}

fn scaffold_csharp(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let namespace = config.csharp_namespace();
    let version = &api.version;

    let target_framework = config
        .csharp
        .as_ref()
        .and_then(|c| c.target_framework.clone())
        .unwrap_or_else(|| "net8.0".to_string());

    let authors_csproj = if meta.authors.is_empty() {
        String::new()
    } else {
        format!("    <Authors>{}</Authors>\n", meta.authors.join(";"))
    };

    let content = format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>{target_framework}</TargetFramework>
    <RootNamespace>{namespace}</RootNamespace>
    <PackageId>{namespace}</PackageId>
    <Version>{version}</Version>
    <Description>{description}</Description>
    <PackageLicenseExpression>{license}</PackageLicenseExpression>
    <RepositoryUrl>{repository}</RepositoryUrl>
{authors}    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
"#,
        target_framework = target_framework,
        namespace = namespace,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        authors = authors_csproj,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("packages/csharp/{}.csproj", namespace)),
        content,
        generated_header: true,
    }])
}

fn scaffold_ffi(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let name = &config.crate_config.name;
    let version = &api.version;

    let content = format!(
        r#"[package]
name = "{name}-ffi"
version = "{version}"
edition = "2024"
description = "{description}"
license = "{license}"
repository = "{repository}"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
{name} = {{ path = "../.." }}
"#,
        name = name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-ffi/Cargo.toml", name)),
        content,
        generated_header: true,
    }])
}

fn scaffold_wasm(api: &ApiSurface, config: &SkifConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let name = &config.crate_config.name;
    let version = &api.version;

    let content = format!(
        r#"[package]
name = "{name}-wasm"
version = "{version}"
edition = "2024"
description = "{description}"
license = "{license}"
repository = "{repository}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{name} = {{ path = "../.." }}
wasm-bindgen = "0.2"
"#,
        name = name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-wasm/Cargo.toml", name)),
        content,
        generated_header: true,
    }])
}

/// Capitalize the first character of a string (for Elixir module names).
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skif_core::config::*;

    fn test_config() -> SkifConfig {
        SkifConfig {
            crate_config: CrateConfig {
                name: "my-lib".to_string(),
                sources: vec![],
                version_from: "Cargo.toml".to_string(),
            },
            languages: vec![Language::Python, Language::Node],
            exclude: ExcludeConfig::default(),
            output: OutputConfig::default(),
            python: None,
            node: None,
            ruby: None,
            php: None,
            elixir: None,
            wasm: None,
            ffi: None,
            go: None,
            java: None,
            csharp: None,
            scaffold: Some(ScaffoldConfig {
                description: Some("Test library".to_string()),
                license: Some("MIT".to_string()),
                repository: Some("https://github.com/test/my-lib".to_string()),
                homepage: None,
                authors: vec!["Alice".to_string()],
                keywords: vec!["test".to_string()],
            }),
            readme: None,
            lint: None,
            custom_files: None,
        }
    }

    fn test_api() -> ApiSurface {
        ApiSurface {
            crate_name: "my-lib".to_string(),
            version: "0.1.0".to_string(),
            types: vec![],
            functions: vec![],
            enums: vec![],
            errors: vec![],
        }
    }

    #[test]
    fn test_scaffold_python() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Python]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("packages/python/pyproject.toml"));
        assert!(files[0].content.contains("maturin"));
        assert!(files[0].content.contains("my-lib"));
    }

    #[test]
    fn test_scaffold_node() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Node]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("packages/typescript/package.json"));
        assert!(files[0].content.contains("napi"));
    }

    #[test]
    fn test_scaffold_multiple() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Python, Language::Node]).unwrap();
        assert_eq!(files.len(), 2);
    }
}
