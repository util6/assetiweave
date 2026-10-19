use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Inspect and update application settings")]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub command: SettingsCommand,
}

#[derive(Subcommand, Debug)]
pub enum SettingsCommand {
    #[command(about = "Show application settings and managed paths")]
    Show,

    #[command(about = "Save application settings JSON")]
    Save(SettingsSaveArgs),
}

#[derive(Args, Debug)]
pub struct SettingsSaveArgs {
    #[arg(long, help = "application settings JSON object")]
    pub json: String,
}
