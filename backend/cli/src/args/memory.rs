use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage progressive Memory and topic tree states")]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommand,
}

#[derive(Subcommand, Debug)]
pub enum MemoryCommand {
    #[command(about = "Inspect recent memory state and snapshot")]
    Recent(MemoryRecentArgs),

    #[command(about = "Inspect project Memory")]
    Project(MemoryProjectArgs),

    #[command(about = "Inspect strong-typed memory state")]
    State(MemoryStateArgs),

    #[command(about = "Commit topic evolution to project memory")]
    Commit(MemoryCommitArgs),

    #[command(about = "Check or advance session watermarks")]
    Watermark(MemoryWatermarkArgs),

    #[command(about = "Update memory node status")]
    Status(MemoryStatusArgs),
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

    #[arg(long, help = "use current working directory as project scope")]
    pub current_project: bool,

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

    #[arg(long = "unexplored", help = "unexplored sibling node summaries")]
    pub unexplored: Vec<String>,

    #[arg(long, help = "recent window duration to refresh (e.g. 48h, 24h)")]
    pub window: Option<String>,
}

#[derive(Args, Debug)]
pub struct MemoryStatusArgs {
    #[arg(help = "node ID (e.g. 73c20273#T1)")]
    pub node_id: String,

    #[arg(help = "new node status: completed, dismissed, unexplored, continued")]
    pub status: String,

    #[arg(long, help = "optional project path or slug")]
    pub project: Option<String>,

    #[arg(
        long,
        default_value = "48h",
        help = "recent window duration (default: 48h)"
    )]
    pub window: Option<String>,
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
