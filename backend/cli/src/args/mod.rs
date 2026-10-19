pub mod agent;
pub mod api;
pub mod asset;
pub mod completion;
pub mod config;
pub mod conversation;
pub mod doctor;
pub mod harvester;
pub mod memory;
pub mod profile;
pub mod schema;
pub mod settings;
pub mod skill;
pub mod source;
pub mod tenant;
pub mod update;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "aiwc",
    about = "AssetIWeave native terminal workspace client",
    version,
    propagate_version = true
)]
pub struct CliArgs {
    #[arg(
        long,
        global = true,
        value_enum,
        default_value_t = OutputFormat::Table,
        help = "Output presentation format"
    )]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(
        alias = "v",
        alias = "ver",
        about = "Show CLI and backend runtime compatibility versions"
    )]
    Version(VersionArgs),

    #[command(
        alias = "ov",
        alias = "o",
        about = "Show AssetIWeave catalog overview metrics"
    )]
    Overview,

    #[command(alias = "sk", about = "Manage skills and mounting relationships")]
    Skill(skill::SkillArgs),

    #[command(alias = "as", about = "List and inspect catalog assets")]
    Asset(asset::AssetArgs),

    #[command(
        alias = "src",
        about = "Manage registered asset sources and trigger scans"
    )]
    Source(source::SourceArgs),

    #[command(
        alias = "t",
        alias = "tn",
        about = "Manage tenants and tenant switching"
    )]
    Tenant(tenant::TenantArgs),

    #[command(
        alias = "p",
        alias = "prof",
        alias = "target-profile",
        about = "List supported application target profiles"
    )]
    Profile(profile::ProfileArgs),

    #[command(
        alias = "c",
        alias = "conv",
        about = "Search and synchronize normalized AI conversation records"
    )]
    Conversation(conversation::ConversationArgs),

    #[command(
        alias = "m",
        alias = "mem",
        about = "Manage progressive Memory and persistent Recall"
    )]
    Memory(memory::MemoryArgs),

    #[command(
        alias = "ag",
        about = "Manage Agent Market installations and runtime bindings"
    )]
    Agent(agent::AgentArgs),

    #[command(alias = "hv", about = "Manage local web conversation harvesters")]
    Harvester(harvester::HarvesterArgs),

    #[command(alias = "st", about = "Inspect application settings")]
    Settings(settings::SettingsArgs),

    #[command(alias = "doc", about = "Run local CLI and engine diagnostics")]
    Doctor(doctor::DoctorArgs),

    #[command(alias = "cfg", about = "Inspect local CLI configuration")]
    Config(config::ConfigArgs),

    #[command(alias = "up", alias = "u", about = "Check for AssetIWeave CLI updates")]
    Update(update::UpdateArgs),

    #[command(alias = "comp", about = "Generate shell completion scripts")]
    Completion(completion::CompletionArgs),

    #[command(alias = "sch", about = "Inspect engine method schemas")]
    Schema(schema::SchemaArgs),

    #[command(alias = "rpc", about = "Call engine methods directly")]
    Api(api::ApiArgs),
}

#[derive(Args, Debug)]
pub struct VersionArgs {
    #[arg(long, help = "Check remote updater manifest for updates")]
    pub check_updates: bool,
}
