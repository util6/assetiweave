use super::types::{AssetGroupInput, SkillGroupExclusiveMountInput, TargetProfileInput};
use crate::backend::domain::{AssetGroup, DeploymentPlan, DeploymentStrategy, TargetProfile};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfileIdParams {
    #[serde(alias = "profileId")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AssetProfileParams {
    #[serde(alias = "assetId")]
    pub asset_id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetAssetMountParams {
    #[serde(alias = "assetId")]
    pub asset_id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    pub enabled: bool,
    pub strategy: Option<DeploymentStrategy>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplySkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub group_id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateProfileParams {
    pub input: TargetProfileInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateProfileParams {
    pub profile: TargetProfile,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSkillGroupParams {
    pub input: AssetGroupInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSkillGroupParams {
    pub group: AssetGroup,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupIdParams {
    #[serde(alias = "groupId")]
    pub group_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetSkillGroupManualMembersParams {
    #[serde(alias = "groupId")]
    pub group_id: String,
    #[serde(alias = "assetIds")]
    pub asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillGroupExclusiveMountParams {
    pub input: SkillGroupExclusiveMountInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecutePlanParams {
    pub plan: DeploymentPlan,
    #[serde(alias = "actionIds")]
    pub action_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub group_id: String,
    #[serde(alias = "profileId")]
    pub profile_id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}
