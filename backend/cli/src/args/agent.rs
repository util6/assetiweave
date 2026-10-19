use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage Agent Market installations and runtime bindings")]
pub struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Subcommand, Debug)]
pub enum AgentCommand {
    #[command(about = "Browse curated Agent Market items")]
    Market(AgentMarketArgs),

    #[command(about = "List or get current Agent installations")]
    Installed(AgentInstalledArgs),

    #[command(about = "Preview and synchronously install one Agent")]
    Install(AgentInstallArgs),

    #[command(about = "Preview and synchronously update one Agent")]
    Update(AgentInstallArgs),

    #[command(about = "Preview and synchronously reinstall one Agent")]
    Reinstall(AgentInstallArgs),

    #[command(about = "Uninstall one Agent")]
    Uninstall(AgentUninstallArgs),

    #[command(about = "Enable one installed Agent")]
    Enable(AgentIdArgs),

    #[command(about = "Disable one installed Agent")]
    Disable(AgentIdArgs),

    #[command(about = "Check Agent runtime health and connectivity")]
    Check(AgentCheckArgs),
}

#[derive(Args, Debug)]
pub struct AgentMarketArgs {
    #[command(subcommand)]
    pub command: AgentMarketCommand,
}

#[derive(Subcommand, Debug)]
pub enum AgentMarketCommand {
    #[command(about = "List curated Agent Market items")]
    List(AgentMarketListArgs),

    #[command(about = "Inspect one curated Agent Market item")]
    Inspect(AgentIdArgs),

    #[command(about = "Refresh the controlled curated Agent Market catalog")]
    Refresh(AgentMarketRefreshArgs),
}

#[derive(Args, Debug)]
pub struct AgentMarketListArgs {
    #[arg(long, help = "search Agent id, name or description")]
    pub query: Option<String>,

    #[arg(long, help = "protocol filter: acp or native")]
    pub protocol: Option<String>,

    #[arg(long, help = "only list installed Agents")]
    pub installed_only: bool,
}

#[derive(Args, Debug)]
pub struct AgentMarketRefreshArgs {
    #[arg(long, help = "confirm catalog network access and cache write")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct AgentInstalledArgs {
    #[command(subcommand)]
    pub command: Option<AgentInstalledCommand>,
}

#[derive(Subcommand, Debug)]
pub enum AgentInstalledCommand {
    #[command(about = "List current Agent installations")]
    List,

    #[command(about = "Get one current Agent installation")]
    Get(AgentIdArgs),
}

#[derive(Args, Debug)]
pub struct AgentInstallArgs {
    #[arg(help = "agent ID")]
    pub agent_id: String,

    #[arg(long, help = "distribution id from `agent market inspect`")]
    pub distribution: Option<String>,

    #[arg(long, help = "confirm installation and local runtime writes")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct AgentUninstallArgs {
    #[arg(help = "agent ID")]
    pub agent_id: String,

    #[arg(long, help = "confirm uninstallation")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct AgentIdArgs {
    #[arg(help = "agent ID")]
    pub agent_id: String,
}

#[derive(Args, Debug)]
pub struct AgentCheckArgs {
    #[arg(help = "optional agent ID to check")]
    pub agent_id: Option<String>,
}
