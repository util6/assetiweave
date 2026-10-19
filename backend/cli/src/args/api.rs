use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Call engine methods directly")]
pub struct ApiArgs {
    #[command(subcommand)]
    pub command: ApiCommand,
}

#[derive(Subcommand, Debug)]
pub enum ApiCommand {
    #[command(about = "Call an engine method with JSON params")]
    Call(ApiCallArgs),
}

#[derive(Args, Debug)]
pub struct ApiCallArgs {
    #[arg(help = "method name to call")]
    pub method: String,

    #[arg(long, help = "JSON params, @file path, or - for stdin")]
    pub json: Option<String>,

    #[arg(long, help = "confirm a high-risk engine method")]
    pub yes: bool,
}
