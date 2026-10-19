use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage normalized conversation records")]
pub struct ConversationArgs {
    #[command(subcommand)]
    pub command: ConversationCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationCommands {
    #[command(about = "Search normalized conversation cards for memory retrieval")]
    Search(ConversationSearchArgs),

    #[command(about = "List normalized conversation sessions")]
    List(ConversationListArgs),

    #[command(about = "Inspect a conversation session with outline and cards")]
    Session(ConversationSessionArgs),

    #[command(about = "Incrementally synchronize conversations from local adapters")]
    Sync(ConversationSyncArgs),

    #[command(about = "Manage conversation adapters")]
    Adapter(ConversationAdapterArgs),

    #[command(about = "Inspect and sync conversation sources")]
    Source(ConversationSourceArgs),

    #[command(about = "Audit and repair normalized conversation data")]
    Data(ConversationDataArgs),

    #[command(about = "Manage session question groups")]
    Question(ConversationQuestionArgs),

    #[command(about = "Locate and read individual conversation blocks")]
    Block(ConversationBlockArgs),

    #[command(about = "Manage conversation content parts")]
    Part(ConversationPartArgs),
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
    #[arg(long, default_value_t = 20, help = "Maximum sessions count")]
    pub limit: usize,

    #[arg(long, default_value_t = 0, help = "Result offset for pagination")]
    pub offset: usize,
}

#[derive(Args, Debug)]
pub struct ConversationSessionArgs {
    #[command(subcommand)]
    pub command: ConversationSessionCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationSessionCommands {
    #[command(about = "Get one session with question groups (accepts short ID prefix)")]
    Get(ConversationSessionGetArgs),

    #[command(
        about = "Get compact session outline tree with consecutive card folding (accepts any ID or 8-hex prefix)"
    )]
    Outline(ConversationSessionOutlineArgs),
}

#[derive(Args, Debug)]
pub struct ConversationSessionGetArgs {
    #[arg(help = "Session identifier (full ID or short 8-hex prefix)")]
    pub session_id: String,
}

#[derive(Args, Debug)]
pub struct ConversationSessionOutlineArgs {
    #[arg(help = "Any identifier (session ID, card ID, turn ID or 8-hex prefix)")]
    pub id: String,
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

#[derive(Args, Debug)]
pub struct ConversationQuestionArgs {
    #[command(subcommand)]
    pub command: ConversationQuestionCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationQuestionCommands {
    #[command(about = "List question groups in a session")]
    List(ConversationQuestionListArgs),

    #[command(about = "Get exact details for one question")]
    Get(ConversationQuestionIdArgs),

    #[command(about = "Merge a question into the previous question")]
    Merge(ConversationQuestionMergeArgs),

    #[command(about = "Split a question into two questions")]
    Split(ConversationQuestionSplitArgs),
}

#[derive(Args, Debug)]
pub struct ConversationQuestionListArgs {
    #[arg(help = "session ID")]
    pub session_id: String,

    #[arg(long, default_value_t = 50, help = "maximum questions")]
    pub limit: usize,

    #[arg(long, default_value_t = 0, help = "offset")]
    pub offset: usize,
}

#[derive(Args, Debug)]
pub struct ConversationQuestionIdArgs {
    #[arg(help = "question ID")]
    pub question_id: String,
}

#[derive(Args, Debug)]
pub struct ConversationQuestionMergeArgs {
    #[arg(help = "question ID to merge")]
    pub question_id: String,

    #[arg(long, help = "preview without merging")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct ConversationQuestionSplitArgs {
    #[arg(help = "question ID to split")]
    pub question_id: String,

    #[arg(long, help = "turn ID that starts the new question")]
    pub before_turn: String,

    #[arg(long, help = "preview without splitting")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct ConversationBlockArgs {
    #[command(subcommand)]
    pub command: ConversationBlockCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationBlockCommands {
    #[command(about = "List block locators for one question")]
    List(ConversationBlockListArgs),

    #[command(about = "Get exact content for one conversation block")]
    Get(ConversationBlockGetArgs),
}

#[derive(Args, Debug)]
pub struct ConversationBlockListArgs {
    #[arg(help = "question ID")]
    pub question_id: String,
}

#[derive(Args, Debug)]
pub struct ConversationBlockGetArgs {
    #[arg(help = "block ID")]
    pub block_id: String,
}

#[derive(Args, Debug)]
pub struct ConversationPartArgs {
    #[command(subcommand)]
    pub command: ConversationPartCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationPartCommands {
    #[command(about = "Manage stored part translations")]
    Translation(ConversationPartTranslationArgs),
}

#[derive(Args, Debug)]
pub struct ConversationPartTranslationArgs {
    #[command(subcommand)]
    pub command: ConversationPartTranslationCommands,
}

#[derive(Subcommand, Debug)]
pub enum ConversationPartTranslationCommands {
    #[command(about = "Update manual translation for a conversation part")]
    Update(ConversationPartTranslationUpdateArgs),
}

#[derive(Args, Debug)]
pub struct ConversationPartTranslationUpdateArgs {
    #[arg(help = "part ID")]
    pub part_id: String,

    #[arg(long, default_value = "session", help = "record kind: session or web")]
    pub record_kind: String,

    #[arg(long, help = "translated content to store")]
    pub text: String,
}
