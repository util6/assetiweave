use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCommands,
}

#[derive(Subcommand, Debug)]
pub enum ProfileCommands {
    #[command(about = "List supported application target profiles")]
    List,
}
