use super::catalog::AssetKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod planning;
pub mod read_models;

pub(crate) use planning::{build_deployment_plan, DeploymentPlanCandidate};
pub use read_models::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalMountState {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    Kiro,
    Zcode,
    Qoder,
    Hermes,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetPathRule {
    pub asset_kind: AssetKind,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetProfileDescriptor {
    pub id: String,
    pub name: String,
    pub app_kind_compat: Option<AppKind>,
    pub default_targets: Vec<TargetPathRule>,
    pub supported_kinds: Vec<AssetKind>,
    pub deployment_strategy: DeploymentStrategy,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentActionType {
    Create,
    Update,
    Remove,
    Skip,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuleSet {
    pub kinds: Vec<AssetKind>,
    pub tags: Vec<String>,
    pub groups: Vec<String>,
    pub sources: Vec<String>,
    pub path_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileSafety {
    pub allow_remove: bool,
    pub allow_overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub app_kind: Option<AppKind>,
    #[serde(default)]
    pub target_provider_id: String,
    pub target_paths: Vec<String>,
    pub supported_kinds: Vec<AssetKind>,
    pub deployment_strategy: DeploymentStrategy,
    pub enabled: bool,
    pub include: RuleSet,
    pub exclude: RuleSet,
    pub safety: ProfileSafety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentAction {
    pub id: String,
    pub action_type: DeploymentActionType,
    pub asset_id: Option<String>,
    pub profile_id: String,
    pub source_path: Option<String>,
    pub target_path: String,
    #[serde(default)]
    pub display_source_path: Option<String>,
    #[serde(default)]
    pub display_target_path: Option<String>,
    pub strategy: DeploymentStrategy,
    pub reason: String,
    pub risk: RiskLevel,
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentPlanSummary {
    pub create_count: u32,
    pub update_count: u32,
    pub remove_count: u32,
    pub skip_count: u32,
    pub conflict_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentPlan {
    pub id: String,
    pub created_at: String,
    pub profile_id: Option<String>,
    pub actions: Vec<DeploymentAction>,
    pub summary: DeploymentPlanSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeploymentState {
    pub profile_id: String,
    pub asset_id: String,
    pub target_path: String,
    pub strategy: DeploymentStrategy,
    pub source_hash: String,
    pub deployed_at: String,
    pub managed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetMount {
    pub asset_id: String,
    pub profile_id: String,
    pub enabled: bool,
    pub strategy: DeploymentStrategy,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroupRules {
    pub source_ids: Vec<String>,
    pub relative_path_globs: Vec<String>,
    pub name_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroupIconSvg {
    pub paths: Vec<AssetGroupIconPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_box: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroupIconPath {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_rule: Option<String>,
    pub d: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub asset_kind: AssetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_svg: Option<AssetGroupIconSvg>,
    pub enabled: bool,
    pub sort_order: i32,
    pub rules: AssetGroupRules,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetGroupMemberOrigin {
    Manual,
    Rule,
    ManualAndRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroupResolvedMember {
    pub asset_id: String,
    pub origin: AssetGroupMemberOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetGroupDetail {
    pub group: AssetGroup,
    pub members: Vec<AssetGroupResolvedMember>,
    pub manual_asset_ids: Vec<String>,
}

#[cfg(test)]
#[path = "mounting_tests.rs"]
mod tests;
