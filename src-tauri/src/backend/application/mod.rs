pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod memory;
pub(crate) mod mounting;
pub(crate) mod system;

pub(crate) mod error;
pub(crate) mod prelude;
mod service;

pub(crate) use error::{AppError, AppResult};

#[cfg(test)]
mod bounded_evidence_baseline_tests;
#[cfg(test)]
mod tests;

pub(crate) use agents::agent_market::{
    AgentInstallPreview, AgentMarketItemView, AgentMarketRefreshResult, AgentUninstallPreview,
};
pub(crate) use catalog::params::{
    AssetIdParams, AssetRefParams, CreateSourceParams, DeleteAssetParams, ImportSkillParams,
    ListAssetsParams, RequiredAssetIdParams, SkillAcquireParams, SkillBackupTaskParams,
    SkillRemoteCheckParams, SkillSearchParams, SkillSearchResult, SourceAddParams,
    SourceRemoveParams, SourceScanParams, UpdateAssetDescriptionParams,
    UpdateSkillBackupSettingsParams, UpdateSourceParams,
};
pub(crate) use conversations::{
    conversation_adapter_catalog_v2::ConversationAdapterPackageUpdateStatus,
    conversation_script_catalog::{
        ConversationAdapterPackageCatalogEntry, ConversationAdapterPackageChangePreflight,
        ConversationAdapterPackageInspection, ConversationScriptCatalogEntry,
    },
    params::{
        ConversationAdapterCatalogRefreshParams, ConversationAdapterLocalRegisterParams,
        ConversationAdapterPackageCatalogParams, ConversationAdapterPackageChangeParams,
        ConversationAdapterPackageInspectParams, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageReleaseListParams, ConversationAdapterPackageUninstallParams,
        ConversationAdapterPackageUpdateCheckParams, ConversationAdapterPackageUpdatePolicyParams,
        ConversationAdapterPackageVersionChangeParams, ConversationAdapterUnregisterParams,
        ConversationAdapterWorkspaceUpgradeParams, ConversationBlockGetParams,
        ConversationBlockListParams, ConversationDataAuditParams, ConversationDataRepairParams,
        ConversationDataRollbackParams, ConversationIncrementalSearchParams,
        ConversationPartTranslationUpdateParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationQuestionMergeParams,
        ConversationQuestionSplitParams, ConversationScriptCatalogParams,
        ConversationScriptInstallParams, ConversationSearchParams, ConversationSearchResult,
        ConversationSessionExportParams, ConversationSessionGetParams,
        ConversationSessionListParams, ConversationSourceDisableParams,
        ConversationSourceUpsertParams, ConversationSyncMode, ConversationSyncParams,
    },
};
pub(crate) use memory::params::{
    MemoryContextResolveParams, MemoryProjectGetParams, MemoryRecallSearchParams,
    MemoryRecallSessionCreateParams, MemoryRecallSessionGetParams, MemoryRecallTurnCancelParams,
    MemoryRecallTurnSendParams, MemoryScopeRebuildParams, MemoryTaskGetParams,
    MemoryTaskListParams, MemoryTaskRetryParams,
};
pub(crate) use mounting::{
    mounts::BatchMountWorkflowInput,
    params::{
        ApplySkillGroupMountParams, AssetProfileParams, CreateProfileParams,
        CreateSkillGroupParams, ExecutePlanParams, GroupIdParams, IdParams, ProfileIdParams,
        SetAssetMountParams, SetSkillGroupManualMembersParams, SkillGroupExclusiveMountParams,
        SkillGroupMountParams, UpdateProfileParams, UpdateSkillGroupParams,
    },
};
pub(crate) use service::AppService;
pub(crate) use system::params::{
    BackgroundTaskGetParams, InitializeAppLocaleParams, LogsGetSnapshotParams,
    LogsWriteOperationParams, SaveAppSettingsParams, TenantCreateParams, UpdateAppShortcutsParams,
    UpdateNavigationModelParams,
};
