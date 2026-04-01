//! README generator for skif.

use skif_core::backend::GeneratedFile;
use skif_core::config::{Language, SkifConfig};
use skif_core::ir::ApiSurface;
use std::path::PathBuf;

/// Generate README files for the given languages.
pub fn generate_readmes(
    api: &ApiSurface,
    config: &SkifConfig,
    languages: &[Language],
) -> anyhow::Result<Vec<GeneratedFile>> {
    let mut files = vec![];
    for &lang in languages {
        files.push(generate_readme(api, config, lang)?);
    }
    Ok(files)
}

fn generate_readme(api: &ApiSurface, config: &SkifConfig, lang: Language) -> anyhow::Result<GeneratedFile> {
    let name = &config.crate_config.name;
    let description = config
        .scaffold
        .as_ref()
        .and_then(|s| s.description.clone())
        .unwrap_or_else(|| format!("Bindings for {}", name));
    let repository = config
        .scaffold
        .as_ref()
        .and_then(|s| s.repository.clone())
        .unwrap_or_else(|| format!("https://github.com/kreuzberg-dev/{}", name));

    let (lang_display, install_instructions, example_code, dir_name) = match lang {
        Language::Python => (
            "Python",
            format!("```bash\npip install {name}\n```"),
            format!(
                "```python\nimport {module}\n\n# TODO: add usage example\n```",
                module = config.python_module_name().trim_start_matches('_')
            ),
            "python",
        ),
        Language::Node => (
            "Node.js",
            format!("```bash\nnpm install {}\n```", config.node_package_name()),
            format!(
                "```typescript\nimport {{ /* ... */ }} from '{}';\n\n// TODO: add usage example\n```",
                config.node_package_name()
            ),
            "typescript",
        ),
        Language::Ruby => (
            "Ruby",
            format!("```bash\ngem install {}\n```", config.ruby_gem_name()),
            format!(
                "```ruby\nrequire '{}'\n\n# TODO: add usage example\n```",
                config.ruby_gem_name()
            ),
            "ruby",
        ),
        Language::Php => (
            "PHP",
            format!("```bash\ncomposer require kreuzberg-dev/{name}\n```"),
            format!(
                "```php\n<?php\n\nuse {};\n\n// TODO: add usage example\n```",
                config.php_extension_name()
            ),
            "php",
        ),
        Language::Elixir => (
            "Elixir",
            format!(
                "Add `:{app}` to your `mix.exs` dependencies:\n\n```elixir\ndefp deps do\n  [\n    {{:{app}, \"~> {version}\"}}\n  ]\nend\n```",
                app = config.elixir_app_name(),
                version = api.version,
            ),
            format!(
                "```elixir\n{module}.hello()\n\n# TODO: add usage example\n```",
                module = capitalize_first(&config.elixir_app_name()),
            ),
            "elixir",
        ),
        Language::Go => (
            "Go",
            format!("```bash\ngo get {}\n```", config.go_module()),
            format!(
                "```go\npackage main\n\nimport \"{module}\"\n\nfunc main() {{\n\t// TODO: add usage example\n}}\n```",
                module = config.go_module(),
            ),
            "go",
        ),
        Language::Java => (
            "Java",
            format!(
                "Add to your `pom.xml`:\n\n```xml\n<dependency>\n    <groupId>{package}</groupId>\n    <artifactId>{name}</artifactId>\n    <version>{version}</version>\n</dependency>\n```",
                package = config.java_package(),
                name = name,
                version = api.version,
            ),
            format!(
                "```java\nimport {package}.*;\n\n// TODO: add usage example\n```",
                package = config.java_package(),
            ),
            "java",
        ),
        Language::Csharp => (
            "C#",
            format!("```bash\ndotnet add package {}\n```", config.csharp_namespace()),
            format!(
                "```csharp\nusing {};\n\n// TODO: add usage example\n```",
                config.csharp_namespace()
            ),
            "csharp",
        ),
        Language::Ffi => (
            "FFI (C/C++)",
            format!(
                "Link against `lib{name}_ffi` and include `{header}`.\n\nSee the build instructions in the main repository.",
                name = name,
                header = config.ffi_header_name(),
            ),
            format!(
                "```c\n#include \"{header}\"\n\nint main(void) {{\n    // TODO: add usage example\n    return 0;\n}}\n```",
                header = config.ffi_header_name(),
            ),
            "ffi",
        ),
        Language::Wasm => (
            "WebAssembly",
            format!("```bash\nnpm install {name}-wasm\n```"),
            format!("```javascript\nimport init from '{name}-wasm';\n\nawait init();\n// TODO: add usage example\n```"),
            "wasm",
        ),
    };

    let content = format!(
        r#"# {name} - {lang_display} Bindings

{description}

## Installation

{install}

## Quick Start

{example}

## Documentation

For full documentation, see the [{name} repository]({repository}).

## License

See the [LICENSE]({repository}/blob/main/LICENSE) file in the root repository.
"#,
        name = name,
        lang_display = lang_display,
        description = description,
        install = install_instructions,
        example = example_code,
        repository = repository,
    );

    // Use the readme config output pattern if provided, otherwise default
    let path = match lang {
        Language::Ffi => PathBuf::from(format!("crates/{}-ffi/README.md", name)),
        Language::Wasm => PathBuf::from(format!("crates/{}-wasm/README.md", name)),
        _ => PathBuf::from(format!("packages/{}/README.md", dir_name)),
    };

    Ok(GeneratedFile {
        path,
        content,
        generated_header: false,
    })
}

/// Capitalize the first character of a string.
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
                authors: vec![],
                keywords: vec![],
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
    fn test_generate_python_readme() {
        let config = test_config();
        let api = test_api();
        let files = generate_readmes(&api, &config, &[Language::Python]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("packages/python/README.md"));
        assert!(files[0].content.contains("Python"));
        assert!(files[0].content.contains("pip install"));
    }

    #[test]
    fn test_generate_node_readme() {
        let config = test_config();
        let api = test_api();
        let files = generate_readmes(&api, &config, &[Language::Node]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("packages/typescript/README.md"));
        assert!(files[0].content.contains("Node.js"));
    }

    #[test]
    fn test_generate_multiple_readmes() {
        let config = test_config();
        let api = test_api();
        let files = generate_readmes(&api, &config, &[Language::Python, Language::Node]).unwrap();
        assert_eq!(files.len(), 2);
    }
}
