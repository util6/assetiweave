use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Inspect local CLI configuration")]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    #[command(about = "Inspect CLI plugins")]
    Plugins(ConfigPluginsArgs),
}

#[derive(Args, Debug)]
pub struct ConfigPluginsArgs {
    #[command(subcommand)]
    pub command: ConfigPluginsCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigPluginsCommand {
    #[command(about = "Show installed plugin inventory")]
    Show,
}
