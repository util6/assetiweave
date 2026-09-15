use super::memory_generation_v2::{
    MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
use crate::backend::dto::recent_snapshot::SourceAvailability;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// L2 晋升候选条目视图（通过应用准入漏斗后的合格候选）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2PromotionCandidate {
    pub(crate) item_id: String,
    pub(crate) item_revision_id: String,
    pub(crate) project_key: String,
    pub(crate) nomination: MemoryPromotionNomination,
    pub(crate) category: MemoryItemCategory,
    pub(crate) status: MemoryItemStatus,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: String,
    pub(crate) occurred_at: String,
    pub(crate) evidence_fingerprint: String,
    pub(crate) observation_count: usize,
    pub(crate) observed_snapshot_ids: Vec<String>,
    pub(crate) session_references: Vec<L2CandidateReferenceView>,
}

/// 候选引用视图，包含角色与可用性
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2CandidateReferenceView {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) reference_key: String,
    pub(crate) role: Option<String>,
    pub(crate) question_id: Option<String>,
    pub(crate) available: bool,
}

/// Project Consolidation 输入事实首包（无 Markdown）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectConsolidationInput {
    pub(crate) project_key: String,
    pub(crate) project_title: String,
    pub(crate) project_path: Option<String>,
    pub(crate) current_l2_items: Vec<L2MemoryItemView>,
    pub(crate) candidates: Vec<L2PromotionCandidate>,
    pub(crate) source_availability_summary: Vec<L2SourceAvailabilityItem>,
    pub(crate) superseded_index: Vec<L2SupersededIndexItem>,
}

/// 计算 Project Consolidation 输入指纹，用于幂等跳过（0 Agent 调用）
pub(crate) fn compute_project_consolidation_fingerprint(input: &ProjectConsolidationInput) -> String {
    let serialized = serde_json::to_string(input).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Project Agent 输出的结构化 operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum ProjectConsolidationOperation {
    Create {
        category: MemoryItemCategory,
        title: String,
        statement: String,
        rationale: String,
        source_refs: Vec<String>,
    },
    Revise {
        item_id: String,
        statement: String,
        rationale: String,
        source_refs: Vec<String>,
    },
    Supersede {
        old_item_id: String,
        replacement_title: String,
        replacement_statement: String,
        rationale: String,
        category: MemoryItemCategory,
        source_refs: Vec<String>,
    },
    Keep {
        item_id: String,
    },
}

/// Project Consolidation Agent 执行结果
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectConsolidationResult {
    pub(crate) operations: Vec<ProjectConsolidationOperation>,
}

/// 当前项目的 L2 视图（直接面向 AppService 与 Context）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2ProjectMemoryView {
    pub(crate) project_key: String,
    pub(crate) project_path: Option<String>,
    pub(crate) items: Vec<L2MemoryItemView>,
    pub(crate) last_successful_consolidation_at: Option<String>,
    pub(crate) revision_hash: String,
}

/// L2 Memory Item 完整视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2MemoryItemView {
    pub(crate) item_id: String,
    pub(crate) revision_id: String,
    pub(crate) revision_number: i64,
    pub(crate) category: MemoryItemCategory,
    pub(crate) status: MemoryItemStatus,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: String,
    pub(crate) lifecycle: String,
    pub(crate) source_availability: SourceAvailability,
    pub(crate) source_references: Vec<L2SourceReferenceView>,
    pub(crate) updated_at: String,
}

/// L2 引用视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2SourceReferenceView {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) reference_key: String,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
}

/// 引用可用性变更项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2SourceAvailabilityItem {
    pub(crate) reference_key: String,
    pub(crate) session_id: String,
    pub(crate) available: bool,
    pub(crate) reason: Option<String>,
}

/// 被取代条目轻量索引，防止 Agent 复活旧结论
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L2SupersededIndexItem {
    pub(crate) old_item_id: String,
    pub(crate) superseding_item_id: String,
    pub(crate) reason: Option<String>,
}
