use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct AssetArgs {
    #[command(subcommand)]
    pub command: AssetCommands,
}

#[derive(Subcommand, Debug)]
pub enum AssetCommands {
    #[command(about = "List catalog assets")]
    List(AssetListArgs),
}

#[derive(Args, Debug)]
pub struct AssetListArgs {
    #[arg(long, help = "Optional asset kind filter (e.g. skill, mcp, prompt)")]
    pub kind: Option<String>,
}
