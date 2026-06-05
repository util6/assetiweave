use crate::targeting::PhysicalMountState;
use assetiweave_core::{
    AppKind, Asset, AssetGroupRules, AssetKind, AssetMount, DeploymentStrategy, ProfileSafety,
    RuleSet, SourceKind, SourceOrigin, SourceScannerKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};

pub(crate) type AppResult<T> = Result<T, String>;

pub(crate) struct AppState {
    pub(crate) db_path: PathBuf,
    pub(crate) lock: Mutex<()>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AppOverview {
    pub(crate) source_count: usize,
    pub(crate) asset_count: usize,
    pub(crate) profile_count: usize,
    pub(crate) last_scan_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CatalogAsset {
    #[serde(flatten)]
    pub(crate) asset: Asset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repository: Option<GitRepositoryInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) backup_status: Option<SkillBackupAssetStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GitRepositoryInfo {
    pub(crate) root_path: String,
    pub(crate) remote_url: Option<String>,
    pub(crate) web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillBackupAssetStatus {
    pub(crate) state: SkillBackupState,
    pub(crate) backup_path: Option<String>,
    pub(crate) hidden_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SkillBackupState {
    BackedUp,
    Downloaded,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillBackupSettings {
    pub(crate) root_path: String,
    pub(crate) expanded_root_path: String,
    pub(crate) default_root_path: String,
    pub(crate) is_default_root: bool,
    pub(crate) exists: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: SourceKind,
    #[serde(alias = "rootPath")]
    pub(crate) root_path: String,
    #[serde(alias = "scannerKind")]
    pub(crate) scanner_kind: Option<SourceScannerKind>,
    #[serde(alias = "sourceOrigin")]
    pub(crate) source_origin: Option<SourceOrigin>,
    #[serde(alias = "repoRoot")]
    pub(crate) repo_root: Option<String>,
    #[serde(alias = "scanRoot")]
    pub(crate) scan_root: Option<String>,
    #[serde(alias = "originAppKind")]
    pub(crate) origin_app_kind: Option<AppKind>,
    #[serde(alias = "includeGlobs")]
    pub(crate) include_globs: Vec<String>,
    #[serde(alias = "excludeGlobs")]
    pub(crate) exclude_globs: Vec<String>,
    #[serde(alias = "defaultKind")]
    pub(crate) default_kind: Option<AssetKind>,
    pub(crate) enabled: bool,
    pub(crate) priority: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TargetProfileInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) app_kind: Option<AppKind>,
    pub(crate) target_paths: Option<Vec<String>>,
    pub(crate) supported_kinds: Option<Vec<AssetKind>>,
    pub(crate) deployment_strategy: Option<DeploymentStrategy>,
    pub(crate) enabled: Option<bool>,
    pub(crate) include: Option<RuleSet>,
    pub(crate) exclude: Option<RuleSet>,
    pub(crate) safety: Option<ProfileSafety>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionResult {
    pub(crate) executed_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) conflict_count: usize,
    pub(crate) errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhysicalMountStateDto {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetMountStatus {
    pub(crate) asset_id: String,
    pub(crate) profile_id: String,
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) state: PhysicalMountStateDto,
    pub(crate) linked_source: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AssetMountObservation {
    pub(crate) asset_id: String,
    pub(crate) profile_id: String,
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) state: PhysicalMountStateDto,
    pub(crate) linked_source: Option<String>,
    pub(crate) observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetMountUpdateResult {
    pub(crate) mount: AssetMount,
    pub(crate) status: AssetMountStatus,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetGroupInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) color: Option<String>,
    pub(crate) enabled: Option<bool>,
    pub(crate) sort_order: Option<i32>,
    pub(crate) rules: Option<AssetGroupRules>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetGroupMountError {
    pub(crate) asset_id: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApplyAssetGroupMountResult {
    pub(crate) group_id: String,
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
    pub(crate) requested_count: usize,
    pub(crate) updated_count: usize,
    pub(crate) error_count: usize,
    pub(crate) mounts: Vec<AssetMount>,
    pub(crate) statuses: Vec<AssetMountStatus>,
    pub(crate) errors: Vec<AssetGroupMountError>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupExclusiveMountInput {
    pub(crate) group_ids: Vec<String>,
    pub(crate) profile_id: String,
    pub(crate) mount_selected: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountItem {
    pub(crate) asset_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountSkippedItem {
    pub(crate) asset_id: String,
    pub(crate) name: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountError {
    pub(crate) asset_id: String,
    pub(crate) name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillGroupExclusiveMountPreview {
    pub(crate) profile_id: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) selected_skill_ids: Vec<String>,
    pub(crate) keep: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) mount: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) unmount: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) skipped: Vec<SkillGroupExclusiveMountSkippedItem>,
    pub(crate) keep_count: usize,
    pub(crate) mount_count: usize,
    pub(crate) unmount_count: usize,
    pub(crate) skipped_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApplySkillGroupExclusiveMountResult {
    #[serde(flatten)]
    pub(crate) preview: SkillGroupExclusiveMountPreview,
    pub(crate) statuses: Vec<AssetMountStatus>,
    pub(crate) errors: Vec<SkillGroupExclusiveMountError>,
}

impl From<PhysicalMountState> for PhysicalMountStateDto {
    fn from(value: PhysicalMountState) -> Self {
        match value {
            PhysicalMountState::Mounted => Self::Mounted,
            PhysicalMountState::NotMounted => Self::NotMounted,
            PhysicalMountState::Conflict => Self::Conflict,
            PhysicalMountState::Broken => Self::Broken,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcut {
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) app_kind: String,
    pub(crate) display_icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) icon_svg: Option<AppShortcutIconSvg>,
    pub(crate) accent_color: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcutIconSvg {
    pub(crate) paths: Vec<AppShortcutIconPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) view_box: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcutIconPath {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clip_rule: Option<String>,
    pub(crate) d: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fill_rule: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationModel {
    pub(crate) active_rail_id: String,
    pub(crate) active_header_tab_id: String,
    pub(crate) active_sub_nav_id: String,
    pub(crate) rail_items: Vec<RailMenuItem>,
    pub(crate) header_tabs: Vec<HeaderTabItem>,
    pub(crate) sub_nav_items: std::collections::BTreeMap<String, Vec<SubNavItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RailMenuItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) icon: String,
    pub(crate) scope: String,
    pub(crate) enabled: bool,
    pub(crate) position: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HeaderTabItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) asset_kind: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubNavItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) route_key: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalizedNavigationLabels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) en: Option<String>,
}
