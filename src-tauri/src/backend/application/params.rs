use super::prelude::*;

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
pub(crate) struct ProfileIdParams {
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct IdParams {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TenantCreateParams {
    pub(crate) name: String,
    pub(crate) slug: Option<String>,
    #[serde(default, alias = "setActive")]
    pub(crate) set_active: bool,
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
pub(crate) struct CreateSourceParams {
    pub(crate) source: SourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateSourceParams {
    pub(crate) source: Source,
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
pub(crate) struct UpdateNavigationModelParams {
    pub(crate) model: NavigationModel,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateAppShortcutsParams {
    pub(crate) shortcuts: Vec<AppShortcut>,
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
pub(crate) struct LogsGetSnapshotParams {
    #[serde(alias = "fileName")]
    pub(crate) file_name: Option<String>,
    #[serde(alias = "lineLimit")]
    pub(crate) line_limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LogsWriteOperationParams {
    pub(crate) level: String,
    pub(crate) operation: String,
    pub(crate) message: String,
    pub(crate) fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SaveAppSettingsParams {
    pub(crate) settings: BTreeMap<String, Value>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterUnregisterParams {
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSourceUpsertParams {
    pub(crate) source: ConversationSource,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSourceDisableParams {
    pub(crate) id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptCatalogParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationScriptInstallParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
    #[serde(alias = "itemId")]
    pub(crate) item_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageCatalogParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageInstallParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
    #[serde(alias = "packageId", alias = "itemId")]
    pub(crate) package_id: String,
    #[serde(default)]
    pub(crate) version: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageUninstallParams {
    #[serde(alias = "packageId")]
    pub(crate) package_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageReleaseListParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
    #[serde(alias = "packageId")]
    pub(crate) package_id: String,
    #[serde(default)]
    pub(crate) refresh: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterCatalogRefreshParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
    #[serde(default)]
    pub(crate) force: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageUpdateCheckParams {
    #[serde(default, alias = "catalogUrl")]
    pub(crate) catalog_url: Option<String>,
    #[serde(default)]
    pub(crate) force: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageInspectParams {
    #[serde(default, alias = "packageId")]
    pub(crate) package_id: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageChangeParams {
    pub(crate) action: crate::backend::models::ConversationAdapterPackageChangeAction,
    #[serde(default, alias = "packageId")]
    pub(crate) package_id: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterLocalRegisterParams {
    #[serde(alias = "packageDir")]
    pub(crate) package_dir: String,
    pub(crate) origin: crate::backend::models::ConversationAdapterPackageOrigin,
    #[serde(default, alias = "sourceUrl")]
    pub(crate) source_url: Option<String>,
    #[serde(default, alias = "gitRef")]
    pub(crate) git_ref: Option<String>,
    #[serde(default, alias = "gitCommit")]
    pub(crate) git_commit: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationSyncParams {
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionListParams {
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionGetParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSessionExportParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
    #[serde(alias = "outputRoot")]
    pub(crate) output_root: String,
    #[serde(default, alias = "questionIds")]
    pub(crate) question_ids: Vec<String>,
    #[serde(default, alias = "contentFilter")]
    pub(crate) content_filter: crate::backend::dto::ConversationExportContentFilter,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionListParams {
    #[serde(alias = "sessionId")]
    pub(crate) session_id: String,
    pub(crate) query: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationSearchParams {
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(alias = "projectPath")]
    pub(crate) project_path: Option<String>,
    pub(crate) query: String,
    #[serde(default, alias = "contentTypes")]
    pub(crate) content_types: Vec<crate::backend::dto::ConversationSearchCardType>,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    #[serde(default)]
    pub(crate) timeline: bool,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationSearchScope {
    pub(crate) record_kind: String,
    pub(crate) adapter_id: Option<String>,
    pub(crate) source_id: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) query: String,
    pub(crate) content_types: Vec<crate::backend::dto::ConversationSearchCardType>,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    pub(crate) timeline: bool,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationSearchResult {
    pub(crate) query: String,
    pub(crate) record_kind: String,
    pub(crate) scope: ConversationSearchScope,
    pub(crate) total_count: usize,
    pub(crate) hits: Vec<crate::backend::dto::ConversationSearchHit>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionGetParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionMergeParams {
    #[serde(alias = "questionIds")]
    pub(crate) question_ids: Vec<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionSplitParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
    #[serde(alias = "beforeTurnId")]
    pub(crate) before_turn_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationPartTranslationUpdateParams {
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    #[serde(alias = "partId")]
    pub(crate) part_id: String,
    #[serde(alias = "translatedText")]
    pub(crate) translated_text: String,
}
