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
