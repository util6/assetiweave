use super::Asset;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct AppOverview {
    pub source_count: usize,
    pub asset_count: usize,
    pub profile_count: usize,
    pub last_scan_status: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct CatalogAsset {
    #[serde(flatten)]
    pub asset: Asset,
    pub display_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<GitRepositoryInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_status: Option<SkillBackupAssetStatus>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct GitRepositoryInfo {
    pub root_path: String,
    pub display_root_path: String,
    pub remote_url: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct SkillBackupAssetStatus {
    pub state: SkillBackupState,
    pub backup_path: Option<String>,
    pub display_backup_path: Option<String>,
    pub hidden_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillBackupState {
    BackedUp,
    Downloaded,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillBackupSettings {
    pub root_path: String,
    pub expanded_root_path: String,
    pub default_root_path: String,
    pub display_root_path: String,
    pub display_default_root_path: String,
    pub is_default_root: bool,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillRemoteSource {
    pub asset_id: String,
    pub provider: String,
    pub source_url: String,
    pub repo_url: String,
    pub branch: String,
    pub path: Option<String>,
    pub acquired_at: String,
    pub acquired_tree_sha: Option<String>,
    pub local_content_hash: Option<String>,
    pub last_checked_at: Option<String>,
    pub latest_tree_sha: Option<String>,
    pub status: String,
    pub message: Option<String>,
}
