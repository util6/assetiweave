use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct TenantArgs {
    #[command(subcommand)]
    pub command: TenantCommands,
}

#[derive(Subcommand, Debug)]
pub enum TenantCommands {
    #[command(about = "List all tenants")]
    List,

    #[command(about = "Show currently active tenant")]
    Active,

    #[command(about = "Switch to a specific tenant")]
    Switch(TenantSwitchArgs),

    #[command(about = "Create a new tenant")]
    Create(TenantCreateArgs),
}

#[derive(Args, Debug)]
pub struct TenantSwitchArgs {
    #[arg(help = "Target tenant identifier")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct TenantCreateArgs {
    #[arg(help = "New tenant identifier")]
    pub id: String,

    #[arg(long, help = "Tenant display name")]
    pub name: Option<String>,
}
