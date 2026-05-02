//! PHP-specific naming helpers for `ResolvedCrateConfig`.

use alef_core::config::ResolvedCrateConfig;

/// Get the PHP Composer autoload namespace derived from the extension name.
///
/// Converts the extension name (e.g. `html_to_markdown_rs`) into a
/// PSR-4 namespace string (e.g. `Html\\To\\Markdown\\Rs`).
pub fn php_autoload_namespace(config: &ResolvedCrateConfig) -> String {
    use heck::ToPascalCase;
    let ext = config.php_extension_name();
    if ext.contains('_') {
        ext.split('_')
            .map(|p| p.to_pascal_case())
            .collect::<Vec<_>>()
            .join("\\")
    } else {
        ext.to_pascal_case()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alef_core::config::new_config::NewAlefConfig;

    fn resolved_one(toml: &str) -> ResolvedCrateConfig {
        let cfg: NewAlefConfig = toml::from_str(toml).unwrap();
        cfg.resolve().unwrap().remove(0)
    }

    fn minimal() -> ResolvedCrateConfig {
        resolved_one(
            r#"
[workspace]
languages = ["php"]

[[crates]]
name = "test-lib"
sources = ["src/lib.rs"]
"#,
        )
    }

    #[test]
    fn php_autoload_namespace_converts_snake_to_pascal_parts() {
        let r = minimal();
        // "test-lib" → php_extension_name → "test_lib" → "Test\\Lib"
        assert_eq!(php_autoload_namespace(&r), "Test\\Lib");
    }

    #[test]
    fn php_autoload_namespace_no_underscore_returns_single_pascal() {
        let r = resolved_one(
            r#"
[workspace]
languages = ["php"]

[[crates]]
name = "mylib"
sources = ["src/lib.rs"]
"#,
        );
        // "mylib" → php_extension_name → "mylib" → "Mylib"
        assert_eq!(php_autoload_namespace(&r), "Mylib");
    }

    #[test]
    fn php_autoload_namespace_explicit_extension_name() {
        let r = resolved_one(
            r#"
[workspace]
languages = ["php"]

[[crates]]
name = "test-lib"
sources = ["src/lib.rs"]

[crates.php]
extension_name = "html_to_markdown_rs"
"#,
        );
        assert_eq!(php_autoload_namespace(&r), "Html\\To\\Markdown\\Rs");
    }
}
