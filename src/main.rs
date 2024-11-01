mod cli;
mod nix;
mod util;

use clap::Parser;
use cli::Cli;
use nix::Nix;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> miette::Result<()> {
    miette::set_panic_hook();

    let nix = Nix::locate()?;

    let Cli {
        language,

        quiet,
        debug,
        trace,
    } = Cli::parse();

    init_logging(quiet, debug, trace);

    language.run(nix).await
}

fn init_logging(quiet: bool, debug: bool, trace: bool) {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().compact())
        .with(tracing_subscriber::EnvFilter::from(format!(
            "nps={}",
            trace
                .then_some("tracing")
                .or_else(|| (debug || cfg!(debug_assertions)).then_some("debug"))
                .or_else(|| quiet.then_some("warning"))
                .unwrap_or("info")
        )))
        .init();
}
