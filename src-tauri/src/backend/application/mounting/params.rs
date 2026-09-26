use super::types::{AssetGroupInput, SkillGroupExclusiveMountInput, TargetProfileInput};
use crate::backend::domain::{AssetGroup, DeploymentPlan, DeploymentStrategy, TargetProfile};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ProfileIdParams {
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct IdParams {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetProfileParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetAssetMountParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
    pub(crate) strategy: Option<DeploymentStrategy>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ApplySkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateProfileParams {
    pub(crate) input: TargetProfileInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateProfileParams {
    pub(crate) profile: TargetProfile,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateSkillGroupParams {
    pub(crate) input: AssetGroupInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSkillGroupParams {
    pub(crate) group: AssetGroup,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GroupIdParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SetSkillGroupManualMembersParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "assetIds")]
    pub(crate) asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupExclusiveMountParams {
    pub(crate) input: SkillGroupExclusiveMountInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ExecutePlanParams {
    pub(crate) plan: DeploymentPlan,
    #[serde(alias = "actionIds")]
    pub(crate) action_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}
