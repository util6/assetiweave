use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage progressive Memory and persistent Recall")]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryCommand {
    #[command(about = "Inspect recent memory state and snapshot")]
    Recent(MemoryRecentArgs),

    #[command(about = "Resolve compiled Memory context")]
    Context(MemoryContextArgs),

    #[command(about = "Inspect project Memory")]
    Project(MemoryProjectArgs),

    #[command(about = "Queue a Memory rebuild")]
    Rebuild(MemoryRebuildArgs),

    #[command(about = "Inspect and control Memory background tasks")]
    Task(MemoryTaskArgs),

    #[command(about = "Search and continue persistent Recall sessions")]
    Recall(MemoryRecallArgs),

    #[command(about = "Inspect strong-typed memory state")]
    State(MemoryStateArgs),

    #[command(about = "Commit topic evolution to project memory")]
    Commit(MemoryCommitArgs),

    #[command(about = "Check or advance session watermarks")]
    Watermark(MemoryWatermarkArgs),
}

#[derive(Args, Debug)]
pub struct MemoryRecentArgs {
    #[command(subcommand)]
    pub command: MemoryRecentCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryRecentCommand {
    #[command(about = "Get recent memory state and snapshot")]
    Get(MemoryRecentGetArgs),
}

#[derive(Args, Debug)]
pub struct MemoryRecentGetArgs {
    #[arg(
        long,
        help = "specific date snapshot, e.g. 2026-10-26 (defaults to latest)"
    )]
    pub date: Option<String>,
}

#[derive(Args, Debug)]
pub struct MemoryContextArgs {
    #[command(subcommand)]
    pub command: MemoryContextCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryContextCommand {
    #[command(about = "Resolve the compiled Memory context")]
    Resolve(MemoryContextResolveArgs),
}

#[derive(Args, Debug)]
pub struct MemoryContextResolveArgs {
    #[arg(long, help = "optional app filter")]
    pub app: Option<String>,

    #[arg(long, help = "optional source filter")]
    pub source: Option<String>,

    #[arg(long, help = "optional project path filter")]
    pub project: Option<String>,

    #[arg(long, help = "optional session ID filter")]
    pub session: Option<String>,

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,

    #[arg(long, help = "optional relevance query")]
    pub query: Option<String>,

    #[arg(long, default_value_t = 2000, help = "maximum context tokens")]
    pub token_budget: usize,
}

#[derive(Args, Debug)]
pub struct MemoryProjectArgs {
    #[command(subcommand)]
    pub command: MemoryProjectCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryProjectCommand {
    #[command(about = "Get the project Memory projection")]
    Get(MemoryProjectGetArgs),
}

#[derive(Args, Debug)]
pub struct MemoryProjectGetArgs {
    #[arg(help = "project path")]
    pub project_path: Option<String>,

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,
}

#[derive(Args, Debug)]
pub struct MemoryRebuildArgs {
    #[arg(long, help = "project path scope")]
    pub project: Option<String>,

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,

    #[arg(long, help = "rebuild target: recent, project, global, or all")]
    pub target: Option<String>,

    #[arg(
        long,
        default_value = "manual",
        help = "rebuild reason: manual, migration, or projection_repair"
    )]
    pub reason: String,

    #[arg(long, help = "force rebuild and clean all storage / watermarks")]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct MemoryTaskArgs {
    #[command(subcommand)]
    pub command: MemoryTaskCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryTaskCommand {
    #[command(about = "List Memory tasks")]
    List(MemoryTaskListArgs),

    #[command(about = "Get a Memory task")]
    Get(MemoryTaskTargetArgs),

    #[command(about = "Cancel a Memory task")]
    Cancel(MemoryTaskTargetArgs),

    #[command(about = "Retry a failed Memory task")]
    Retry(MemoryTaskTargetArgs),
}

#[derive(Args, Debug)]
pub struct MemoryTaskListArgs {
    #[arg(long, help = "only active tasks")]
    pub active_only: bool,
}

#[derive(Args, Debug)]
pub struct MemoryTaskTargetArgs {
    #[arg(help = "task ID")]
    pub task_id: String,
}

#[derive(Args, Debug)]
pub struct MemoryRecallArgs {
    #[command(subcommand)]
    pub command: MemoryRecallCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryRecallCommand {
    #[command(about = "Search Memory source content")]
    Search(MemoryRecallSearchArgs),

    #[command(about = "Manage persistent Recall sessions")]
    Session(MemoryRecallSessionArgs),

    #[command(about = "Send or cancel persistent Recall turns")]
    Turn(MemoryRecallTurnArgs),
}

#[derive(Args, Debug)]
pub struct MemoryRecallSearchArgs {
    #[arg(long, help = "search query")]
    pub query: String,

    #[arg(long, help = "app or adapter scope")]
    pub app: Option<String>,

    #[arg(long, help = "source scope")]
    pub source: Option<String>,

    #[arg(long, help = "project path scope")]
    pub project: Option<String>,

    #[arg(long, help = "session scope")]
    pub session: Option<String>,

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,

    #[arg(long, default_value_t = 24, help = "maximum hits")]
    pub limit: usize,

    #[arg(long, default_value_t = 0, help = "pagination offset")]
    pub offset: usize,
}

#[derive(Args, Debug)]
pub struct MemoryRecallSessionArgs {
    #[command(subcommand)]
    pub command: MemoryRecallSessionCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryRecallSessionCommand {
    #[command(about = "Create a Recall session")]
    Create(MemoryRecallSessionCreateArgs),

    #[command(about = "Get a Recall session")]
    Get(MemoryRecallSessionGetArgs),
}

#[derive(Args, Debug)]
pub struct MemoryRecallSessionCreateArgs {
    #[arg(long, help = "app or adapter scope")]
    pub app: Option<String>,

    #[arg(long, help = "source scope")]
    pub source: Option<String>,

    #[arg(long, help = "project path scope")]
    pub project: Option<String>,

    #[arg(long, help = "session scope")]
    pub session: Option<String>,

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,
}

#[derive(Args, Debug)]
pub struct MemoryRecallSessionGetArgs {
    #[arg(help = "session ID")]
    pub session_id: String,
}

#[derive(Args, Debug)]
pub struct MemoryRecallTurnArgs {
    #[command(subcommand)]
    pub command: MemoryRecallTurnCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryRecallTurnCommand {
    #[command(about = "Queue a Recall turn")]
    Send(MemoryRecallTurnSendArgs),

    #[command(about = "Cancel a Recall turn")]
    Cancel(MemoryRecallTurnCancelArgs),
}

#[derive(Args, Debug)]
pub struct MemoryRecallTurnSendArgs {
    #[arg(help = "session ID")]
    pub session_id: String,

    #[arg(long, help = "Recall question")]
    pub query: String,
}

#[derive(Args, Debug)]
pub struct MemoryRecallTurnCancelArgs {
    #[arg(help = "turn ID")]
    pub turn_id: String,
}

#[derive(Args, Debug)]
pub struct MemoryStateArgs {
    #[command(subcommand)]
    pub command: MemoryStateCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryStateCommand {
    #[command(about = "Get memory state (project state or recent state)")]
    Get(MemoryStateGetArgs),
}

#[derive(Args, Debug)]
pub struct MemoryStateGetArgs {
    #[arg(long, help = "project slug (if omitted, gets recent state)")]
    pub project: Option<String>,

    #[arg(
        long,
        default_value_t = false,
        help = "print raw state JSON un-enveloped"
    )]
    pub raw: bool,
}

#[derive(Args, Debug)]
pub struct MemoryCommitArgs {
    #[arg(long, help = "project slug (required if not using --payload)")]
    pub project: Option<String>,

    #[arg(long, help = "topic ID")]
    pub topic: Option<String>,

    #[arg(long, help = "optional human-readable topic name")]
    pub topic_name: Option<String>,

    #[arg(long, help = "optional parent node ID")]
    pub parent_id: Option<String>,

    #[arg(
        long,
        help = "optional parent node new status (continued or completed)"
    )]
    pub parent_status: Option<String>,

    #[arg(long, help = "node turn summary")]
    pub summary: Option<String>,

    #[arg(long, help = "source session full UUID")]
    pub session_id: Option<String>,

    #[arg(long, help = "source session short ID")]
    pub short_id: Option<String>,

    #[arg(long, help = "turn index (1-based)")]
    pub turn: Option<u32>,

    #[arg(
        long,
        default_value = "completed",
        help = "new node status (completed, continued, unexplored)"
    )]
    pub status: String,

    #[arg(
        long,
        help = "full TopicEvolutionCommitPayload as JSON string, or @filepath"
    )]
    pub payload: Option<String>,

    #[arg(
        long,
        help = "path to JSON file containing TopicEvolutionCommitPayload"
    )]
    pub file: Option<std::path::PathBuf>,
}

#[derive(Args, Debug)]
pub struct MemoryWatermarkArgs {
    #[command(subcommand)]
    pub command: MemoryWatermarkCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryWatermarkCommand {
    #[command(about = "Check session watermark")]
    Check(MemoryWatermarkCheckArgs),

    #[command(about = "Advance session watermark")]
    Advance(MemoryWatermarkAdvanceArgs),
}

#[derive(Args, Debug)]
pub struct MemoryWatermarkCheckArgs {
    #[arg(long, help = "project slug")]
    pub project: String,

    #[arg(long, help = "session ID")]
    pub session_id: String,
}

#[derive(Args, Debug)]
pub struct MemoryWatermarkAdvanceArgs {
    #[arg(long, help = "project slug")]
    pub project: String,

    #[arg(long, help = "session ID")]
    pub session_id: String,

    #[arg(long, help = "turn number to advance to")]
    pub turn: u32,
}
