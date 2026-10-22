use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommands,
}

#[derive(Subcommand, Debug)]
pub enum SkillCommands {
    #[command(about = "List installed and available skills")]
    List,

    #[command(alias = "b", about = "Inspect built-in skills")]
    Builtin(SkillBuiltinArgs),

    #[command(about = "Read raw markdown content of a built-in skill")]
    Read(SkillReadArgs),

    #[command(about = "Mount a skill to a target profile")]
    Mount(SkillMountArgs),

    #[command(about = "Unmount a skill from a target profile")]
    Unmount(SkillUnmountArgs),

    #[command(about = "Import a local skill directory into the backup library")]
    Import(SkillImportArgs),

    #[command(about = "Back up an indexed skill into the library")]
    Backup(SkillBackupArgs),

    #[command(about = "Delete a skill from the library")]
    Delete(SkillDeleteArgs),

    #[command(about = "Manage skill groups")]
    Group(SkillGroupArgs),

    #[command(about = "Search internet providers for skills")]
    Search(SkillSearchArgs),

    #[command(about = "Download and import a skill candidate")]
    Acquire(SkillAcquireArgs),

    #[command(about = "Inspect acquired skill remote sources")]
    Remote(SkillRemoteArgs),
}

#[derive(Args, Debug)]
pub struct SkillBuiltinArgs {
    #[command(subcommand)]
    pub command: SkillBuiltinCommands,
}

#[derive(Subcommand, Debug)]
pub enum SkillBuiltinCommands {
    #[command(about = "List built-in skills with descriptions")]
    List,

    #[command(about = "Read raw markdown content of a built-in skill")]
    Read(SkillReadArgs),
}

#[derive(Args, Debug)]
pub struct SkillReadArgs {
    #[arg(help = "Built-in skill name or ID")]
    pub name: String,
}

#[derive(Args, Debug)]
pub struct SkillSearchArgs {
    #[arg(long, help = "skill search query")]
    pub query: String,

    #[arg(long, default_value = "github", help = "search provider")]
    pub provider: String,

    #[arg(long, default_value_t = 10, help = "maximum candidate count")]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct SkillAcquireArgs {
    #[arg(long, help = "GitHub repository, tree, or blob URL")]
    pub url: String,

    #[arg(long, help = "Git branch override")]
    pub branch: Option<String>,

    #[arg(long, help = "skill directory path inside the repository")]
    pub path: Option<String>,

    #[arg(long, help = "imported skill name")]
    pub name: Option<String>,

    #[arg(long, help = "preview without cloning or importing")]
    pub dry_run: bool,

    #[arg(long, help = "confirm download and import")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct SkillRemoteArgs {
    #[command(subcommand)]
    pub command: SkillRemoteCommands,
}

#[derive(Subcommand, Debug)]
pub enum SkillRemoteCommands {
    #[command(about = "List acquired skill remote sources")]
    List,

    #[command(about = "Check acquired skill remote sources for updates")]
    Check,
}

#[derive(Args, Debug)]
pub struct SkillMountArgs {
    #[arg(help = "Skill asset identifier or name")]
    pub asset_ref: String,

    #[arg(long, help = "Target application profile id")]
    pub profile: String,

    #[arg(long, help = "Preview mount plan without creating symlinks")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct SkillUnmountArgs {
    #[arg(help = "Skill asset identifier or name")]
    pub asset_ref: String,

    #[arg(long, help = "Target application profile id")]
    pub profile: String,

    #[arg(long, help = "Preview unmount plan without removing symlinks")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct SkillImportArgs {
    #[arg(long, help = "Directory path containing SKILL.md")]
    pub from: String,

    #[arg(long, help = "Optional custom name for imported skill")]
    pub name: Option<String>,

    #[arg(long, help = "Preview import without copying files")]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct SkillBackupArgs {
    #[arg(help = "Asset identifier to back up")]
    pub asset_id: String,
}

#[derive(Args, Debug)]
pub struct SkillDeleteArgs {
    #[arg(help = "Asset identifier or name to delete")]
    pub asset_ref: String,

    #[arg(long, help = "Optional target profile id")]
    pub profile: Option<String>,

    #[arg(long, help = "Preview deletion without removing files")]
    pub dry_run: bool,

    #[arg(long, help = "Confirm destructive deletion without prompt")]
    pub yes: bool,

    #[arg(long, help = "Unmount from profiles before deleting")]
    pub unmount: bool,
}

#[derive(Args, Debug)]
pub struct SkillGroupArgs {
    #[command(subcommand)]
    pub command: SkillGroupCommands,
}

#[derive(Subcommand, Debug)]
pub enum SkillGroupCommands {
    #[command(about = "List skill groups")]
    List,

    #[command(about = "Show details of a skill group")]
    Show(SkillGroupShowArgs),

    #[command(about = "Create a new skill group")]
    Create(SkillGroupCreateArgs),

    #[command(about = "Delete a skill group")]
    Delete(SkillGroupDeleteArgs),

    #[command(about = "Mount all skills in a group to a target profile")]
    Mount(SkillGroupMountArgs),

    #[command(about = "Unmount all skills in a group from a target profile")]
    Unmount(SkillGroupUnmountArgs),
}

#[derive(Args, Debug)]
pub struct SkillGroupShowArgs {
    #[arg(help = "Skill group identifier")]
    pub group_id: String,
}

#[derive(Args, Debug)]
pub struct SkillGroupCreateArgs {
    #[arg(long, help = "Group display name")]
    pub name: String,

    #[arg(long, help = "Group description")]
    pub description: Option<String>,
}

#[derive(Args, Debug)]
pub struct SkillGroupDeleteArgs {
    #[arg(help = "Skill group identifier to delete")]
    pub group_id: String,

    #[arg(long, help = "Confirm deletion without prompt")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct SkillGroupMountArgs {
    #[arg(help = "Skill group identifier")]
    pub group_id: String,

    #[arg(long, help = "Target application profile id")]
    pub profile: String,
}

#[derive(Args, Debug)]
pub struct SkillGroupUnmountArgs {
    #[arg(help = "Skill group identifier")]
    pub group_id: String,

    #[arg(long, help = "Target application profile id")]
    pub profile: String,
}
