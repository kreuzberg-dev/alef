use anyhow::Result;
use clap::Parser;

mod bin_cli;

use bin_cli::args::Cli;
use bin_cli::dispatch;
use bin_cli::helpers::init_tracing;

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose, cli.quiet, cli.no_color);

    if cli.jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(cli.jobs)
            .build_global()
            .ok();
    }

    dispatch::run(cli)
}
