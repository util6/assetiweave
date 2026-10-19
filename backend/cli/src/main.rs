mod args;
mod commands;
mod render;
mod service;
mod view_models;

use anyhow::Result;
use args::CliArgs;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt::init();
    }

    let cli = CliArgs::parse();
    commands::dispatch(cli).await
}
