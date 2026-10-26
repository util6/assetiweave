pub mod agents;
pub mod catalog;
pub mod conversations;
pub mod memory;
pub mod mounting;
pub mod system;

pub mod error;
pub mod prelude;
pub mod service;
pub use service::AppService;

pub use error::{AppError, AppResult};

#[cfg(test)]
mod tests;

pub use agents::agent_market::{
    AgentInstallPreview, AgentMarketItemView, AgentMarketRefreshResult, AgentUninstallPreview,
};
pub use catalog::builtin_skills::{
    get_builtin_skill_markdown, list_builtin_skills, BuiltinSkillMetadata,
};
pub use catalog::params::{
    AssetIdParams, AssetRefParams, CreateSourceParams, DeleteAssetParams, ImportSkillParams,
    ListAssetsParams, ProbeSourcePathParams, RequiredAssetIdParams, SkillAcquireParams,
    SkillBackupTaskParams, SkillRemoteCheckParams, SkillSearchParams, SkillSearchResult,
    SourceAddParams, SourceRemoveParams, SourceScanParams, SplitContainerSourceParams,
    UpdateAssetDescriptionParams, UpdateSkillBackupSettingsParams, UpdateSourceParams,
};
pub use conversations::{
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
        ConversationBlockListParams, ConversationCardGetParams, ConversationDataAuditParams,
        ConversationDataRepairParams, ConversationDataRollbackParams,
        ConversationIncrementalSearchParams, ConversationOutlineParams,
        ConversationPartTranslationUpdateParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationScriptCatalogParams,
        ConversationScriptInstallParams, ConversationSearchParams, ConversationSearchResult,
        ConversationSessionExportParams, ConversationSessionGetParams,
        ConversationSessionListParams, ConversationSessionOutlineParams,
        ConversationSourceDisableParams, ConversationSourceUpsertParams, ConversationSyncMode,
        ConversationSyncParams,
    },
    resolve_time_window_bounds, resolve_unified_time,
};
pub use memory::params::{
    MemoryContextResolveParams, MemoryProjectGetParams, MemoryRecallSearchParams,
    MemoryRecallSessionCreateParams, MemoryRecallSessionGetParams, MemoryRecallTurnCancelParams,
    MemoryRecallTurnSendParams, MemoryScopeRebuildParams, MemoryTaskGetParams,
    MemoryTaskListParams, MemoryTaskRetryParams,
};
pub use mounting::{
    mounts::BatchMountWorkflowInput,
    params::{
        ApplySkillGroupMountParams, AssetProfileParams, CreateProfileParams,
        CreateSkillGroupParams, ExecutePlanParams, GroupIdParams, IdParams, ProfileIdParams,
        SetAssetMountParams, SetSkillGroupManualMembersParams, SkillGroupExclusiveMountParams,
        SkillGroupMountParams, UpdateProfileParams, UpdateSkillGroupParams,
    },
};
pub use system::params::{
    BackgroundTaskGetParams, InitializeAppLocaleParams, LogsGetSnapshotParams,
    LogsWriteOperationParams, SaveAppSettingsParams, TenantCreateParams, UpdateAppShortcutsParams,
    UpdateNavigationModelParams,
};
