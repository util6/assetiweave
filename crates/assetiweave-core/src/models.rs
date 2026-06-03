use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Prompt,
    Rule,
    Memory,
    Skill,
    Mcp,
    Agent,
    Command,
    Workflow,
    Profile,
    Custom,
    Unclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetFormat {
    Markdown,
    Json,
    Yaml,
    Toml,
    Directory,
    Script,
    Sqlite,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Local,
    GitCheckout,
    Import,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScannerKind {
    Skill,
    Mcp,
    Prompt,
    Rule,
    Mixed,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    GitRepo,
    LocalFolder,
    AppTarget,
    AppLocal,
    AssetiweaveLibrary,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    Codex,
    Claude,
    Cursor,
    #[serde(rename = "opencode", alias = "open_code")]
    OpenCode,
    Gemini,
    Antigravity,
    #[serde(rename = "openclaw", alias = "open_claw")]
    OpenClaw,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStrategy {
    #[serde(alias = "symlink")]
    SymlinkToSource,
    #[serde(alias = "copy")]
    CopyToTarget,
    Render,
    Append,
    ConfigMerge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentActionType {
    Create,
    Update,
    Remove,
    Skip,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub root_path: String,
    pub scanner_kind: SourceScannerKind,
    pub source_origin: SourceOrigin,
    pub repo_root: Option<String>,
    pub scan_root: String,
    pub origin_app_kind: Option<AppKind>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub default_kind: Option<AssetKind>,
    pub enabled: bool,
    pub priority: i32,
    pub last_scanned_at: Option<String>,
    pub last_scan_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub kind: AssetKind,
    pub format: AssetFormat,
    pub relative_path: String,
    pub absolute_path: String,
    pub entry_file: Option<String>,
    pub description: Option<String>,
    pub content_hash: Option<String>,
    pub discovered_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataOverlay {
    pub asset_id: String,
    pub display_name: Option<String>,
    pub kind_override: Option<AssetKind>,
    pub tags: Vec<String>,
    pub groups: Vec<String>,
    pub enabled: bool,
    pub notes: Option<String>,
    pub explicit_profiles_include: Vec<String>,
    pub explicit_profiles_exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    pub kinds: Vec<AssetKind>,
    pub tags: Vec<String>,
    pub groups: Vec<String>,
    pub sources: Vec<String>,
    pub path_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSafety {
    pub allow_remove: bool,
    pub allow_overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetProfile {
    pub id: String,
    pub name: String,
    pub app_kind: AppKind,
    pub target_paths: Vec<String>,
    pub supported_kinds: Vec<AssetKind>,
    pub deployment_strategy: DeploymentStrategy,
    pub enabled: bool,
    pub include: RuleSet,
    pub exclude: RuleSet,
    pub safety: ProfileSafety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentAction {
    pub id: String,
    pub action_type: DeploymentActionType,
    pub asset_id: Option<String>,
    pub profile_id: String,
    pub source_path: Option<String>,
    pub target_path: String,
    pub strategy: DeploymentStrategy,
    pub reason: String,
    pub risk: RiskLevel,
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlanSummary {
    pub create_count: u32,
    pub update_count: u32,
    pub remove_count: u32,
    pub skip_count: u32,
    pub conflict_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentPlan {
    pub id: String,
    pub created_at: String,
    pub profile_id: Option<String>,
    pub actions: Vec<DeploymentAction>,
    pub summary: DeploymentPlanSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentState {
    pub profile_id: String,
    pub asset_id: String,
    pub target_path: String,
    pub strategy: DeploymentStrategy,
    pub source_hash: String,
    pub deployed_at: String,
    pub managed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMount {
    pub asset_id: String,
    pub profile_id: String,
    pub enabled: bool,
    pub strategy: DeploymentStrategy,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGroupRules {
    pub source_ids: Vec<String>,
    pub relative_path_globs: Vec<String>,
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub asset_kind: AssetKind,
    pub enabled: bool,
    pub sort_order: i32,
    pub rules: AssetGroupRules,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetGroupMemberOrigin {
    Manual,
    Rule,
    ManualAndRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGroupResolvedMember {
    pub asset_id: String,
    pub origin: AssetGroupMemberOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetGroupDetail {
    pub group: AssetGroup,
    pub members: Vec<AssetGroupResolvedMember>,
    pub manual_asset_ids: Vec<String>,
}

pub fn stable_asset_id(source_id: &str, relative_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_id.as_bytes());
    hasher.update(b":");
    hasher.update(relative_path.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_kind_uses_frontend_compatible_names() {
        assert_eq!(serde_json::to_string(&AppKind::OpenCode).unwrap(), "\"opencode\"");
        assert_eq!(serde_json::to_string(&AppKind::OpenClaw).unwrap(), "\"openclaw\"");
        assert_eq!(
            serde_json::from_str::<AppKind>("\"open_code\"").unwrap(),
            AppKind::OpenCode
        );
        assert_eq!(
            serde_json::from_str::<AppKind>("\"open_claw\"").unwrap(),
            AppKind::OpenClaw
        );
    }

    #[test]
    fn stable_asset_id_is_repeatable() {
        assert_eq!(
            stable_asset_id("source-a", "skills/foo/SKILL.md"),
            stable_asset_id("source-a", "skills/foo/SKILL.md")
        );
    }

    #[test]
    fn stable_asset_id_depends_on_source() {
        assert_ne!(
            stable_asset_id("source-a", "skills/foo/SKILL.md"),
            stable_asset_id("source-b", "skills/foo/SKILL.md")
        );
    }
}
