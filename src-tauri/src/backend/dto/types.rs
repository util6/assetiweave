use crate::backend::models::{
    AppKind, Asset, AssetGroupRules, AssetKind, AssetMount, ConversationPart, ConversationPartRole,
    ConversationQuestion, ConversationSession, ConversationTurn, DeploymentStrategy,
    MemoryDreamCursor, MemoryDreamDeltaSession, MemoryDreamGateResult, MemoryDreamNote,
    MemoryDreamNoteDetail, MemoryDreamState, MemoryDreamTrigger, MemoryExtraction, MemoryItem,
    MemoryRecallCandidate, MemoryRecallClaim, MemoryRecallConflict, MemoryRecallEvidence,
    MemoryRecallMode, MemoryRecallQuestion, MemoryScope, ProfileSafety, RuleSet, SourceKind,
    SourceOrigin, SourceScannerKind,
};
use crate::backend::targeting::PhysicalMountState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) type AppResult<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryItemPage {
    pub(crate) total_count: usize,
    pub(crate) items: Vec<MemoryItem>,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryDreamPreview {
    pub(crate) scope: MemoryScope,
    pub(crate) scope_fingerprint: String,
    pub(crate) trigger: MemoryDreamTrigger,
    pub(crate) ready: bool,
    pub(crate) gates: Vec<MemoryDreamGateResult>,
    pub(crate) state: Option<MemoryDreamState>,
    pub(crate) source_revision_start: i64,
    pub(crate) source_revision_end: i64,
    pub(crate) cursor_start: Option<MemoryDreamCursor>,
    pub(crate) cursor_end: Option<MemoryDreamCursor>,
    pub(crate) stable_before: String,
    pub(crate) sessions: Vec<MemoryDreamDeltaSession>,
    pub(crate) session_count: usize,
    pub(crate) question_count: usize,
    pub(crate) input_char_count: usize,
    pub(crate) max_sessions: usize,
    pub(crate) max_questions: usize,
    pub(crate) max_input_chars: usize,
    pub(crate) has_more: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryDreamRunResult {
    pub(crate) dry_run: bool,
    pub(crate) run_id: Option<String>,
    pub(crate) note_id: Option<String>,
    pub(crate) markdown: Option<String>,
    pub(crate) preview: MemoryDreamPreview,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryDreamNotePage {
    pub(crate) total_count: usize,
    pub(crate) items: Vec<MemoryDreamNote>,
    pub(crate) limit: usize,
    pub(crate) offset: usize,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryOverview {
    pub(crate) follow_ups: Vec<MemoryItem>,
    pub(crate) recent_items: Vec<MemoryItem>,
    pub(crate) candidate_count: usize,
    pub(crate) latest_dream: Option<MemoryDreamNoteDetail>,
    pub(crate) stale_count: usize,
    pub(crate) dream_status: MemoryDreamPreview,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryRecallPreview {
    pub(crate) mode: MemoryRecallMode,
    pub(crate) scope: MemoryScope,
    pub(crate) query: Option<String>,
    pub(crate) backend: String,
    pub(crate) source_revision: i64,
    pub(crate) total_question_count: usize,
    pub(crate) selected_question_count: usize,
    pub(crate) skipped_question_count: usize,
    pub(crate) evidence_count: usize,
    pub(crate) input_char_count: usize,
    pub(crate) truncated: bool,
    pub(crate) include_unavailable: bool,
    pub(crate) questions: Vec<MemoryRecallQuestion>,
    pub(crate) evidence: Vec<MemoryRecallEvidence>,
    pub(crate) formal_matches: Vec<MemoryItem>,
    pub(crate) dream_matches: Vec<MemoryDreamNote>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryRecallRunResult {
    pub(crate) run_id: Option<String>,
    pub(crate) preview: MemoryRecallPreview,
    pub(crate) synthesized: bool,
    pub(crate) answer_markdown: Option<String>,
    pub(crate) claims: Vec<MemoryRecallClaim>,
    pub(crate) memory_candidates: Vec<MemoryRecallCandidate>,
    pub(crate) conflicts: Vec<MemoryRecallConflict>,
    pub(crate) insufficient_evidence: bool,
    pub(crate) extractions: Vec<MemoryExtraction>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct MemoryVerifyResult {
    pub(crate) source_revision: i64,
    pub(crate) unchanged_revision: bool,
    pub(crate) items: Vec<crate::backend::models::MemoryItemDetail>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub(crate) struct ConversationExportContentFilter(BTreeMap<String, bool>);

impl ConversationExportContentFilter {
    pub(crate) fn is_visible(&self, kind: &str) -> bool {
        self.0.get(kind).copied().unwrap_or(true)
    }
}

impl Default for ConversationExportContentFilter {
    fn default() -> Self {
        Self(BTreeMap::from([
            ("answer".to_string(), true),
            ("tool".to_string(), true),
            ("command".to_string(), true),
            ("code".to_string(), true),
            ("result".to_string(), true),
        ]))
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSessionListItem {
    #[serde(flatten)]
    pub(crate) session: ConversationSession,
    pub(crate) question_count: usize,
    pub(crate) turn_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationQuestionDetail {
    pub(crate) question: ConversationQuestion,
    pub(crate) turns: Vec<ConversationTurn>,
    pub(crate) parts: Vec<ConversationPart>,
    pub(crate) cards: Vec<ConversationCard>,
    pub(crate) content_nodes: Vec<ConversationContentNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ConversationContentNode {
    Card {
        turn_id: String,
        card_index: usize,
    },
    Execution {
        turn_id: String,
        source_execution_id: String,
        command_card_index: Option<usize>,
        result_card_indices: Vec<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationCardRenderer {
    Markdown,
    Plain,
    Path,
    Json,
    Code,
    Command,
    TerminalOutput,
    Diff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ConversationCard {
    pub(crate) card_id: String,
    pub(crate) part_id: String,
    pub(crate) adapter_id: String,
    pub(crate) kind: String,
    pub(crate) semantic_role: Option<String>,
    pub(crate) renderer: ConversationCardRenderer,
    pub(crate) role: ConversationPartRole,
    pub(crate) body: String,
    pub(crate) language: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) source_execution_id: Option<String>,
    pub(crate) command_label: Option<String>,
    pub(crate) translated_body: Option<String>,
    pub(crate) legacy_anchor_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationBlockLocator {
    pub(crate) record_kind: String,
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) turn_id: String,
    pub(crate) block_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) part_id: Option<String>,
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) semantic_role: Option<String>,
    pub(crate) renderer: ConversationCardRenderer,
    pub(crate) role: ConversationPartRole,
    pub(crate) content_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationBlockDetail {
    #[serde(flatten)]
    pub(crate) locator: ConversationBlockLocator,
    pub(crate) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) translated_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSessionDetail {
    pub(crate) session: ConversationSession,
    pub(crate) questions: Vec<ConversationQuestionDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationRecordKind {
    Session,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchRetrievalMode {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationSearchIndexStatus {
    pub(crate) health: String,
    pub(crate) schema_version: i64,
    pub(crate) tokenizer_version: String,
    pub(crate) source_revision: i64,
    pub(crate) indexed_revision: Option<i64>,
    pub(crate) active_generation: Option<String>,
    pub(crate) document_count: i64,
    pub(crate) size_bytes: i64,
    pub(crate) last_built_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) lease_owner: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) is_rebuilding: bool,
    pub(crate) updated_at: String,
    pub(crate) supported_modes: Vec<SearchRetrievalMode>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationSearchIndexRebuildReport {
    pub(crate) generation: String,
    pub(crate) indexed_revision: i64,
    pub(crate) document_count: i64,
    pub(crate) size_bytes: i64,
    pub(crate) duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub(crate) struct ConversationSearchCardType(String);

impl ConversationSearchCardType {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn question() -> Self {
        Self::new("question")
    }

    pub(crate) fn answer() -> Self {
        Self::new("answer")
    }

    pub(crate) fn tool() -> Self {
        Self::new("tool")
    }

    pub(crate) fn command() -> Self {
        Self::new("command")
    }

    pub(crate) fn code() -> Self {
        Self::new("code")
    }

    pub(crate) fn result() -> Self {
        Self::new("result")
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSearchHit {
    pub(crate) session: ConversationSessionListItem,
    pub(crate) question_id: String,
    pub(crate) question_index: i64,
    pub(crate) question_title: String,
    pub(crate) turn_id: Option<String>,
    pub(crate) part_id: Option<String>,
    pub(crate) block_id: String,
    pub(crate) card_type: ConversationSearchCardType,
    pub(crate) snippet: String,
    pub(crate) score: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) incremental: Option<ConversationSearchIncrementalMatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) highlight_segments: Option<Vec<ConversationSearchHighlightSegment>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSearchIncrementalMatch {
    pub(crate) sync_run_id: String,
    pub(crate) change_kind: String,
    pub(crate) observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSearchHighlightSegment {
    pub(crate) text: String,
    pub(crate) matched: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSearchPage {
    pub(crate) total_count: usize,
    pub(crate) hits: Vec<ConversationSearchHit>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationMutationResult {
    pub(crate) dry_run: bool,
    pub(crate) session_id: String,
    pub(crate) affected_question_ids: Vec<String>,
    pub(crate) questions: Vec<ConversationQuestionDetail>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AppOverview {
    pub(crate) source_count: usize,
    pub(crate) asset_count: usize,
    pub(crate) profile_count: usize,
    pub(crate) last_scan_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct CatalogAsset {
    #[serde(flatten)]
    pub(crate) asset: Asset,
    pub(crate) display_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repository: Option<GitRepositoryInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) backup_status: Option<SkillBackupAssetStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GitRepositoryInfo {
    pub(crate) root_path: String,
    pub(crate) display_root_path: String,
    pub(crate) remote_url: Option<String>,
    pub(crate) web_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SkillBackupAssetStatus {
    pub(crate) state: SkillBackupState,
    pub(crate) backup_path: Option<String>,
    pub(crate) display_backup_path: Option<String>,
    pub(crate) hidden_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SkillBackupState {
    BackedUp,
    Downloaded,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillBackupSettings {
    pub(crate) root_path: String,
    pub(crate) expanded_root_path: String,
    pub(crate) default_root_path: String,
    pub(crate) display_root_path: String,
    pub(crate) display_default_root_path: String,
    pub(crate) is_default_root: bool,
    pub(crate) exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillRemoteSource {
    pub(crate) asset_id: String,
    pub(crate) provider: String,
    pub(crate) source_url: String,
    pub(crate) repo_url: String,
    pub(crate) branch: String,
    pub(crate) path: Option<String>,
    pub(crate) acquired_at: String,
    pub(crate) acquired_tree_sha: Option<String>,
    pub(crate) local_content_hash: Option<String>,
    pub(crate) last_checked_at: Option<String>,
    pub(crate) latest_tree_sha: Option<String>,
    pub(crate) status: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SourceInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: SourceKind,
    #[serde(alias = "rootPath")]
    pub(crate) root_path: String,
    #[serde(alias = "scannerKind")]
    pub(crate) scanner_kind: Option<SourceScannerKind>,
    #[serde(alias = "sourceOrigin")]
    pub(crate) source_origin: Option<SourceOrigin>,
    #[serde(alias = "repoRoot")]
    pub(crate) repo_root: Option<String>,
    #[serde(alias = "scanRoot")]
    pub(crate) scan_root: Option<String>,
    #[serde(alias = "originAppKind")]
    pub(crate) origin_app_kind: Option<AppKind>,
    #[serde(default, alias = "originProviderId")]
    pub(crate) origin_provider_id: Option<String>,
    #[serde(alias = "includeGlobs")]
    pub(crate) include_globs: Vec<String>,
    #[serde(alias = "excludeGlobs")]
    pub(crate) exclude_globs: Vec<String>,
    #[serde(alias = "defaultKind")]
    pub(crate) default_kind: Option<AssetKind>,
    pub(crate) enabled: bool,
    pub(crate) priority: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TargetProfileInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) app_kind: Option<AppKind>,
    #[serde(default, alias = "targetProviderId")]
    pub(crate) target_provider_id: Option<String>,
    pub(crate) target_paths: Option<Vec<String>>,
    pub(crate) supported_kinds: Option<Vec<AssetKind>>,
    pub(crate) deployment_strategy: Option<DeploymentStrategy>,
    pub(crate) enabled: Option<bool>,
    pub(crate) include: Option<RuleSet>,
    pub(crate) exclude: Option<RuleSet>,
    pub(crate) safety: Option<ProfileSafety>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionResult {
    pub(crate) executed_count: usize,
    pub(crate) skipped_count: usize,
    pub(crate) conflict_count: usize,
    pub(crate) errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhysicalMountStateDto {
    Mounted,
    NotMounted,
    Conflict,
    Broken,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetMountStatus {
    pub(crate) asset_id: String,
    pub(crate) profile_id: String,
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) display_target_dir: String,
    pub(crate) display_target_path: String,
    pub(crate) display_linked_source: Option<String>,
    pub(crate) state: PhysicalMountStateDto,
    pub(crate) linked_source: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AssetMountObservation {
    pub(crate) asset_id: String,
    pub(crate) profile_id: String,
    pub(crate) target_dir: String,
    pub(crate) target_path: String,
    pub(crate) state: PhysicalMountStateDto,
    pub(crate) linked_source: Option<String>,
    pub(crate) observed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetMountUpdateResult {
    pub(crate) mount: AssetMount,
    pub(crate) status: AssetMountStatus,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AssetGroupInput {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) icon_svg: Option<crate::backend::models::AssetGroupIconSvg>,
    pub(crate) enabled: Option<bool>,
    pub(crate) sort_order: Option<i32>,
    pub(crate) rules: Option<AssetGroupRules>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssetGroupMountError {
    pub(crate) asset_id: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApplyAssetGroupMountResult {
    pub(crate) group_id: String,
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
    pub(crate) requested_count: usize,
    pub(crate) updated_count: usize,
    pub(crate) error_count: usize,
    pub(crate) mounts: Vec<AssetMount>,
    pub(crate) statuses: Vec<AssetMountStatus>,
    pub(crate) errors: Vec<AssetGroupMountError>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SkillGroupExclusiveMountInput {
    pub(crate) group_ids: Vec<String>,
    pub(crate) profile_id: String,
    pub(crate) mount_selected: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountItem {
    pub(crate) asset_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountSkippedItem {
    pub(crate) asset_id: String,
    pub(crate) name: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct SkillGroupExclusiveMountError {
    pub(crate) asset_id: String,
    pub(crate) name: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SkillGroupExclusiveMountPreview {
    pub(crate) profile_id: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) selected_skill_ids: Vec<String>,
    pub(crate) keep: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) mount: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) unmount: Vec<SkillGroupExclusiveMountItem>,
    pub(crate) skipped: Vec<SkillGroupExclusiveMountSkippedItem>,
    pub(crate) keep_count: usize,
    pub(crate) mount_count: usize,
    pub(crate) unmount_count: usize,
    pub(crate) skipped_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApplySkillGroupExclusiveMountResult {
    #[serde(flatten)]
    pub(crate) preview: SkillGroupExclusiveMountPreview,
    pub(crate) statuses: Vec<AssetMountStatus>,
    pub(crate) errors: Vec<SkillGroupExclusiveMountError>,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcut {
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) app_kind: String,
    pub(crate) display_icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) icon_svg: Option<AppShortcutIconSvg>,
    pub(crate) accent_color: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcutIconSvg {
    pub(crate) paths: Vec<AppShortcutIconPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) view_box: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppShortcutIconPath {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) clip_rule: Option<String>,
    pub(crate) d: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fill_rule: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationModel {
    pub(crate) active_rail_id: String,
    pub(crate) active_header_tab_id: String,
    pub(crate) active_sub_nav_id: String,
    pub(crate) rail_items: Vec<RailMenuItem>,
    pub(crate) header_tabs: Vec<HeaderTabItem>,
    pub(crate) sub_nav_items: std::collections::BTreeMap<String, Vec<SubNavItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RailMenuItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) icon: String,
    pub(crate) scope: String,
    pub(crate) enabled: bool,
    pub(crate) position: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HeaderTabItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) asset_kind: Option<String>,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubNavItem {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<LocalizedNavigationLabels>,
    pub(crate) route_key: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalizedNavigationLabels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zh: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) en: Option<String>,
}
