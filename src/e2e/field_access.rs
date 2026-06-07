//! Field path resolution for nested struct/map access in e2e assertions.
//!
//! The `FieldResolver` maps fixture field paths (e.g., "metadata.title") to
//! actual API struct paths (e.g., "metadata.document.title") and generates
//! language-specific accessor expressions.

mod optional_renderers;
mod parse;
mod renderers;
mod resolver;
mod types;

pub use types::{DartFirstClassMap, FieldResolver, PhpGetterMap, StringyField, StringyFieldKind, SwiftFirstClassMap};

#[cfg(test)]
#[path = "field_access/tests.rs"]
mod tests;
