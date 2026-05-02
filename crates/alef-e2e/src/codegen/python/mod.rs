//! Python e2e test code generator.
//!
//! Generates `e2e/python/conftest.py` and `tests/test_{category}.py` files from
//! JSON fixtures, driven entirely by `E2eConfig` and `CallConfig`.

mod assertions;
mod config;
mod helpers;
mod http;
mod json;
mod test_file;
mod test_function;
mod visitors;

use std::path::PathBuf;

use crate::config::E2eConfig;
use crate::escape::sanitize_filename;
use crate::fixture::{Fixture, FixtureGroup};
use alef_core::backend::GeneratedFile;
use alef_core::config::ResolvedCrateConfig;
use anyhow::Result;

use self::config::{render_conftest, render_pyproject};
use self::helpers::is_skipped;
use self::test_file::render_test_file;

/// Python e2e test code generator.
pub struct PythonE2eCodegen;

impl super::E2eCodegen for PythonE2eCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        _config: &ResolvedCrateConfig,
    ) -> Result<Vec<GeneratedFile>> {
        let mut files = Vec::new();
        let output_base = PathBuf::from(e2e_config.effective_output()).join("python");

        files.push(GeneratedFile {
            path: output_base.join("conftest.py"),
            content: render_conftest(e2e_config, groups),
            generated_header: true,
        });

        files.push(GeneratedFile {
            path: output_base.join("__init__.py"),
            content: "\n".to_string(),
            generated_header: false,
        });

        files.push(GeneratedFile {
            path: output_base.join("tests").join("__init__.py"),
            content: "\n".to_string(),
            generated_header: false,
        });

        let python_pkg = e2e_config.resolve_package("python");
        let pkg_name = python_pkg
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("kreuzcrawl");
        let pkg_path = python_pkg
            .as_ref()
            .and_then(|p| p.path.as_deref())
            .unwrap_or("../../packages/python");
        let pkg_version = python_pkg
            .as_ref()
            .and_then(|p| p.version.as_deref())
            .unwrap_or("0.1.0");
        files.push(GeneratedFile {
            path: output_base.join("pyproject.toml"),
            content: render_pyproject(pkg_name, pkg_path, pkg_version, e2e_config.dep_mode),
            generated_header: true,
        });

        for group in groups {
            let fixtures: Vec<&Fixture> = group.fixtures.iter().collect();
            if fixtures.is_empty() {
                continue;
            }
            if fixtures.iter().all(|f| is_skipped(f, "python")) {
                continue;
            }

            let filename = format!("test_{}.py", sanitize_filename(&group.category));
            let content = render_test_file(&group.category, &fixtures, e2e_config);
            files.push(GeneratedFile {
                path: output_base.join("tests").join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "python"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::E2eCodegen;

    #[test]
    fn language_name_is_python() {
        let codegen = PythonE2eCodegen;
        assert_eq!(codegen.language_name(), "python");
    }

    #[test]
    fn generate_empty_groups_produces_config_files_only() {
        use alef_core::config::NewAlefConfig;
        let cfg: NewAlefConfig = toml::from_str(
            r#"
[workspace]
languages = ["python"]

[[crates]]
name = "my-lib"
sources = ["src/lib.rs"]

[crates.e2e]
fixtures = "fixtures"
output = "e2e"
[crates.e2e.call]
function = "process"
module = "my-lib"
result_var = "result"
"#,
        )
        .unwrap();
        let e2e = cfg.crates[0].e2e.clone().unwrap();
        let resolved = cfg.resolve().unwrap().remove(0);
        let codegen = PythonE2eCodegen;
        let files = codegen.generate(&[], &e2e, &resolved).unwrap();
        // conftest.py, __init__.py (root), tests/__init__.py, pyproject.toml
        assert_eq!(files.len(), 4, "expected 4 config files, got: {}", files.len());
        let paths: Vec<_> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();
        assert!(paths.iter().any(|p| p.ends_with("conftest.py")));
        assert!(paths.iter().any(|p| p.ends_with("pyproject.toml")));
    }
}
