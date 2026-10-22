use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageOrigin,
    ConversationPackageUpdatePolicy, ConversationSource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationAdapterUnregisterParams {
    #[serde(alias = "adapterId")]
    pub adapter_id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSourceUpsertParams {
    pub source: ConversationSource,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSourceDisableParams {
    pub id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationScriptCatalogParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationScriptInstallParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
    #[serde(alias = "itemId")]
    pub item_id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageCatalogParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageInstallParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
    #[serde(alias = "packageId", alias = "itemId")]
    pub package_id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageUninstallParams {
    #[serde(alias = "packageId")]
    pub package_id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageVersionChangeParams {
    #[serde(alias = "packageId")]
    pub package_id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageReleaseListParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
    #[serde(alias = "packageId")]
    pub package_id: String,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterCatalogRefreshParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageUpdateCheckParams {
    #[serde(default, alias = "catalogUrl")]
    pub catalog_url: Option<String>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageUpdatePolicyParams {
    #[serde(alias = "packageId")]
    pub package_id: String,
    pub update_policy: ConversationPackageUpdatePolicy,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageInspectParams {
    #[serde(default, alias = "packageId")]
    pub package_id: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterPackageChangeParams {
    pub action: ConversationAdapterPackageChangeAction,
    #[serde(default, alias = "packageId")]
    pub package_id: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterLocalRegisterParams {
    #[serde(alias = "packageDir")]
    pub package_dir: String,
    pub origin: ConversationAdapterPackageOrigin,
    #[serde(default, alias = "sourceUrl")]
    pub source_url: Option<String>,
    #[serde(default, alias = "gitRef")]
    pub git_ref: Option<String>,
    #[serde(default, alias = "gitCommit")]
    pub git_commit: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationAdapterWorkspaceUpgradeParams {
    #[serde(default, alias = "packageDir")]
    pub package_dir: Option<String>,
    #[serde(default)]
    pub developer: bool,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSyncMode {
    #[default]
    Incremental,
    Full,
}

impl ConversationSyncMode {
    pub fn uses_known_versions(self) -> bool {
        matches!(self, Self::Incremental)
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationSyncParams {
    #[serde(alias = "sourceId")]
    pub source_id: Option<String>,
    #[serde(alias = "adapterId")]
    pub adapter_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    /// Synchronization strategy. Incremental is the default; full reparses every discoverable record.
    #[serde(default)]
    pub mode: ConversationSyncMode,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ConversationDataAuditParams {
    #[serde(default, alias = "sourceId")]
    pub source_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    #[serde(default, alias = "includeResolved")]
    pub include_resolved: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ConversationDataRepairParams {
    #[serde(default, alias = "sourceId")]
    pub source_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
    #[serde(default)]
    pub resync: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationDataRollbackParams {
    #[serde(alias = "backupPath")]
    pub backup_path: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
    #[serde(default)]
    pub yes: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ConversationSessionListParams {
    #[serde(alias = "adapterId")]
    pub adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub source_id: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default, alias = "projectPath")]
    pub project_path: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub window: Option<String>,
    #[serde(default)]
    pub range: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSessionGetParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(default, alias = "roles")]
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationSessionOutlineParams {
    #[serde(alias = "anyId")]
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationOutlineParams {
    #[serde(alias = "anyId")]
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConversationCardGetParams {
    #[serde(alias = "cardId")]
    pub card_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSessionExportParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    #[serde(alias = "outputRoot")]
    pub output_root: String,
    #[serde(default, alias = "questionIds")]
    pub question_ids: Vec<String>,
    #[serde(default, alias = "contentFilter")]
    pub content_filter: crate::backend::domain::conversations::ConversationExportContentFilter,
    #[serde(default, alias = "exportFormat")]
    pub format: crate::backend::domain::conversations::ConversationExportFormat,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationQuestionListParams {
    #[serde(alias = "sessionId")]
    pub session_id: String,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSearchParams {
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    #[serde(alias = "adapterId")]
    pub adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub source_id: Option<String>,
    #[serde(alias = "projectPath")]
    pub project_path: Option<String>,
    pub query: String,
    #[serde(default, alias = "contentTypes")]
    pub content_types: Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    #[serde(default, alias = "cardKinds")]
    pub card_kinds: Vec<String>,
    #[serde(default, alias = "semanticRoles")]
    pub semantic_roles: Vec<String>,
    #[serde(default, alias = "includeQuestions")]
    pub include_questions: Option<bool>,
    #[serde(default, alias = "includeCards")]
    pub include_cards: Option<bool>,
    pub since: Option<String>,
    pub until: Option<String>,
    #[serde(default)]
    pub timeline: bool,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default, alias = "searchOptions")]
    pub search_options: Option<ConversationSearchOptions>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationIncrementalSearchParams {
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    #[serde(alias = "adapterId")]
    pub adapter_id: Option<String>,
    #[serde(alias = "sourceId")]
    pub source_id: Option<String>,
    #[serde(alias = "projectPath")]
    pub project_path: Option<String>,
    pub query: String,
    #[serde(default, alias = "contentTypes")]
    pub content_types: Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    #[serde(default, alias = "cardKinds")]
    pub card_kinds: Vec<String>,
    #[serde(default, alias = "semanticRoles")]
    pub semantic_roles: Vec<String>,
    #[serde(default, alias = "includeQuestions")]
    pub include_questions: Option<bool>,
    #[serde(default, alias = "includeCards")]
    pub include_cards: Option<bool>,
    #[serde(default, alias = "recentRuns")]
    pub recent_runs: Option<usize>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default, alias = "searchOptions")]
    pub search_options: Option<ConversationSearchOptions>,
}

impl ConversationIncrementalSearchParams {
    pub fn into_search_params(self) -> ConversationSearchParams {
        ConversationSearchParams {
            record_kind: self.record_kind,
            adapter_id: self.adapter_id,
            source_id: self.source_id,
            project_path: self.project_path,
            query: self.query,
            content_types: self.content_types,
            card_kinds: self.card_kinds,
            semantic_roles: self.semantic_roles,
            include_questions: self.include_questions,
            include_cards: self.include_cards,
            since: None,
            until: None,
            timeline: false,
            limit: self.limit,
            offset: self.offset,
            search_options: self.search_options,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationSearchOptions {
    #[serde(default, alias = "retrievalMode")]
    pub retrieval_mode: Option<crate::backend::domain::conversations::SearchRetrievalMode>,
}

#[derive(Debug, Serialize)]
pub struct ConversationSearchScope {
    pub record_kind: String,
    pub adapter_id: Option<String>,
    pub source_id: Option<String>,
    pub project_path: Option<String>,
    pub query: String,
    pub content_types: Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    pub card_kinds: Vec<String>,
    pub semantic_roles: Vec<String>,
    pub include_questions: bool,
    pub include_cards: bool,
    pub since: Option<String>,
    pub until: Option<String>,
    pub timeline: bool,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub struct ConversationSearchResult {
    pub query: String,
    pub record_kind: String,
    pub scope: ConversationSearchScope,
    pub total_count: usize,
    pub hits: Vec<crate::backend::domain::conversations::ConversationSearchHit>,
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incremental: Option<ConversationSearchIncrementalScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type_counts: Option<BTreeMap<String, usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_role_counts: Option<BTreeMap<String, usize>>,
}

#[derive(Debug, Serialize)]
pub struct ConversationSearchIncrementalScope {
    pub recent_runs: usize,
    pub included_run_count: usize,
    pub changed_session_count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationQuestionGetParams {
    #[serde(alias = "questionId")]
    pub question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationBlockListParams {
    #[serde(alias = "questionId")]
    pub question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationBlockGetParams {
    #[serde(alias = "blockId")]
    pub block_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationQuestionMergeParams {
    #[serde(alias = "questionIds")]
    pub question_ids: Vec<String>,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationQuestionSplitParams {
    #[serde(alias = "questionId")]
    pub question_id: String,
    #[serde(alias = "beforeTurnId")]
    pub before_turn_id: String,
    #[serde(default, alias = "dryRun")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConversationPartTranslationUpdateParams {
    #[serde(default, alias = "recordKind")]
    pub record_kind: Option<String>,
    #[serde(alias = "partId")]
    pub part_id: String,
    #[serde(alias = "translatedText")]
    pub translated_text: String,
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
