mod build;
mod clean;
mod lint;
mod setup;
mod test;
mod test_apps;
mod update;

pub use build::{build, run_post_build};
pub use clean::clean;
pub use lint::{fmt, fmt_post_generate, lint};
pub use setup::setup;
pub use test::test;
pub use test_apps::test_apps_run;
pub use update::update;
