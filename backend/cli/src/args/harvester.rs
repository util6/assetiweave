use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage local web conversation harvesters")]
pub struct HarvesterArgs {
    #[command(subcommand)]
    pub command: HarvesterCommand,
}

#[derive(Subcommand, Debug)]
pub enum HarvesterCommand {
    #[command(about = "Inspect packaged harvester templates")]
    Template(HarvesterTemplateArgs),

    #[command(about = "Install a packaged or external harvester into ~/.assetiweave")]
    Install(HarvesterInstallArgs),

    #[command(about = "Update installed official harvester templates")]
    Update(HarvesterUpdateArgs),

    #[command(about = "List installed local harvesters")]
    List(HarvesterListArgs),

    #[command(about = "Diagnose an installed conversation harvester without changing it")]
    Doctor(HarvesterDoctorArgs),

    #[command(about = "Restore an official harvester while preserving authentication and output")]
    Repair(HarvesterRepairArgs),

    #[command(about = "Run an installed harvester script")]
    Run(HarvesterRunArgs),
}

#[derive(Args, Debug)]
pub struct HarvesterTemplateArgs {
    #[command(subcommand)]
    pub command: HarvesterTemplateCommand,
}

#[derive(Subcommand, Debug)]
pub enum HarvesterTemplateCommand {
    #[command(about = "List packaged official harvester templates")]
    List(HarvesterRootArgs),
}

#[derive(Args, Debug)]
pub struct HarvesterRootArgs {
    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,
}

#[derive(Args, Debug)]
pub struct HarvesterInstallArgs {
    #[arg(help = "template or harvester ID")]
    pub id: String,

    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,

    #[arg(long = "from", help = "external package directory, archive, or URL")]
    pub from: Option<String>,

    #[arg(long, help = "replace an existing installed template")]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct HarvesterUpdateArgs {
    #[arg(help = "optional template ID to update")]
    pub id: Option<String>,

    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,

    #[arg(long = "from", help = "external package directory, archive, or URL")]
    pub from: Option<String>,

    #[arg(long, help = "update all packaged official templates")]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct HarvesterListArgs {
    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,
}

#[derive(Args, Debug)]
pub struct HarvesterDoctorArgs {
    #[arg(help = "harvester ID")]
    pub id: String,

    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,
}

#[derive(Args, Debug)]
pub struct HarvesterRepairArgs {
    #[arg(help = "harvester ID")]
    pub id: String,

    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,

    #[arg(long, help = "preview repair without changing files")]
    pub dry_run: bool,

    #[arg(long, help = "confirm restoring official package files")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct HarvesterRunArgs {
    #[arg(help = "harvester ID")]
    pub id: String,

    #[arg(long, help = "harvester root; defaults to ~/.assetiweave/harvesters")]
    pub root: Option<String>,

    #[arg(long, default_value_t = 600, help = "maximum run time in seconds")]
    pub timeout: u64,

    #[arg(long, help = "confirm executing this local harvester script")]
    pub yes: bool,

    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "arguments passed to the harvester"
    )]
    pub args: Vec<String>,
}
