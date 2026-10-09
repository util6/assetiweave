use super::{AssetMount, PhysicalMountState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalMountStateDto {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
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

#[derive(Debug, Clone, Serialize)]
pub struct AssetMountStatus {
    pub asset_id: String,
    pub profile_id: String,
    pub target_dir: String,
    pub target_path: String,
    pub display_target_dir: String,
    pub display_target_path: String,
    pub display_linked_source: Option<String>,
    pub state: PhysicalMountStateDto,
    pub linked_source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssetMountObservation {
    pub asset_id: String,
    pub profile_id: String,
    pub target_dir: String,
    pub target_path: String,
    pub state: PhysicalMountStateDto,
    pub linked_source: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetMountUpdateResult {
    pub mount: AssetMount,
    pub status: AssetMountStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetGroupMountError {
    pub asset_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyAssetGroupMountResult {
    pub group_id: String,
    pub profile_id: String,
    pub enabled: bool,
    pub requested_count: usize,
    pub updated_count: usize,
    pub error_count: usize,
    pub mounts: Vec<AssetMount>,
    pub statuses: Vec<AssetMountStatus>,
    pub errors: Vec<AssetGroupMountError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkillGroupExclusiveMountItem {
    pub asset_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkillGroupExclusiveMountSkippedItem {
    pub asset_id: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkillGroupExclusiveMountError {
    pub asset_id: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillGroupExclusiveMountPreview {
    pub profile_id: String,
    pub group_ids: Vec<String>,
    pub selected_skill_ids: Vec<String>,
    pub keep: Vec<SkillGroupExclusiveMountItem>,
    pub mount: Vec<SkillGroupExclusiveMountItem>,
    pub unmount: Vec<SkillGroupExclusiveMountItem>,
    pub skipped: Vec<SkillGroupExclusiveMountSkippedItem>,
    pub keep_count: usize,
    pub mount_count: usize,
    pub unmount_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplySkillGroupExclusiveMountResult {
    #[serde(flatten)]
    pub preview: SkillGroupExclusiveMountPreview,
    pub statuses: Vec<AssetMountStatus>,
    pub errors: Vec<SkillGroupExclusiveMountError>,
}
