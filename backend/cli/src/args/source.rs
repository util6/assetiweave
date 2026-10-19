use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct SourceArgs {
    #[command(subcommand)]
    pub command: SourceCommands,
}

#[derive(Subcommand, Debug)]
pub enum SourceCommands {
    #[command(about = "List registered asset sources")]
    List,

    #[command(about = "Register a new asset source directory")]
    Add(SourceAddArgs),

    #[command(about = "Remove a registered asset source")]
    Remove(SourceRemoveArgs),

    #[command(about = "Scan registered asset sources for changes")]
    Scan(SourceScanArgs),
}

#[derive(Args, Debug)]
pub struct SourceAddArgs {
    #[arg(long, help = "Directory path of the source repository")]
    pub path: String,

    #[arg(long, help = "Display name for the source")]
    pub name: Option<String>,

    #[arg(long, help = "Default asset kind")]
    pub kind: Option<String>,

    #[arg(long, help = "Preview registration without persisting")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct SourceRemoveArgs {
    #[arg(help = "Source identifier")]
    pub id: String,

    #[arg(long, help = "Confirm destructive removal without prompt")]
    pub yes: bool,

    #[arg(long, help = "Preview removal without modifying database")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct SourceScanArgs {
    #[arg(long, help = "Optional asset kind filter")]
    pub kind: Option<String>,

    #[arg(long, help = "Dry run preview")]
    pub dry_run: bool,
}
