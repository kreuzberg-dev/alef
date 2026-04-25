use super::extras::Language;
use super::output::{StringOrVec, TestConfig};
use super::tools::{ToolsConfig, require_tool};

/// Return the default test configuration for a language.
///
/// The `output_dir` is the package directory where scaffolded files live
/// (e.g. `packages/python`). It is substituted into command templates.
/// `tools` selects the package manager for languages that drive tests
/// through one (Python, Node).
pub(crate) fn default_test_config(lang: Language, output_dir: &str, tools: &ToolsConfig) -> TestConfig {
    match lang {
        Language::Python => {
            let pm = tools.python_pm();
            // pytest is invoked via the package manager when one is present;
            // for `pip` we just call pytest directly.
            let (cmd, cov, pre_tool) = match pm {
                "pip" => (
                    format!("cd {output_dir} && pytest"),
                    format!("cd {output_dir} && pytest --cov=. --cov-report=lcov"),
                    "pytest",
                ),
                "poetry" => (
                    format!("cd {output_dir} && poetry run pytest"),
                    format!("cd {output_dir} && poetry run pytest --cov=. --cov-report=lcov"),
                    "poetry",
                ),
                _ => (
                    format!("cd {output_dir} && uv run pytest"),
                    format!("cd {output_dir} && uv run pytest --cov=. --cov-report=lcov"),
                    "uv",
                ),
            };
            TestConfig {
                precondition: Some(require_tool(pre_tool)),
                before: None,
                command: Some(StringOrVec::Single(cmd)),
                e2e: None,
                coverage: Some(StringOrVec::Single(cov)),
            }
        }
        Language::Node | Language::Wasm => {
            let pm = tools.node_pm();
            let (cmd, cov) = match pm {
                "npm" => (
                    format!("cd {output_dir} && npm test"),
                    format!("cd {output_dir} && npm test -- --coverage"),
                ),
                "yarn" => (
                    format!("cd {output_dir} && yarn test"),
                    format!("cd {output_dir} && yarn test --coverage"),
                ),
                _ => (
                    format!("cd {output_dir} && pnpm test"),
                    format!("cd {output_dir} && pnpm test -- --coverage"),
                ),
            };
            TestConfig {
                precondition: Some(require_tool(pm)),
                before: None,
                command: Some(StringOrVec::Single(cmd)),
                e2e: None,
                coverage: Some(StringOrVec::Single(cov)),
            }
        }
        Language::Go => TestConfig {
            precondition: Some(require_tool("go")),
            before: None,
            command: Some(StringOrVec::Single(format!("cd {output_dir} && go test ./..."))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!(
                "cd {output_dir} && go test -coverprofile=coverage.out ./..."
            ))),
        },
        Language::Ruby => TestConfig {
            precondition: Some(require_tool("bundle")),
            before: None,
            command: Some(StringOrVec::Single(format!("cd {output_dir} && bundle exec rspec"))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!(
                "cd {output_dir} && bundle exec rspec --format documentation"
            ))),
        },
        Language::Php => TestConfig {
            precondition: Some(require_tool("composer")),
            before: None,
            command: Some(StringOrVec::Single(format!("cd {output_dir} && composer test"))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!("cd {output_dir} && composer test"))),
        },
        Language::Java => TestConfig {
            precondition: Some(require_tool("mvn")),
            before: None,
            command: Some(StringOrVec::Single(format!("mvn -f {output_dir}/pom.xml test -q"))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!(
                "mvn -f {output_dir}/pom.xml test jacoco:report -q"
            ))),
        },
        Language::Csharp => TestConfig {
            precondition: Some(require_tool("dotnet")),
            before: None,
            command: Some(StringOrVec::Single(format!("dotnet test {output_dir}"))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!(
                "dotnet test {output_dir} --collect:\"XPlat Code Coverage\""
            ))),
        },
        Language::Elixir => TestConfig {
            precondition: Some(require_tool("mix")),
            before: None,
            command: Some(StringOrVec::Single(format!("cd {output_dir} && mix test"))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!("cd {output_dir} && mix test --cover"))),
        },
        Language::R => TestConfig {
            precondition: Some(require_tool("Rscript")),
            before: None,
            command: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"testthat::test_dir('tests')\""
            ))),
            e2e: None,
            coverage: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"testthat::test_dir('tests')\""
            ))),
        },
        Language::Rust => TestConfig {
            precondition: Some(require_tool("cargo")),
            before: None,
            command: Some(StringOrVec::Single("cargo test --workspace".to_string())),
            e2e: None,
            coverage: Some(StringOrVec::Single(
                "cargo llvm-cov --workspace --lcov --output-path coverage.lcov".to_string(),
            )),
        },
        Language::Ffi => TestConfig {
            precondition: None,
            before: None,
            command: None,
            e2e: None,
            coverage: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_languages() -> Vec<Language> {
        vec![
            Language::Python,
            Language::Node,
            Language::Wasm,
            Language::Ruby,
            Language::Php,
            Language::Go,
            Language::Java,
            Language::Csharp,
            Language::Elixir,
            Language::R,
            Language::Ffi,
            Language::Rust,
        ]
    }

    fn cfg(lang: Language, dir: &str) -> TestConfig {
        default_test_config(lang, dir, &ToolsConfig::default())
    }

    #[test]
    fn ffi_has_no_test_commands() {
        let c = cfg(Language::Ffi, "packages/ffi");
        assert!(c.command.is_none());
        assert!(c.e2e.is_none());
        assert!(c.coverage.is_none());
    }

    #[test]
    fn non_ffi_languages_have_command_and_coverage() {
        for lang in all_languages() {
            if lang == Language::Ffi {
                continue;
            }
            let c = cfg(lang, "packages/test");
            assert!(c.command.is_some(), "{lang} should have a default test command");
            assert!(c.coverage.is_some(), "{lang} should have a default coverage command");
        }
    }

    #[test]
    fn non_ffi_languages_have_default_precondition() {
        for lang in all_languages() {
            if lang == Language::Ffi {
                continue;
            }
            let c = cfg(lang, "packages/test");
            let pre = c
                .precondition
                .unwrap_or_else(|| panic!("{lang} should have a precondition"));
            assert!(pre.starts_with("command -v "));
        }
    }

    #[test]
    fn e2e_is_always_none() {
        for lang in all_languages() {
            let c = cfg(lang, "packages/test");
            assert!(c.e2e.is_none(), "{lang} e2e should always be None (user-configured)");
        }
    }

    #[test]
    fn python_uses_pytest_via_uv_by_default() {
        let c = cfg(Language::Python, "packages/python");
        let cmd = c.command.unwrap().commands().join(" ");
        assert!(cmd.contains("uv run pytest"));
    }

    #[test]
    fn python_test_dispatches_on_package_manager() {
        let mk = |pm: &str| ToolsConfig {
            python_package_manager: Some(pm.to_string()),
            ..Default::default()
        };
        let pip = default_test_config(Language::Python, "packages/python", &mk("pip"));
        assert!(pip.command.unwrap().commands().join(" ").contains("&& pytest"));
        let poetry = default_test_config(Language::Python, "packages/python", &mk("poetry"));
        assert!(
            poetry
                .command
                .unwrap()
                .commands()
                .join(" ")
                .contains("poetry run pytest")
        );
    }

    #[test]
    fn node_uses_pnpm_by_default() {
        let c = cfg(Language::Node, "packages/node");
        let cmd = c.command.unwrap().commands().join(" ");
        assert!(cmd.contains("pnpm test"));
    }

    #[test]
    fn node_test_dispatches_on_package_manager() {
        let mk = |pm: &str| ToolsConfig {
            node_package_manager: Some(pm.to_string()),
            ..Default::default()
        };
        let npm = default_test_config(Language::Node, "packages/node", &mk("npm"));
        assert!(npm.command.unwrap().commands().join(" ").contains("npm test"));
        let yarn = default_test_config(Language::Node, "packages/node", &mk("yarn"));
        assert!(yarn.command.unwrap().commands().join(" ").contains("yarn test"));
    }

    #[test]
    fn go_uses_go_test() {
        let c = cfg(Language::Go, "packages/go");
        let cmd = c.command.unwrap().commands().join(" ");
        assert!(cmd.contains("go test ./..."));
    }

    #[test]
    fn ruby_uses_rspec() {
        let c = cfg(Language::Ruby, "packages/ruby");
        let cmd = c.command.unwrap().commands().join(" ");
        assert!(cmd.contains("bundle exec rspec"));
    }

    #[test]
    fn java_uses_maven() {
        let c = cfg(Language::Java, "packages/java");
        let cmd = c.command.unwrap().commands().join(" ");
        let cov = c.coverage.unwrap().commands().join(" ");
        assert!(cmd.contains("mvn"));
        assert!(cov.contains("jacoco:report"));
    }

    #[test]
    fn rust_uses_cargo_and_llvm_cov() {
        let c = cfg(Language::Rust, "packages/rust");
        let cmd = c.command.unwrap().commands().join(" ");
        let cov = c.coverage.unwrap().commands().join(" ");
        assert!(cmd.contains("cargo test --workspace"));
        assert!(cov.contains("cargo llvm-cov"));
    }

    #[test]
    fn output_dir_substituted_in_commands() {
        let c = cfg(Language::Python, "my/custom/dir");
        let cmd = c.command.unwrap().commands().join(" ");
        assert!(cmd.contains("my/custom/dir"));
    }
}
