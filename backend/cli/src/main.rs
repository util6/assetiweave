mod args;
mod commands;
mod render;
mod service;
mod trampoline;
mod view_models;

use anyhow::Result;
use args::CliArgs;
use clap::Parser;

fn main() -> Result<()> {
    if let Some(exit_code) = trampoline::maybe_trampoline() {
        std::process::exit(exit_code);
    }

    run_async_main()
}

#[tokio::main]
async fn run_async_main() -> Result<()> {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt::init();
    }

    let cli = CliArgs::parse();
    commands::dispatch(cli).await
}
