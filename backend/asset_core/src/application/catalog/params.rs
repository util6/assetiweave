use crate::backend::application::mounting::SourceInput;
use crate::backend::domain::{AssetKind, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListAssetsParams {
    pub kind: Option<AssetKind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AssetIdParams {
    #[serde(alias = "assetId")]
    pub asset_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequiredAssetIdParams {
    #[serde(alias = "assetId")]
    pub asset_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillBackupTaskParams {
    #[serde(alias = "assetIds")]
    pub asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateAssetDescriptionParams {
    #[serde(alias = "assetId")]
    pub asset_id: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteAssetParams {
    #[serde(alias = "assetId")]
    pub asset_id: String,
    #[serde(default)]
    pub unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSourceParams {
    pub source: SourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProbeSourcePathParams {
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SplitContainerSourceParams {
    #[serde(alias = "sourceId")]
    pub source_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSourceParams {
    pub source: Source,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceAddParams {
    #[serde(flatten)]
    pub source: SourceInput,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AssetRefParams {
    #[serde(alias = "assetRef")]
    pub asset_ref: String,
    #[serde(alias = "profileId")]
    pub profile_id: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
    #[serde(default)]
    pub unmount: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportSkillParams {
    pub from: String,
    pub name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillSearchParams {
    pub query: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillAcquireParams {
    pub url: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    pub name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillRemoteCheckParams {
    #[serde(default, alias = "assetId")]
    pub asset_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SkillSearchResult {
    pub query: String,
    pub provider: String,
    pub candidates: Vec<SkillSearchCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SkillSearchCandidate {
    pub name: String,
    pub description: Option<String>,
    pub match_reason: Option<String>,
    pub url: String,
    pub path: Option<String>,
    pub clone_url: Option<String>,
    pub default_branch: Option<String>,
    pub stars: Option<u64>,
    pub acquire_command: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSkillBackupSettingsParams {
    #[serde(alias = "rootPath")]
    pub root_path: String,
    #[serde(default)]
    pub migrate: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceRemoveParams {
    pub id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceScanParams {
    pub kind: Option<AssetKind>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}
