use super::extras::Language;
use super::output::{StringOrVec, UpdateConfig};

/// Return the default update configuration for a language.
///
/// The `output_dir` is the package directory where scaffolded files live
/// (e.g. `packages/python`). It is substituted into command templates.
pub fn default_update_config(lang: Language, output_dir: &str) -> UpdateConfig {
    match lang {
        Language::Rust => UpdateConfig {
            update: Some(StringOrVec::Single("cargo update".to_string())),
            upgrade: Some(StringOrVec::Multiple(vec![
                "cargo upgrade --incompatible".to_string(),
                "cargo update".to_string(),
            ])),
        },
        Language::Python => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "cd {output_dir} && uv sync --upgrade"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "cd {output_dir} && uv sync --all-packages --all-extras --upgrade"
            ))),
        },
        Language::Node | Language::Wasm => UpdateConfig {
            update: Some(StringOrVec::Single("pnpm up -r".to_string())),
            upgrade: Some(StringOrVec::Multiple(vec![
                "corepack up".to_string(),
                "pnpm up --latest -r -w".to_string(),
            ])),
        },
        Language::Ruby => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "cd {output_dir} && bundle update --all"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "cd {output_dir} && bundle update --all --conservative=false"
            ))),
        },
        Language::Php => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "cd {output_dir} && composer update"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "cd {output_dir} && composer update --with-all-dependencies"
            ))),
        },
        Language::Go => UpdateConfig {
            update: Some(StringOrVec::Multiple(vec![
                format!("cd {output_dir} && go get -u ./..."),
                format!("cd {output_dir} && go mod tidy"),
            ])),
            upgrade: Some(StringOrVec::Multiple(vec![
                format!("cd {output_dir} && go get -u ./..."),
                format!("cd {output_dir} && go mod tidy"),
            ])),
        },
        Language::Java => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "mvn -f {output_dir}/pom.xml versions:use-latest-releases -q"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "mvn -f {output_dir}/pom.xml versions:use-latest-releases -DallowMajorUpdates=true -q"
            ))),
        },
        Language::Csharp => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "dotnet outdated --upgrade {output_dir}"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "dotnet outdated --upgrade --version-lock major {output_dir}"
            ))),
        },
        Language::Elixir => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "cd {output_dir} && mix deps.update --all"
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "cd {output_dir} && mix deps.update --all"
            ))),
        },
        Language::R => UpdateConfig {
            update: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"remotes::update_packages()\""
            ))),
            upgrade: Some(StringOrVec::Single(format!(
                "cd {output_dir} && Rscript -e \"remotes::update_packages()\""
            ))),
        },
        Language::Ffi => UpdateConfig {
            update: None,
            upgrade: None,
        },
    }
}
