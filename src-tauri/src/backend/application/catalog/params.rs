use crate::backend::application::mounting::SourceInput;
use crate::backend::domain::{AssetKind, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListAssetsParams {
    pub(crate) kind: Option<AssetKind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RequiredAssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillBackupTaskParams {
    #[serde(alias = "assetIds")]
    pub(crate) asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateAssetDescriptionParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DeleteAssetParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(default)]
    pub(crate) unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateSourceParams {
    pub(crate) source: SourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSourceParams {
    pub(crate) source: Source,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceAddParams {
    #[serde(flatten)]
    pub(crate) source: SourceInput,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetRefParams {
    #[serde(alias = "assetRef")]
    pub(crate) asset_ref: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default)]
    pub(crate) unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ImportSkillParams {
    pub(crate) from: String,
    pub(crate) name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillSearchParams {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillAcquireParams {
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SkillRemoteCheckParams {
    #[serde(default, alias = "assetId")]
    pub(crate) asset_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillSearchResult {
    pub(crate) query: String,
    pub(crate) provider: String,
    pub(crate) candidates: Vec<SkillSearchCandidate>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillSearchCandidate {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) match_reason: Option<String>,
    pub(crate) url: String,
    pub(crate) path: Option<String>,
    pub(crate) clone_url: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) stars: Option<u64>,
    pub(crate) acquire_command: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSkillBackupSettingsParams {
    #[serde(alias = "rootPath")]
    pub(crate) root_path: String,
    #[serde(default)]
    pub(crate) migrate: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceRemoveParams {
    pub(crate) id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceScanParams {
    pub(crate) kind: Option<AssetKind>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}
