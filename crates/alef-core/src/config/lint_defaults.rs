use super::extras::Language;
use super::output::{LintConfig, StringOrVec};

/// Return the default lint configuration for a language.
///
/// The `output_dir` is the package directory where scaffolded files live
/// (e.g. `packages/python`). It is substituted into command templates.
pub fn default_lint_config(lang: Language, output_dir: &str) -> LintConfig {
    match lang {
        Language::Python => LintConfig {
            format: Some(StringOrVec::Single(format!("ruff format {output_dir}"))),
            check: Some(StringOrVec::Single(format!("ruff check --fix {output_dir}"))),
            typecheck: Some(StringOrVec::Single(format!("mypy {output_dir}"))),
        },
        Language::Node | Language::Wasm => LintConfig {
            format: Some(StringOrVec::Single(format!("npx oxfmt {output_dir}"))),
            check: Some(StringOrVec::Single(format!(
                "npx oxlint --fix {output_dir}"
            ))),
            typecheck: None,
        },
        Language::Ruby => LintConfig {
            format: Some(StringOrVec::Single(format!("bundle exec rubocop -A {output_dir}"))),
            check: Some(StringOrVec::Single(format!("bundle exec rubocop {output_dir}"))),
            typecheck: None,
        },
        Language::Php => LintConfig {
            format: Some(StringOrVec::Single(format!("cd {output_dir} && composer run format"))),
            check: Some(StringOrVec::Single(format!("cd {output_dir} && composer run lint"))),
            typecheck: None,
        },
        Language::Go => LintConfig {
            format: Some(StringOrVec::Single(format!("gofmt -w {output_dir}"))),
            check: Some(StringOrVec::Single(format!(
                "cd {output_dir} && golangci-lint run ./..."
            ))),
            typecheck: None,
        },
        Language::Java => LintConfig {
            format: Some(StringOrVec::Single(format!(
                "mvn -f {output_dir}/pom.xml spotless:apply -q"
            ))),
            check: Some(StringOrVec::Single(format!(
                "mvn -f {output_dir}/pom.xml spotless:check checkstyle:check -q"
            ))),
            typecheck: None,
        },
        Language::Csharp => LintConfig {
            format: Some(StringOrVec::Single(format!("dotnet format {output_dir}"))),
            check: Some(StringOrVec::Single(format!(
                "dotnet format {output_dir} --verify-no-changes"
            ))),
            typecheck: None,
        },
        Language::Elixir => LintConfig {
            format: Some(StringOrVec::Single(format!("cd {output_dir} && mix format"))),
            check: Some(StringOrVec::Single(format!("cd {output_dir} && mix credo --strict"))),
            typecheck: None,
        },
        Language::R => LintConfig {
            format: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"styler::style_pkg()\""
            ))),
            check: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"lintr::lint_package()\""
            ))),
            typecheck: None,
        },
        Language::Ffi => LintConfig {
            format: Some(StringOrVec::Single(format!(
                "find {output_dir}/tests -name '*.c' -o -name '*.h' | xargs clang-format -i"
            ))),
            check: Some(StringOrVec::Single(format!("cppcheck {output_dir}/tests/"))),
            typecheck: None,
        },
        Language::Rust => LintConfig {
            format: Some(StringOrVec::Single("cargo fmt".to_string())),
            check: Some(StringOrVec::Single(
                "cargo clippy --fix --allow-dirty --allow-staged -- -D warnings".to_string(),
            )),
            typecheck: None,
        },
    }
}
