use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageOrigin,
    ConversationPackageUpdatePolicy, ConversationSource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub(crate) struct ConversationAdapterPackageVersionChangeParams {
    #[serde(alias = "packageId")]
    pub(crate) package_id: String,
    #[serde(default)]
    pub(crate) version: Option<String>,
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
pub(crate) struct ConversationAdapterPackageUpdatePolicyParams {
    #[serde(alias = "packageId")]
    pub(crate) package_id: String,
    pub(crate) update_policy: ConversationPackageUpdatePolicy,
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
    pub(crate) action: ConversationAdapterPackageChangeAction,
    #[serde(default, alias = "packageId")]
    pub(crate) package_id: Option<String>,
    #[serde(default, alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationAdapterLocalRegisterParams {
    #[serde(alias = "packageDir")]
    pub(crate) package_dir: String,
    pub(crate) origin: ConversationAdapterPackageOrigin,
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
pub(crate) struct ConversationAdapterWorkspaceUpgradeParams {
    #[serde(default, alias = "packageDir")]
    pub(crate) package_dir: Option<String>,
    #[serde(default)]
    pub(crate) developer: bool,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationSyncMode {
    #[default]
    Incremental,
    Full,
}

impl ConversationSyncMode {
    pub(crate) fn uses_known_versions(self) -> bool {
        matches!(self, Self::Incremental)
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationSyncParams {
    #[serde(alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(alias = "adapterId")]
    pub(crate) adapter_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    /// Synchronization strategy. Incremental is the default; full reparses every discoverable record.
    #[serde(default)]
    pub(crate) mode: ConversationSyncMode,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct ConversationDataAuditParams {
    #[serde(default, alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    #[serde(default, alias = "includeResolved")]
    pub(crate) include_resolved: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct ConversationDataRepairParams {
    #[serde(default, alias = "sourceId")]
    pub(crate) source_id: Option<String>,
    #[serde(default, alias = "recordKind")]
    pub(crate) record_kind: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default)]
    pub(crate) resync: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct ConversationDataRollbackParams {
    #[serde(alias = "backupPath")]
    pub(crate) backup_path: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
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
    pub(crate) content_filter:
        crate::backend::domain::conversations::ConversationExportContentFilter,
    #[serde(default, alias = "exportFormat")]
    pub(crate) format: crate::backend::domain::conversations::ConversationExportFormat,
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
    pub(crate) content_types:
        Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    #[serde(default, alias = "cardKinds")]
    pub(crate) card_kinds: Vec<String>,
    #[serde(default, alias = "semanticRoles")]
    pub(crate) semantic_roles: Vec<String>,
    #[serde(default, alias = "includeQuestions")]
    pub(crate) include_questions: Option<bool>,
    #[serde(default, alias = "includeCards")]
    pub(crate) include_cards: Option<bool>,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    #[serde(default)]
    pub(crate) timeline: bool,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
    #[serde(default, alias = "searchOptions")]
    pub(crate) search_options: Option<ConversationSearchOptions>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationIncrementalSearchParams {
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
    pub(crate) content_types:
        Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    #[serde(default, alias = "cardKinds")]
    pub(crate) card_kinds: Vec<String>,
    #[serde(default, alias = "semanticRoles")]
    pub(crate) semantic_roles: Vec<String>,
    #[serde(default, alias = "includeQuestions")]
    pub(crate) include_questions: Option<bool>,
    #[serde(default, alias = "includeCards")]
    pub(crate) include_cards: Option<bool>,
    #[serde(default, alias = "recentRuns")]
    pub(crate) recent_runs: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
    #[serde(default, alias = "searchOptions")]
    pub(crate) search_options: Option<ConversationSearchOptions>,
}

impl ConversationIncrementalSearchParams {
    pub(crate) fn into_search_params(self) -> ConversationSearchParams {
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
pub(crate) struct ConversationSearchOptions {
    #[serde(default, alias = "retrievalMode")]
    pub(crate) retrieval_mode: Option<crate::backend::domain::conversations::SearchRetrievalMode>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationSearchScope {
    pub(crate) record_kind: String,
    pub(crate) adapter_id: Option<String>,
    pub(crate) source_id: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) query: String,
    pub(crate) content_types:
        Vec<crate::backend::domain::conversations::ConversationSearchCardType>,
    pub(crate) card_kinds: Vec<String>,
    pub(crate) semantic_roles: Vec<String>,
    pub(crate) include_questions: bool,
    pub(crate) include_cards: bool,
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
    pub(crate) hits: Vec<crate::backend::domain::conversations::ConversationSearchHit>,
    pub(crate) backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) incremental: Option<ConversationSearchIncrementalScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content_type_counts: Option<BTreeMap<String, usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) semantic_role_counts: Option<BTreeMap<String, usize>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationSearchIncrementalScope {
    pub(crate) recent_runs: usize,
    pub(crate) included_run_count: usize,
    pub(crate) changed_session_count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationQuestionGetParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationBlockListParams {
    #[serde(alias = "questionId")]
    pub(crate) question_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationBlockGetParams {
    #[serde(alias = "blockId")]
    pub(crate) block_id: String,
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

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;
