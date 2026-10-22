use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(
    about = "Manage normalized conversation records",
    after_long_help = crate::affordance::conversation::help("conversation")
)]
pub struct ConversationArgs {
    #[command(subcommand)]
    pub command: ConversationCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationCommands {
    #[command(
        about = "Search normalized conversation cards for memory retrieval",
        after_long_help = crate::affordance::conversation::help("search")
    )]
    Search(ConversationSearchArgs),

    #[command(
        about = "List normalized conversation sessions",
        after_long_help = crate::affordance::conversation::help("list")
    )]
    List(ConversationListArgs),

    #[command(
        about = "Inspect conversation structure with compact columnar outline (accepts session, turn, or card ID)",
        after_long_help = crate::affordance::conversation::help("outline")
    )]
    Outline(ConversationOutlineArgs),

    #[command(
        about = "Inspect and read individual conversation cards",
        after_long_help = crate::affordance::conversation::help("card")
    )]
    Card(ConversationCardArgs),

    #[command(
        about = "Incrementally synchronize conversations from local adapters",
        after_long_help = crate::affordance::conversation::help("sync")
    )]
    Sync(ConversationSyncArgs),

    #[command(
        about = "Manage conversation adapters",
        after_long_help = crate::affordance::conversation::help("adapter")
    )]
    Adapter(ConversationAdapterArgs),

    #[command(
        about = "Inspect and sync conversation sources",
        after_long_help = crate::affordance::conversation::help("source")
    )]
    Source(ConversationSourceArgs),

    #[command(
        about = "Audit and repair normalized conversation data",
        after_long_help = crate::affordance::conversation::help("data")
    )]
    Data(ConversationDataArgs),
}

#[derive(Args, Debug)]
pub struct ConversationSearchArgs {
    #[arg(long, help = "Search keyword or natural language query")]
    pub query: Option<String>,

    #[arg(long, help = "Filter by record kinds (e.g. card, turn, session)")]
    pub kinds: Vec<String>,

    #[arg(long, default_value_t = 20, help = "Maximum search results count")]
    pub limit: usize,

    #[arg(long, default_value_t = 0, help = "Result offset for pagination")]
    pub offset: usize,

    #[arg(long, help = "Search query starting timestamp filter")]
    pub since: Option<String>,

    #[arg(long, help = "Search query ending timestamp filter")]
    pub until: Option<String>,

    #[arg(long, help = "Format results in timeline order")]
    pub timeline: bool,
}

#[derive(Args, Debug)]
pub struct ConversationListArgs {
    #[arg(long, default_value_t = 15, help = "Maximum sessions count")]
    pub limit: usize,

    #[arg(long, default_value_t = 0, help = "Result offset for pagination")]
    pub offset: usize,

    #[arg(
        long,
        num_args = 1..=2,
        value_name = "TIME",
        help = "Time filter: single number in hours (e.g. 24), start time to now (e.g. '2026-10-13'), or two endpoints (e.g. 2026-10-13 2026-10-20)"
    )]
    pub time: Vec<String>,

    #[arg(long, help = "Time window filter (e.g. 24, 24h, 48h)")]
    pub window: Option<String>,

    #[arg(
        long,
        help = "Start time filter (e.g. 24h, yesterday, today, or RFC3339)"
    )]
    pub since: Option<String>,

    #[arg(
        long,
        help = "Time range filter like 'a..b' or 'a - b' (e.g. '2026-10-13..2026-10-16', 'yesterday..today')"
    )]
    pub range: Option<String>,

    #[arg(long, help = "End time filter")]
    pub until: Option<String>,

    #[arg(long, help = "Filter by project path or name")]
    pub project: Option<String>,

    #[arg(long, help = "Filter by current project directory")]
    pub current_project: bool,

    #[arg(long, help = "Filter by adapter id")]
    pub adapter: Option<String>,

    #[arg(long, help = "Search query filter")]
    pub query: Option<String>,
}

#[derive(Args, Debug)]
pub struct ConversationOutlineArgs {
    #[arg(help = "Any identifier (session ID, turn ID, card ID or 8-hex prefix)")]
    pub id: String,
}

#[derive(Args, Debug)]
pub struct ConversationCardArgs {
    #[command(subcommand)]
    pub command: ConversationCardCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationCardCommands {
    #[command(
        about = "Get exact content for one conversation card",
        after_long_help = crate::affordance::conversation::help("card get")
    )]
    Get(ConversationCardGetArgs),
}

#[derive(Args, Debug)]
pub struct ConversationCardGetArgs {
    #[arg(help = "Card ID or 8-hex prefix")]
    pub card_id: String,

    #[arg(
        long,
        help = "Output raw content directly to stdout without formatting"
    )]
    pub raw: bool,
}

#[derive(Args, Debug)]
pub struct ConversationSyncArgs {
    #[arg(long, help = "Optional adapter identifier to sync specifically")]
    pub adapter: Option<String>,

    #[arg(long, help = "Optional source directory to sync")]
    pub source: Option<String>,

    #[arg(long, help = "Force full re-synchronization")]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct ConversationAdapterArgs {
    #[command(subcommand)]
    pub command: ConversationAdapterCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationAdapterCommands {
    #[command(about = "List installed conversation adapters and their health")]
    List,

    #[command(about = "Inspect one conversation adapter")]
    Inspect(ConversationAdapterInspectArgs),
}

#[derive(Args, Debug)]
pub struct ConversationAdapterInspectArgs {
    #[arg(help = "Adapter ID")]
    pub adapter_id: String,
}

#[derive(Args, Debug)]
pub struct ConversationSourceArgs {
    #[command(subcommand)]
    pub command: ConversationSourceCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationSourceCommands {
    #[command(about = "List registered conversation sources")]
    List,

    #[command(about = "Synchronize one conversation source")]
    Sync(ConversationSourceSyncArgs),
}

#[derive(Args, Debug)]
pub struct ConversationSourceSyncArgs {
    #[arg(help = "Source ID")]
    pub source_id: String,

    #[arg(long, help = "Perform full reparse")]
    pub full: bool,
}

#[derive(Args, Debug)]
pub struct ConversationDataArgs {
    #[command(subcommand)]
    pub command: ConversationDataCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationDataCommands {
    #[command(about = "Audit conversation data and derived indexes")]
    Audit(ConversationDataAuditArgs),

    #[command(about = "Preview or apply safe conversation data repairs")]
    Repair(ConversationDataRepairArgs),

    #[command(about = "Preview or restore a conversation maintenance backup")]
    Rollback(ConversationDataRollbackArgs),
}

#[derive(Args, Debug)]
pub struct ConversationDataAuditArgs {
    #[arg(long, help = "source id filter")]
    pub source: Option<String>,

    #[arg(long, help = "record kind: session or web")]
    pub record_kind: Option<String>,
}

#[derive(Args, Debug)]
pub struct ConversationDataRepairArgs {
    #[arg(long, help = "source id filter")]
    pub source: Option<String>,

    #[arg(long, help = "record kind: session or web")]
    pub record_kind: Option<String>,

    #[arg(long, help = "preview repairs without writing")]
    pub dry_run: bool,

    #[arg(long, help = "resync selected sources before applying repairs")]
    pub resync: bool,

    #[arg(long, help = "confirm applying repairs")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct ConversationDataRollbackArgs {
    #[arg(long, help = "backup path to restore from")]
    pub backup_path: String,

    #[arg(long, help = "preview rollback without modifying database")]
    pub dry_run: bool,

    #[arg(long, help = "confirm rollback")]
    pub yes: bool,
}
