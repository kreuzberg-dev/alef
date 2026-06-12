//! Rust e2e test-file collection helpers.

use crate::e2e::escape::sanitize_filename;
use crate::e2e::fixture::FixtureGroup;

/// Collect test file names for use in build.zig and similar build scripts.
pub fn collect_test_filenames(groups: &[FixtureGroup]) -> Vec<String> {
    groups
        .iter()
        .filter(|g| !g.fixtures.is_empty())
        .map(|g| format!("{}_test.rs", sanitize_filename(&g.category)))
        .collect()
}
