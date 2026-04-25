//! Validation of user-supplied pipeline overrides in `alef.toml`.
//!
//! When a user provides an explicit `[lint.<lang>]` / `[test.<lang>]` /
//! `[build_commands.<lang>]` / `[setup.<lang>]` / `[update.<lang>]` /
//! `[clean.<lang>]` table that **sets a main command field**, that table
//! must also declare a `precondition`. The rationale:
//!
//! - Built-in defaults all declare a `command -v <tool>` precondition so
//!   pipelines degrade gracefully when the underlying tool is missing.
//! - Custom commands are opaque to alef — only the user knows what the
//!   command requires. Forcing an explicit `precondition` keeps the
//!   warn-and-skip behavior intact on systems that can't run the command.
//!
//! Tables that only customize `before` (without overriding the main command)
//! are exempt: the default precondition still applies via the surrounding
//! defaults logic.

use std::collections::HashMap;

use super::AlefConfig;
use super::output::{BuildCommandConfig, CleanConfig, LintConfig, SetupConfig, StringOrVec, TestConfig, UpdateConfig};
use crate::error::AlefError;

/// Validate user-supplied pipeline overrides.
///
/// Returns the first error encountered (or `Ok(())` when every user-supplied
/// table either declares a precondition or only sets non-main fields).
pub fn validate(config: &AlefConfig) -> Result<(), AlefError> {
    if let Some(map) = &config.lint {
        validate_section("lint", map, lint_main_fields, |c| c.precondition.as_deref())?;
    }
    if let Some(map) = &config.test {
        validate_section("test", map, test_main_fields, |c| c.precondition.as_deref())?;
    }
    if let Some(map) = &config.build_commands {
        validate_section("build_commands", map, build_main_fields, |c| c.precondition.as_deref())?;
    }
    if let Some(map) = &config.setup {
        validate_section("setup", map, setup_main_fields, |c| c.precondition.as_deref())?;
    }
    if let Some(map) = &config.update {
        validate_section("update", map, update_main_fields, |c| c.precondition.as_deref())?;
    }
    if let Some(map) = &config.clean {
        validate_section("clean", map, clean_main_fields, |c| c.precondition.as_deref())?;
    }
    Ok(())
}

fn validate_section<C, F, P>(
    section: &str,
    table: &HashMap<String, C>,
    main_fields: F,
    precondition: P,
) -> Result<(), AlefError>
where
    F: Fn(&C) -> &'static [&'static str],
    P: Fn(&C) -> Option<&str>,
{
    for (lang, cfg) in table {
        let main = main_fields(cfg);
        if !main.is_empty() && precondition(cfg).is_none() {
            let fields = main.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join("/");
            return Err(AlefError::Config(format!(
                "[{section}.{lang}] sets a main command ({fields}) without `precondition`. \
                 Custom commands must declare a `precondition` so the step degrades gracefully \
                 when the tool is missing on the user's system. Use a POSIX check such as \
                 `precondition = \"command -v <tool> >/dev/null 2>&1\"`."
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-config "is a main command set?" helpers.
//
// Each helper returns a slice listing the names of the main fields that are
// currently set; emptiness means the user table only customizes ancillary
// fields (typically `before`), which doesn't require a precondition.
// ---------------------------------------------------------------------------

fn lint_main_fields(c: &LintConfig) -> &'static [&'static str] {
    match (c.format.is_some(), c.check.is_some(), c.typecheck.is_some()) {
        (false, false, false) => &[],
        _ => &["format", "check", "typecheck"],
    }
}

fn test_main_fields(c: &TestConfig) -> &'static [&'static str] {
    match (c.command.is_some(), c.e2e.is_some(), c.coverage.is_some()) {
        (false, false, false) => &[],
        _ => &["command", "e2e", "coverage"],
    }
}

fn build_main_fields(c: &BuildCommandConfig) -> &'static [&'static str] {
    match (c.build.is_some(), c.build_release.is_some()) {
        (false, false) => &[],
        _ => &["build", "build_release"],
    }
}

fn setup_main_fields(c: &SetupConfig) -> &'static [&'static str] {
    if c.install.is_some() { &["install"] } else { &[] }
}

fn update_main_fields(c: &UpdateConfig) -> &'static [&'static str] {
    match (c.update.is_some(), c.upgrade.is_some()) {
        (false, false) => &[],
        _ => &["update", "upgrade"],
    }
}

fn clean_main_fields(c: &CleanConfig) -> &'static [&'static str] {
    if c.clean.is_some() { &["clean"] } else { &[] }
}

// `StringOrVec` is referenced indirectly through the config types; ensure the
// import is used so the compiler doesn't warn on unused imports for non-test
// consumers of this module.
#[allow(dead_code)]
fn _string_or_vec_marker(_: &StringOrVec) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> AlefConfig {
        toml::from_str(toml_str).expect("config should parse")
    }

    fn base_config() -> &'static str {
        r#"
languages = ["python"]
[crate]
name = "test-lib"
sources = ["src/lib.rs"]
"#
    }

    #[test]
    fn no_user_overrides_is_valid() {
        let config = parse(base_config());
        validate(&config).expect("default config should validate");
    }

    #[test]
    fn lint_override_with_main_cmd_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[lint.python]\nformat = \"black .\"\n",
            base = base_config()
        ));
        let err = validate(&config).expect_err("missing precondition should error");
        let msg = format!("{err}");
        assert!(msg.contains("[lint.python]"), "error should name the section: {msg}");
        assert!(msg.contains("precondition"), "error should mention precondition: {msg}");
    }

    #[test]
    fn lint_override_with_main_cmd_and_precondition_is_ok() {
        let config = parse(&format!(
            "{base}\n\n[lint.python]\nprecondition = \"command -v black\"\nformat = \"black .\"\n",
            base = base_config()
        ));
        validate(&config).expect("config with precondition should validate");
    }

    #[test]
    fn lint_override_with_only_before_no_precondition_is_ok() {
        // Adding `before` doesn't override the main command, so no precondition required.
        let config = parse(&format!(
            "{base}\n\n[lint.python]\nbefore = \"echo hi\"\n",
            base = base_config()
        ));
        validate(&config).expect("table with only `before` should validate");
    }

    #[test]
    fn test_override_with_main_cmd_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[test.python]\ncommand = \"pytest\"\n",
            base = base_config()
        ));
        let err = validate(&config).expect_err("missing precondition should error");
        assert!(format!("{err}").contains("[test.python]"));
    }

    #[test]
    fn test_override_with_only_e2e_requires_precondition() {
        let config = parse(&format!(
            "{base}\n\n[test.python]\ne2e = \"pytest tests/e2e\"\n",
            base = base_config()
        ));
        validate(&config).expect_err("e2e without precondition should error");
    }

    #[test]
    fn build_override_with_main_cmd_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[build_commands.python]\nbuild = \"maturin develop\"\n",
            base = base_config()
        ));
        let err = validate(&config).expect_err("missing precondition should error");
        assert!(format!("{err}").contains("[build_commands.python]"));
    }

    #[test]
    fn setup_override_with_install_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[setup.python]\ninstall = \"uv sync\"\n",
            base = base_config()
        ));
        validate(&config).expect_err("setup install without precondition should error");
    }

    #[test]
    fn update_override_with_main_cmd_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[update.python]\nupdate = \"uv sync --upgrade\"\n",
            base = base_config()
        ));
        validate(&config).expect_err("update without precondition should error");
    }

    #[test]
    fn clean_override_with_main_cmd_no_precondition_errors() {
        let config = parse(&format!(
            "{base}\n\n[clean.python]\nclean = \"rm -rf dist\"\n",
            base = base_config()
        ));
        validate(&config).expect_err("clean without precondition should error");
    }

    #[test]
    fn override_with_main_cmd_and_precondition_validates_for_each_section() {
        let cases = [
            ("lint.python", "format", "command -v black"),
            ("test.python", "command", "command -v pytest"),
            ("build_commands.python", "build", "command -v maturin"),
            ("setup.python", "install", "command -v uv"),
            ("update.python", "update", "command -v uv"),
            ("clean.python", "clean", "command -v rm"),
        ];
        for (header, field, pre) in cases {
            let toml_str = format!(
                "{base}\n\n[{header}]\nprecondition = \"{pre}\"\n{field} = \"echo run\"\n",
                base = base_config()
            );
            let config = parse(&toml_str);
            validate(&config).unwrap_or_else(|_| panic!("[{header}] with precondition should validate"));
        }
    }
}
