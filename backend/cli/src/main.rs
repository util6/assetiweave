pub mod affordance;
mod args;
mod commands;
pub mod error;
mod render;
mod service;
mod trampoline;
mod view_models;

use args::CliArgs;
use clap::Parser;

fn main() {
    if let Some(exit_code) = trampoline::maybe_trampoline() {
        std::process::exit(exit_code);
    }

    run_async_main();
}

#[tokio::main]
async fn run_async_main() {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt::init();
    }

    let cli = CliArgs::parse();
    let format = cli.format;
    if let Err(err) = commands::dispatch(cli).await {
        render::render_error(&err, format);
        std::process::exit(1);
    }
}
