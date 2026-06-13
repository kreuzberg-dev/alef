//! Extension trait and supporting types for alef.

use crate::template_env::TemplateEnv;
use alef::core::backend::GeneratedFile;
use alef::core::config::Language;
use alef::core::ir::ApiSurface;
use anyhow::Result;
use std::any::Any;

/// Opaque per-extension configuration.
pub struct ExtensionConfig {
    pub inner: Option<Box<dyn Any + Send + Sync>>,
    pub raw: Option<toml::Value>,
}

impl ExtensionConfig {
    /// Construct an empty config.
    pub fn empty() -> Self {
        Self { inner: None, raw: None }
    }

    /// Construct from a raw TOML value.
    pub fn from_raw(raw: toml::Value) -> Self {
        Self {
            inner: None,
            raw: Some(raw),
        }
    }

    /// Construct with typed inner config and raw TOML value.
    pub fn with_typed<T: Any + Send + Sync>(typed: T, raw: Option<toml::Value>) -> Self {
        Self {
            inner: Some(Box::new(typed)),
            raw,
        }
    }

    /// Downcast the typed inner config to `T`.
    pub fn downcast<T: Any>(&self) -> Option<&T> {
        self.inner.as_ref().and_then(|b| b.downcast_ref::<T>())
    }
}

impl std::fmt::Debug for ExtensionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionConfig")
            .field("has_inner", &self.inner.is_some())
            .field("has_raw", &self.raw.is_some())
            .finish()
    }
}

/// Extension point for alef's code generation pipeline.
///
/// All three methods have default no-op implementations; override only what
/// you need.
pub trait Extension: Send + Sync {
    /// Stable, unique slug for this extension. Used as the TOML config key.
    fn name(&self) -> &str;

    /// Parse this extension's TOML section.
    ///
    /// Default: returns [`ExtensionConfig::empty`].
    fn parse_config(&self, raw: Option<&toml::Value>) -> Result<ExtensionConfig> {
        let _ = raw;
        Ok(ExtensionConfig::empty())
    }

    /// Augment the API surface after extraction and before generation.
    ///
    /// Default: no-op.
    fn augment_surface(&self, _api: &mut ApiSurface, _cfg: &ExtensionConfig) -> Result<()> {
        Ok(())
    }

    /// Emit extra files for one language.
    ///
    /// Default: returns an empty list.
    fn emit_for_language(
        &self,
        _api: &ApiSurface,
        _cfg: &ExtensionConfig,
        _language: Language,
        _env: &TemplateEnv,
    ) -> Result<Vec<GeneratedFile>> {
        Ok(vec![])
    }
}
