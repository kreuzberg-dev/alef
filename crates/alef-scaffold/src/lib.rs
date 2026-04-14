//! Package scaffolding generator for alef.

use alef_core::backend::GeneratedFile;
use alef_core::config::{AlefConfig, Language};
use alef_core::ir::ApiSurface;
use std::path::PathBuf;

/// Format the features clause for the core crate dependency in generated Cargo.toml files.
///
/// Checks for per-language feature overrides first, then falls back to `[crate] features`.
/// Returns an empty string if no features are configured, otherwise returns
/// `, features = ["feat1", "feat2"]`.
fn core_dep_features(config: &AlefConfig, lang: Language) -> String {
    let features = config.features_for_language(lang);
    if features.is_empty() {
        String::new()
    } else {
        let quoted: Vec<String> = features.iter().map(|f| format!("\"{f}\"")).collect();
        format!(", features = [{}]", quoted.join(", "))
    }
}

/// Generate package scaffolding files for the given languages.
pub fn scaffold(api: &ApiSurface, config: &AlefConfig, languages: &[Language]) -> anyhow::Result<Vec<GeneratedFile>> {
    let mut files = vec![];
    for &lang in languages {
        files.extend(scaffold_language(api, config, lang)?);
    }
    Ok(files)
}

fn scaffold_language(api: &ApiSurface, config: &AlefConfig, lang: Language) -> anyhow::Result<Vec<GeneratedFile>> {
    match lang {
        Language::Python => {
            let mut files = scaffold_python(api, config)?;
            files.extend(scaffold_python_cargo(api, config)?);
            Ok(files)
        }
        Language::Node => {
            let mut files = scaffold_node(api, config)?;
            files.extend(scaffold_node_cargo(api, config)?);
            Ok(files)
        }
        Language::Ffi => scaffold_ffi(api, config),
        Language::Go => scaffold_go(api, config),
        Language::Java => scaffold_java(api, config),
        Language::Csharp => scaffold_csharp(api, config),
        Language::Ruby => {
            let mut files = scaffold_ruby(api, config)?;
            files.extend(scaffold_ruby_cargo(api, config)?);
            Ok(files)
        }
        Language::Php => {
            let mut files = scaffold_php(api, config)?;
            files.extend(scaffold_php_cargo(api, config)?);
            Ok(files)
        }
        Language::Elixir => {
            let mut files = scaffold_elixir(api, config)?;
            files.extend(scaffold_elixir_cargo(api, config)?);
            Ok(files)
        }
        Language::Wasm => scaffold_wasm(api, config),
        Language::R => {
            let mut files = scaffold_r(api, config)?;
            files.extend(scaffold_r_cargo(api, config)?);
            Ok(files)
        }
    }
}

/// Helper to get scaffold metadata with defaults.
struct ScaffoldMeta {
    description: String,
    license: String,
    repository: String,
    homepage: String,
    authors: Vec<String>,
    keywords: Vec<String>,
}

fn scaffold_meta(config: &AlefConfig) -> ScaffoldMeta {
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

fn scaffold_python_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let module_name = config.python_module_name();
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-py"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
name = "{module_name}"
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
pyo3 = {{ version = "0.28", features = ["extension-module"] }}
pyo3-async-runtimes = {{ version = "0.28", features = ["tokio-runtime"] }}
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        license = meta.license,
        module_name = module_name,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Python),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-py/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }])
}

fn scaffold_python(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let name = &config.crate_config.name;
    let version = &api.version;
    let module_name = config.python_module_name();
    let core_crate_dir = config.core_crate_dir();

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

    let homepage_toml = if meta.homepage.is_empty() {
        String::new()
    } else {
        format!("homepage = \"{}\"\n", meta.homepage)
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
requires-python = ">=3.10"
{authors}{keywords}{homepage}[project.urls]
repository = "{repository}"

[tool.maturin]
module-name = "{name}.{module_name}"
manifest-path = "../../crates/{crate_dir}-py/Cargo.toml"
features = ["pyo3/extension-module"]

[tool.ruff]
line-length = 100
target-version = "py39"

[tool.ruff.lint]
select = ["E", "F", "W"]
"#,
        name = name,
        version = version,
        description = meta.description,
        license = meta.license,
        authors = authors_toml,
        keywords = keywords_toml,
        homepage = homepage_toml,
        repository = meta.repository,
        module_name = module_name,
        crate_dir = core_crate_dir,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/python/pyproject.toml"),
        content,
        generated_header: true,
    }])
}

fn scaffold_node_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-node"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
napi = {{ version = "3", features = ["async"] }}
napi-derive = "3"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"

[build-dependencies]
napi-build = "2"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        license = meta.license,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Node),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-node/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }])
}

fn scaffold_node(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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

    let homepage_json = if meta.homepage.is_empty() {
        String::new()
    } else {
        format!(",\n  \"homepage\": \"{}\"", meta.homepage)
    };

    let authors_json = if meta.authors.is_empty() {
        String::new()
    } else {
        format!(",\n  \"author\": \"{}\"", meta.authors.join(", "))
    };

    let content = format!(
        r#"{{
  "name": "{package_name}",
  "version": "{version}",
  "description": "{description}",
  "license": "{license}",
  "main": "index.js",
  "types": "index.d.ts",
  "repository": "{repository}"{homepage}{authors}{keywords},
  "files": [
    "index.js",
    "index.d.ts",
    "**/*.node"
  ],
  "scripts": {{
    "build": "napi build --release",
    "build:debug": "napi build",
    "test": "node -e \"console.log('Add test command')\""
  }},
  "napi": {{
    "name": "{name}",
    "triples": [
      "x86_64-unknown-linux-gnu",
      "x86_64-apple-darwin",
      "aarch64-apple-darwin",
      "x86_64-pc-windows-msvc"
    ]
  }},
  "devDependencies": {{
    "@napi-rs/cli": "^3.0.0"
  }}
}}
"#,
        package_name = package_name,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        homepage = homepage_json,
        authors = authors_json,
        keywords = keywords_json,
        name = name,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/typescript/package.json"),
        content,
        generated_header: false,
    }])
}

fn scaffold_ruby_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-rb"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../../../../crates/{core_crate_dir}"{features} }}
magnus = "0.8"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
serde_magnus = "0.11"
tokio = {{ version = "1", features = ["full"] }}
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        license = meta.license,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Ruby),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("packages/ruby/ext/{}_rb/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }])
}

fn scaffold_ruby(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let gem_name = config.ruby_gem_name();
    let core_crate_dir = config.core_crate_dir();
    // The native extension name uses the core crate dir with underscores and _rb suffix,
    // matching the directory generated by scaffold_ruby_cargo: ext/{core_crate_dir}_rb/
    let ext_name = format!("{}_rb", core_crate_dir.replace('-', "_"));
    let version = &api.version;

    let authors_ruby = if meta.authors.is_empty() {
        "[]".to_string()
    } else {
        let entries: Vec<String> = meta.authors.iter().map(|a| format!("\"{}\"", a)).collect();
        format!("[{}]", entries.join(", "))
    };

    let metadata_ruby = if meta.keywords.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = meta.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
        format!("  spec.metadata[\"keywords\"] = [{}].join(\",\")\n", entries.join(", "))
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
  spec.required_ruby_version = ">= 3.2.0"
{metadata}
  spec.files         = Dir.glob(["lib/**/*", "ext/**/*"])
  spec.require_paths = ["lib"]
  spec.extensions    = ["ext/{ext_name}/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9"
end
"#,
        gem_name = gem_name,
        ext_name = ext_name,
        version = version,
        authors = authors_ruby,
        description = meta.description,
        repository = meta.repository,
        license = meta.license,
        metadata = metadata_ruby,
    );

    let rubocop_content = r#"plugins:
  - rubocop-performance
  - rubocop-rspec

AllCops:
  TargetRubyVersion: 3.2
  NewCops: enable
  SuggestExtensions: false
  Exclude:
    - 'vendor/**/*'
    - 'tmp/**/*'
    - 'lib/**/*.bundle'
    - 'ext/**/*'

Style/FrozenStringLiteralComment:
  Enabled: true
  EnforcedStyle: always

Style/StringLiterals:
  Enabled: true
  EnforcedStyle: single_quotes

Style/StringLiteralsInInterpolation:
  Enabled: true
  EnforcedStyle: single_quotes

Style/Documentation:
  Enabled: false

Layout/LineLength:
  Max: 120
  AllowedPatterns:
    - '\A\s*#'
  Exclude:
    - 'spec/**/*'

Metrics/MethodLength:
  Max: 20
  Exclude:
    - 'spec/**/*'

Metrics/BlockLength:
  Enabled: true
  Max: 350
  CountComments: false

Metrics/AbcSize:
  Max: 20
  Exclude:
    - 'spec/**/*'

RSpec/ExampleLength:
  Max: 50

RSpec/MultipleExpectations:
  Max: 25

RSpec/NestedGroups:
  Max: 6
"#
    .to_string();

    let rakefile_content = format!(
        r#"# frozen_string_literal: true

require 'bundler/gem_tasks'
require 'rake/extensiontask'
require 'rspec/core/rake_task'

GEMSPEC = Gem::Specification.load(File.expand_path('{gem_name}.gemspec', __dir__))

Rake::ExtensionTask.new('{ext_name}', GEMSPEC) do |ext|
  ext.lib_dir = 'lib'
  ext.ext_dir = 'ext/{ext_name}'
  ext.cross_compile = true
  ext.cross_platform = %w[
    x86_64-linux
    aarch64-linux
    x86_64-darwin
    arm64-darwin
    x64-mingw32
    x64-mingw-ucrt
  ]
end

RSpec::Core::RakeTask.new(:spec)

task spec: :compile
task default: :spec
"#,
        gem_name = gem_name,
        ext_name = ext_name,
    );

    let lib_content = format!(
        r#"# frozen_string_literal: true

require '{ext_name}'
"#,
        ext_name = ext_name,
    );

    let extconf_content = format!(
        r#"# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

default_profile = ENV.fetch('CARGO_PROFILE', 'release')

create_rust_makefile('{ext_name}') do |config|
  config.profile = default_profile.to_sym
end
"#,
        ext_name = ext_name,
    );

    Ok(vec![
        GeneratedFile {
            path: PathBuf::from(format!("packages/ruby/{}.gemspec", gem_name)),
            content,
            generated_header: true,
        },
        GeneratedFile {
            path: PathBuf::from("packages/ruby/.rubocop.yml"),
            content: rubocop_content,
            generated_header: true,
        },
        GeneratedFile {
            path: PathBuf::from("packages/ruby/Rakefile"),
            content: rakefile_content,
            generated_header: true,
        },
        GeneratedFile {
            path: PathBuf::from(format!("packages/ruby/lib/{}.rb", gem_name)),
            content: lib_content,
            generated_header: true,
        },
        GeneratedFile {
            path: PathBuf::from(format!("packages/ruby/ext/{ext_name}/extconf.rb", ext_name = ext_name)),
            content: extconf_content,
            generated_header: true,
        },
    ])
}

fn scaffold_php_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-php"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
ext-php-rs = "0.15"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        license = meta.license,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Php),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-php/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }])
}

fn scaffold_php(_api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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
    "php": ">=8.2"
  }},
  "require-dev": {{
    "phpstan/phpstan": "^2.1",
    "friendsofphp/php-cs-fixer": "^3.95",
    "phpunit/phpunit": "^11.0"
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

fn scaffold_elixir_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let app_name = config.elixir_app_name();
    let nif_name = format!("{app_name}_nif");
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{nif_name}"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../../../../crates/{core_crate_dir}"{features} }}
rustler = "0.37"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tokio = {{ version = "1", features = ["full"] }}
"#,
        nif_name = nif_name,
        version = version,
        license = meta.license,
        crate_name = &config.crate_config.name,
        core_crate_dir = core_crate_dir,
        features = core_dep_features(config, Language::Elixir),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("packages/elixir/native/{nif_name}/Cargo.toml")),
        content,
        generated_header: true,
    }])
}

fn scaffold_elixir(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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
      {{:rustler, "~> 0.34"}},
      {{:credo, "~> 1.7", only: [:dev, :test], runtime: false}}
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

fn scaffold_go(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let go_module = config.go_module();
    let version = &api.version;
    let _ = version; // go.mod doesn't embed the package version

    let content = format!(
        r#"module {module}

go 1.21

require (
)
"#,
        module = go_module,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/go/go.mod"),
        content,
        generated_header: false,
    }])
}

fn scaffold_java(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
        <maven.compiler.source>21</maven.compiler.source>
        <maven.compiler.target>21</maven.compiler.target>
    </properties>

    <dependencies>
        <dependency>
            <groupId>com.fasterxml.jackson.core</groupId>
            <artifactId>jackson-databind</artifactId>
            <version>2.18.2</version>
        </dependency>
        <dependency>
            <groupId>com.fasterxml.jackson.datatype</groupId>
            <artifactId>jackson-datatype-jdk8</artifactId>
            <version>2.18.2</version>
        </dependency>
        <dependency>
            <groupId>org.junit.jupiter</groupId>
            <artifactId>junit-jupiter</artifactId>
            <version>5.11.4</version>
            <scope>test</scope>
        </dependency>
    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-compiler-plugin</artifactId>
                <version>3.11.0</version>
                <configuration>
                    <source>21</source>
                    <target>21</target>
                </configuration>
            </plugin>
            <plugin>
                <groupId>org.apache.maven.plugins</groupId>
                <artifactId>maven-surefire-plugin</artifactId>
                <version>3.2.5</version>
                <configuration>
                    <argLine>--enable-native-access=ALL-UNNAMED -Djava.library.path=${{project.basedir}}/../../target/release</argLine>
                </configuration>
            </plugin>
            <plugin>
                <groupId>com.diffplug.spotless</groupId>
                <artifactId>spotless-maven-plugin</artifactId>
                <version>3.4.0</version>
                <configuration>
                    <java>
                        <googleJavaFormat>
                            <version>1.35.0</version>
                        </googleJavaFormat>
                    </java>
                </configuration>
                <executions>
                    <execution>
                        <goals>
                            <goal>apply</goal>
                        </goals>
                        <phase>process-sources</phase>
                    </execution>
                </executions>
            </plugin>
        </plugins>
    </build>
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

fn scaffold_csharp(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
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

fn scaffold_ffi(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-ffi"
version = "{version}"
edition = "2021"
description = "{description}"
license = "{license}"
repository = "{repository}"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tokio = {{ version = "1", features = ["full"] }}

[build-dependencies]
cbindgen = "0.28"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Ffi),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-ffi/Cargo.toml", core_crate_dir)),
        content,
        generated_header: true,
    }])
}

fn scaffold_wasm(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-wasm"
version = "{version}"
edition = "2024"
description = "{description}"
license = "{license}"
repository = "{repository}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
wasm-bindgen = "0.2"
serde-wasm-bindgen = "0.6"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        description = meta.description,
        license = meta.license,
        repository = meta.repository,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::Wasm),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from(format!("crates/{}-wasm/Cargo.toml", core_crate_dir)),
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

fn scaffold_r(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let package_name = config.r_package_name();

    let mut description = meta.description.clone();
    if description.ends_with('.') {
        description.pop();
    }

    let authors_r = if meta.authors.is_empty() {
        r#"Authors@R: person("Author", "Name", email = "author@example.com", role = c("aut", "cre"))"#.to_string()
    } else {
        format!(
            "Authors@R: person(\"{}\", email = \"author@example.com\", role = c(\"aut\", \"cre\"))",
            meta.authors.first().unwrap_or(&"Author Name".to_string())
        )
    };

    let content = format!(
        r#"Package: {package}
Title: {title}
Version: {version}
{authors}
Description: {description}
    Rust bindings generated with extendr.
URL: {repository}
BugReports: {repository}/issues
License: {license}
Depends: R (>= 4.2)
Imports: jsonlite
Suggests:
    testthat (>= 3.0.0),
    withr,
    roxygen2
SystemRequirements: Cargo (Rust's package manager), rustc (>= 1.91)
Config/rextendr/version: 0.4.2
Encoding: UTF-8
Roxygen: list(markdown = TRUE)
RoxygenNote: 7.3.3
Config/testthat/edition: 3
"#,
        package = package_name,
        title = meta.description,
        version = version,
        authors = authors_r,
        description = description,
        repository = meta.repository,
        license = meta.license,
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/r/DESCRIPTION"),
        content,
        generated_header: true,
    }])
}

fn scaffold_r_cargo(api: &ApiSurface, config: &AlefConfig) -> anyhow::Result<Vec<GeneratedFile>> {
    let meta = scaffold_meta(config);
    let version = &api.version;
    let core_crate_dir = config.core_crate_dir();

    let content = format!(
        r#"[package]
name = "{core_crate_dir}-r"
version = "{version}"
edition = "2024"
license = "{license}"

[lib]
crate-type = ["cdylib"]

[dependencies]
{crate_name} = {{ path = "../{core_crate_dir}"{features} }}
extendr-api = {{ version = "0.7", features = ["use-precompiled-bindings"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
"#,
        core_crate_dir = core_crate_dir,
        version = version,
        license = meta.license,
        crate_name = &config.crate_config.name,
        features = core_dep_features(config, Language::R),
    );

    Ok(vec![GeneratedFile {
        path: PathBuf::from("packages/r/src/rust/Cargo.toml".to_string()),
        content,
        generated_header: true,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::config::*;

    fn test_config() -> AlefConfig {
        AlefConfig {
            crate_config: CrateConfig {
                name: "my-lib".to_string(),
                sources: vec![],
                version_from: "Cargo.toml".to_string(),
                core_import: None,
                workspace_root: None,
                skip_core_import: false,
                features: vec![],
                path_mappings: std::collections::HashMap::new(),
            },
            languages: vec![Language::Python, Language::Node],
            exclude: ExcludeConfig::default(),
            include: IncludeConfig::default(),
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
            r: None,
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
            adapters: vec![],
            custom_modules: CustomModulesConfig::default(),
            custom_registrations: CustomRegistrationsConfig::default(),
            opaque_types: std::collections::HashMap::new(),
            generate: GenerateConfig::default(),
            generate_overrides: std::collections::HashMap::new(),
            dto: Default::default(),
            sync: None,
            test: None,
            e2e: None,
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
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("packages/python/pyproject.toml"));
        assert!(files[0].content.contains("maturin"));
        assert!(files[0].content.contains("my-lib"));
        assert_eq!(files[1].path, PathBuf::from("crates/my-lib-py/Cargo.toml"));
        assert!(files[1].content.contains("pyo3"));
    }

    #[test]
    fn test_scaffold_node() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Node]).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("packages/typescript/package.json"));
        assert!(files[0].content.contains("napi"));
        assert_eq!(files[1].path, PathBuf::from("crates/my-lib-node/Cargo.toml"));
        assert!(files[1].content.contains("napi-derive"));
    }

    #[test]
    fn test_scaffold_multiple() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Python, Language::Node]).unwrap();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn test_scaffold_python_production_features() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Python]).unwrap();
        let content = &files[0].content;
        assert!(content.contains("[project.urls]"));
        assert!(content.contains("repository ="));
        assert!(content.contains("[tool.ruff]"));
        assert!(content.contains("line-length = 100"));
        assert!(content.contains("target-version = \"py39\""));
    }

    #[test]
    fn test_scaffold_node_production_features() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Node]).unwrap();
        let content = &files[0].content;
        assert!(content.contains("\"scripts\""));
        assert!(content.contains("\"build\""));
        assert!(content.contains("\"files\""));
        assert!(content.contains("\"devDependencies\""));
        assert!(content.contains("@napi-rs/cli"));
        assert!(content.contains("\"triples\""));
    }

    #[test]
    fn test_scaffold_ffi_with_core_import() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Ffi]).unwrap();
        assert_eq!(files.len(), 1);
        let content = &files[0].content;
        assert!(content.contains("serde"));
        assert!(content.contains("serde_json"));
        // Should have core_import as dependency
        assert!(content.contains("my-lib ="));
    }

    #[test]
    fn test_scaffold_go_production_format() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Go]).unwrap();
        assert_eq!(files.len(), 1);
        let content = &files[0].content;
        assert!(content.contains("go 1.21"));
        assert!(content.contains("require ("));
    }

    #[test]
    fn test_scaffold_java_production_features() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Java]).unwrap();
        assert_eq!(files.len(), 1);
        let content = &files[0].content;
        assert!(content.contains("<properties>"));
        assert!(content.contains("<project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>"));
        assert!(content.contains("<dependencies>"));
        assert!(content.contains("<build>"));
        assert!(content.contains("maven-compiler-plugin"));
        assert!(content.contains("maven-surefire-plugin"));
        assert!(content.contains("--enable-native-access=ALL-UNNAMED"));
        assert!(content.contains("-Djava.library.path=${project.basedir}/../../target/release"));
    }

    #[test]
    fn test_scaffold_ruby_production_features() {
        let config = test_config();
        let api = test_api();
        let files = scaffold(&api, &config, &[Language::Ruby]).unwrap();
        assert_eq!(files.len(), 6);
        let content = &files[0].content;
        assert!(content.contains("spec.required_ruby_version"));
        assert!(content.contains("spec.extensions"));
        assert!(content.contains("spec.metadata[\"keywords\"]"));
        // Check for .rubocop.yml generation
        assert_eq!(files[1].path, PathBuf::from("packages/ruby/.rubocop.yml"));
        // Check for Rakefile generation
        assert_eq!(files[2].path, PathBuf::from("packages/ruby/Rakefile"));
        assert!(files[2].content.contains("Rake::ExtensionTask"));
        assert!(files[2].content.contains("my_lib_rb"));
        // Check for lib entry point generation
        assert_eq!(files[3].path, PathBuf::from("packages/ruby/lib/my_lib.rb"));
        assert!(files[3].content.contains("require 'my_lib_rb'"));
        // Check for extconf.rb generation
        assert_eq!(files[4].path, PathBuf::from("packages/ruby/ext/my_lib_rb/extconf.rb"));
        assert!(files[4].content.contains("create_rust_makefile"));
        assert!(files[4].content.contains("rb_sys/mkmf"));
        // Check for Cargo.toml generation
        assert_eq!(files[5].path, PathBuf::from("packages/ruby/ext/my-lib_rb/Cargo.toml"));
        assert!(files[5].content.contains("magnus"));
    }
}
