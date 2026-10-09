use crate::backend::domain::{
    AppKind, AssetGroupIconSvg, AssetGroupRules, AssetKind, DeploymentStrategy, ProfileSafety,
    RuleSet, SourceKind, SourceOrigin, SourceScannerKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: SourceKind,
    #[serde(alias = "rootPath")]
    pub root_path: String,
    #[serde(alias = "scannerKind")]
    pub scanner_kind: Option<SourceScannerKind>,
    #[serde(alias = "sourceOrigin")]
    pub source_origin: Option<SourceOrigin>,
    #[serde(alias = "repoRoot")]
    pub repo_root: Option<String>,
    #[serde(alias = "scanRoot")]
    pub scan_root: Option<String>,
    #[serde(alias = "originAppKind")]
    pub origin_app_kind: Option<AppKind>,
    #[serde(default, alias = "originProviderId")]
    pub origin_provider_id: Option<String>,
    #[serde(alias = "includeGlobs")]
    pub include_globs: Vec<String>,
    #[serde(alias = "excludeGlobs")]
    pub exclude_globs: Vec<String>,
    #[serde(alias = "defaultKind")]
    pub default_kind: Option<AssetKind>,
    pub enabled: bool,
    pub priority: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TargetProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub app_kind: Option<AppKind>,
    #[serde(default, alias = "targetProviderId")]
    pub target_provider_id: Option<String>,
    pub target_paths: Option<Vec<String>>,
    pub supported_kinds: Option<Vec<AssetKind>>,
    pub deployment_strategy: Option<DeploymentStrategy>,
    pub enabled: Option<bool>,
    pub include: Option<RuleSet>,
    pub exclude: Option<RuleSet>,
    pub safety: Option<ProfileSafety>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionResult {
    pub executed_count: usize,
    pub skipped_count: usize,
    pub conflict_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AssetGroupInput {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_svg: Option<AssetGroupIconSvg>,
    pub enabled: Option<bool>,
    pub sort_order: Option<i32>,
    pub rules: Option<AssetGroupRules>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SkillGroupExclusiveMountInput {
    pub group_ids: Vec<String>,
    pub profile_id: String,
    pub mount_selected: bool,
    pub dry_run: bool,
}
