//! `alef-core` — public extension API for the alef polyglot binding generator.
//!
//! Exposes the extension contract that consumers implement to add domain-specific
//! code generation:
//!
//! - [`Extension`] — the single trait for all extension modes.
//! - [`ExtensionConfig`] — opaque typed config threaded through the pipeline.
//! - [`TemplateEnv`] — thin minijinja wrapper extensions use for rendering.
//! - [`extensions::template::TemplateExtension`] — built-in extension that
//!   renders `[[extensions.template]]` TOML blocks.
//!
//! ## Extension modes
//!
//! - **Linked** — implement `Extension`, ship a thin bin.
//! - **Dynamic** — optional `dylib-loader` feature loads `.so/.dylib/.dll`.
//! - **Template-only** — [`extensions::template::TemplateExtension`] reads
//!   `[[extensions.template]]` blocks; no Rust required.

#![allow(missing_docs)]

pub mod extension;
pub mod extensions;
pub mod template_env;

pub use extension::{Extension, ExtensionConfig};
pub use extensions::template::TemplateExtension;
pub use template_env::TemplateEnv;
