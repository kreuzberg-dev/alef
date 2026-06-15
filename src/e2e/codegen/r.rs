//! R e2e test generator using testthat.

mod args;
mod assertions;
mod project;
mod stubs;
mod test_case;
mod test_file;
mod values;
mod visitor;

pub use stubs::emit_test_backend;

use crate::core::backend::GeneratedFile;
use crate::core::config::ResolvedCrateConfig;
use crate::e2e::config::E2eConfig;
use crate::e2e::escape::sanitize_filename;
use crate::e2e::fixture::{Fixture, FixtureGroup};
use anyhow::{Context as _, Result};
use std::path::PathBuf;

use super::E2eCodegen;

/// R e2e code generator.
pub struct RCodegen;

impl E2eCodegen for RCodegen {
    fn generate(
        &self,
        groups: &[FixtureGroup],
        e2e_config: &E2eConfig,
        config: &ResolvedCrateConfig,
        type_defs: &[crate::core::ir::TypeDef],
        _enums: &[crate::core::ir::EnumDef],
    ) -> Result<Vec<GeneratedFile>> {
        let lang = self.language_name();
        let output_base = PathBuf::from(e2e_config.effective_output()).join(lang);

        let mut files = Vec::new();

        let call = &e2e_config.call;
        let overrides = call.overrides.get(lang);
        let module_path = overrides
            .and_then(|o| o.module.as_ref())
            .cloned()
            .unwrap_or_else(|| call.module.clone());
        let result_is_simple = call.result_is_simple || overrides.is_some_and(|o| o.result_is_simple);
        let result_is_r_list = overrides.is_some_and(|o| o.result_is_r_list);

        let r_pkg = e2e_config.resolve_package("r");
        let pkg_name = r_pkg
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .cloned()
            .unwrap_or_else(|| module_path.clone());
        let pkg_path = r_pkg
            .as_ref()
            .and_then(|p| p.path.as_ref())
            .cloned()
            .unwrap_or_else(|| "../../packages/r".to_string());
        let pkg_version = r_pkg
            .as_ref()
            .and_then(|p| p.version.as_ref())
            .cloned()
            .or_else(|| config.resolved_version())
            .unwrap_or_else(|| "0.1.0".to_string());

        files.push(GeneratedFile {
            path: output_base.join("DESCRIPTION"),
            content: project::render_description(&pkg_name, &pkg_version, e2e_config.dep_mode),
            generated_header: false,
        });

        files.push(GeneratedFile {
            path: output_base.join("run_tests.R"),
            content: project::render_test_runner(&pkg_name, &pkg_path, e2e_config.dep_mode),
            generated_header: true,
        });

        if e2e_config.dep_mode == crate::e2e::config::DependencyMode::Registry {
            files.push(GeneratedFile {
                path: output_base.join("install.R"),
                content: project::render_install_r(
                    &pkg_name,
                    &pkg_version,
                    e2e_config
                        .registry
                        .github_repo
                        .as_deref()
                        .context("R registry mode requires `[crates.e2e.registry] github_repo`")?,
                ),
                generated_header: false,
            });
        }

        files.push(GeneratedFile {
            path: output_base.join("tests").join("setup-fixtures.R"),
            content: project::render_setup_fixtures(&e2e_config.test_documents_relative_from(1), &e2e_config.env),
            generated_header: true,
        });

        for group in groups {
            let active: Vec<&Fixture> = group
                .fixtures
                .iter()
                .filter(|f| super::should_include_fixture(f, lang, e2e_config))
                .collect();

            if active.is_empty() {
                continue;
            }

            let filename = format!("test_{}.R", sanitize_filename(&group.category));
            let content = test_file::render_test_file(
                &group.category,
                &active,
                result_is_simple,
                result_is_r_list,
                e2e_config,
                config,
                type_defs,
            );
            files.push(GeneratedFile {
                path: output_base.join("tests").join(filename),
                content,
                generated_header: true,
            });
        }

        Ok(files)
    }

    fn language_name(&self) -> &'static str {
        "r"
    }
}
