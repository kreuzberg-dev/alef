//! R e2e test file rendering.

use crate::core::config::ResolvedCrateConfig;
use crate::core::hash::{self, CommentStyle};
use crate::e2e::config::E2eConfig;
use crate::e2e::fixture::Fixture;
use std::fmt::Write as FmtWrite;

use super::test_case::render_test_case;

pub(super) fn render_test_file(
    category: &str,
    fixtures: &[&Fixture],
    result_is_simple: bool,
    result_is_r_list: bool,
    e2e_config: &E2eConfig,
    config: &ResolvedCrateConfig,
    type_defs: &[crate::core::ir::TypeDef],
) -> String {
    let mut out = String::new();
    out.push_str(&hash::header(CommentStyle::Hash));
    let _ = writeln!(out, "# E2e tests for category: {category}");
    let _ = writeln!(out);

    for (i, fixture) in fixtures.iter().enumerate() {
        render_test_case(
            &mut out,
            fixture,
            e2e_config,
            result_is_simple,
            result_is_r_list,
            config,
            type_defs,
        );
        if i + 1 < fixtures.len() {
            let _ = writeln!(out);
        }
    }

    // Clean up trailing newlines.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
