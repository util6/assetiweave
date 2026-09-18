use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_MEMORY_CONTRACT_V2: &str = "memory.contract.v2";
pub const DEFAULT_BUDGET_POLICY_V1: &str = "budget.v1";
pub const DEFAULT_PROJECTION_POLICY_V2: &str = "projection.v2";

pub const ALLOWED_MEMORY_GENERATION_TOOLS: &[&str] = &[
    "get_session_outline",
    "search_session_content",
    "read_question_content",
    "read_content_node",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryJobPurpose {
    RecentSnapshot,
    ProjectConsolidation,
    GlobalConsolidation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWindowV2 {
    pub start_utc: String,
    pub end_utc: String,
    pub hours: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScopeV2 {
    pub project_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorySkillBinding {
    pub asset_id: String,
    pub asset_revision: i64,
    pub content_hash: String,
    pub entry_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWorkOrderV2 {
    pub work_order_id: String,
    pub tenant_id: String,
    pub purpose: MemoryJobPurpose,
    pub target_watermark_utc: String,
    pub window: MemoryWindowV2,
    pub scope: MemoryScopeV2,
    pub source_revision_set_hash: String,
    pub contract_version: String,
    pub budget_policy_version: String,
    pub projection_policy_version: String,
    pub skill: MemorySkillBinding,
    pub input_fingerprint: String,
    pub allowed_tools: Vec<String>,
    pub created_at: String,
}

impl MemoryWorkOrderV2 {
    pub fn compute_input_fingerprint(
        purpose: MemoryJobPurpose,
        target_watermark_utc: &str,
        window_hours: u32,
        scope_project_key: Option<&str>,
        source_revision_set_hash: &str,
        skill_content_hash: &str,
        contract_version: &str,
        budget_version: &str,
        projection_version: &str,
    ) -> String {
        let purpose_str = match purpose {
            MemoryJobPurpose::RecentSnapshot => "recent_snapshot",
            MemoryJobPurpose::ProjectConsolidation => "project_consolidation",
            MemoryJobPurpose::GlobalConsolidation => "global_consolidation",
        };
        let project_str = scope_project_key.unwrap_or("");
        let combined = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}",
            purpose_str,
            target_watermark_utc,
            window_hours,
            project_str,
            source_revision_set_hash,
            skill_content_hash,
            contract_version,
            budget_version,
            projection_version,
        );
        format!("{:x}", Sha256::digest(combined.as_bytes()))
    }

    pub fn new(
        work_order_id: String,
        tenant_id: String,
        purpose: MemoryJobPurpose,
        target_watermark_utc: String,
        window: MemoryWindowV2,
        scope: MemoryScopeV2,
        source_revision_set_hash: String,
        skill: MemorySkillBinding,
        created_at: String,
    ) -> Self {
        let input_fingerprint = Self::compute_input_fingerprint(
            purpose,
            &target_watermark_utc,
            window.hours,
            scope.project_key.as_deref(),
            &source_revision_set_hash,
            &skill.content_hash,
            DEFAULT_MEMORY_CONTRACT_V2,
            DEFAULT_BUDGET_POLICY_V1,
            DEFAULT_PROJECTION_POLICY_V2,
        );

        let allowed_tools = ALLOWED_MEMORY_GENERATION_TOOLS
            .iter()
            .map(|&s| s.to_string())
            .collect();

        Self {
            work_order_id,
            tenant_id,
            purpose,
            target_watermark_utc,
            window,
            scope,
            source_revision_set_hash,
            contract_version: DEFAULT_MEMORY_CONTRACT_V2.to_string(),
            budget_policy_version: DEFAULT_BUDGET_POLICY_V1.to_string(),
            projection_policy_version: DEFAULT_PROJECTION_POLICY_V2.to_string(),
            skill,
            input_fingerprint,
            allowed_tools,
            created_at,
        }
    }

    /// M35-SKILL-05: 严格限制仅允许白名单只读证据工具，绝无写、网络、执行或子 Agent 权限
    pub fn is_allowed_tool(&self, tool_name: &str) -> bool {
        ALLOWED_MEMORY_GENERATION_TOOLS.contains(&tool_name)
    }
}

use super::{
    GlobalConsolidationInput, L2ProjectMemoryView, L3MemoryItemView, ProjectConsolidationInput,
    RecentMemoryEvent, SessionMemorySourceReference,
};
use crate::backend::models::memory_generation_v2::{MemoryItemCategory, MemoryItemStatus};

/// M35-L1-08 / M35-L1-10: 上一轮结构化可续接条目，严禁使用旧 Markdown
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContinuableMemoryItemView {
    pub item_id: String,
    pub project_key: String,
    pub category: MemoryItemCategory,
    pub status: MemoryItemStatus,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub days_since_first_seen: i64,
    pub remaining_days: i64,
    pub current_revision_id: String,
    pub current_revision_number: i64,
    pub evidence_fingerprint: String,
    pub source_refs: Vec<String>,
}

/// M35-L1-11: 结构化候选会话事实概要
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSessionSummary {
    pub session_ref: String,
    pub session_id: String,
    pub project_key: String,
    pub title: String,
    pub last_activity_at: String,
    pub source_id: String,
    pub source_agent: String,
    pub source_revision: i64,
}

/// M35-L1-11: 固定在 Work Order 中的单个 Session 结构化事实。
/// `memory` 为空表示 Phase 1 尚未为该 Session 生成有效摘要；这仍然是
/// 可审计的事实，不允许 Worker 在执行时偷偷读取更新后的 Session Memory。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentSnapshotSessionEvidence {
    pub candidate: CandidateSessionSummary,
    pub memory_source_revision: Option<i64>,
    pub summary: Option<String>,
    pub goal: Option<String>,
    pub result: Option<String>,
    pub decisions: Vec<String>,
    pub verification: Vec<String>,
    pub blockers: Vec<String>,
    pub follow_up: Vec<String>,
    pub topics: Vec<String>,
    pub source_references: Vec<SessionMemorySourceReference>,
    pub recent_events: Vec<RecentMemoryEvent>,
}

/// M35-L1-10 / M35-L1-11: 证据首包数据，纯结构化事实，绝无旧 Markdown
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentSnapshotWorkOrderEvidencePack {
    pub target_watermark_utc: String,
    pub window_start_utc: String,
    pub window_end_utc: String,
    pub window_hours: u32,
    pub project_keys: Vec<String>,
    pub candidate_sessions: Vec<CandidateSessionSummary>,
    #[serde(default)]
    pub session_evidence: Vec<RecentSnapshotSessionEvidence>,
    pub continuable_items: Vec<ContinuableMemoryItemView>,
    /// Current effective long-term context at enqueue time. These are views,
    /// not write authority; commit still revalidates against SQLite.
    #[serde(default)]
    pub current_l2_projects: Vec<L2ProjectMemoryView>,
    #[serde(default)]
    pub current_l3_items: Vec<L3MemoryItemView>,
    pub allowed_tools: Vec<String>,
    pub output_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentSnapshotWorkOrderPayload {
    pub target_watermark_utc: String,
    pub local_watermark_date: String,
    pub local_watermark_time: String,
    pub timezone_offset_minutes: i64,
    pub window_hours: i64,
    pub window_start_utc: String,
    pub window_end_utc: String,
    pub target_fingerprint: String,
    pub content_fingerprint: String,
    pub skill: MemorySkillBinding,
    pub skill_text: String,
    pub evidence: RecentSnapshotWorkOrderEvidencePack,
}

/// Project/Global maintenance carries the complete structured evidence used by
/// the Agent. SQLite remains the authority for the commit phase, but a worker
/// never silently replaces the queued evidence with a newer read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMaintenanceWorkOrderPayload {
    pub work_order: MemoryWorkOrderV2,
    pub project_path: Option<String>,
    pub skill_text: String,
    /// `default` keeps historical queued rows deserializable; the worker
    /// rejects a new execution without the typed frozen input.
    #[serde(default)]
    pub project_input: Option<ProjectConsolidationInput>,
    #[serde(default)]
    pub global_input: Option<GlobalConsolidationInput>,
}
