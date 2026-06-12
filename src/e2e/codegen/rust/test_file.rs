//! Per-category test file generation for Rust e2e tests.

mod collection;
mod file_rendering;
mod helpers;
mod test_function;

#[cfg(test)]
mod tests;

pub use collection::collect_test_filenames;
pub use file_rendering::render_test_file;
pub(super) use helpers::is_skipped;
pub use test_function::render_test_function;
