use super::memory_generation_v2::{
    MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
use crate::backend::dto::recent_snapshot::SourceAvailability;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// L3 晋升候选条目视图（通过应用准入漏斗后的合格候选）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3PromotionCandidate {
    pub(crate) item_id: String,
    pub(crate) item_revision_id: String,
    pub(crate) nomination: MemoryPromotionNomination,
    pub(crate) category: MemoryItemCategory,
    pub(crate) status: MemoryItemStatus,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: String,
    pub(crate) supporting_project_keys: Vec<String>,
    pub(crate) session_references: Vec<L3CandidateReferenceView>,
}

/// 候选引用视图，包含项目归属与可用性
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3CandidateReferenceView {
    pub(crate) project_key: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) reference_key: String,
    pub(crate) role: Option<String>,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
}

/// Global Consolidation 输入事实首包（无 Markdown）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalConsolidationInput {
    pub(crate) tenant_id: String,
    pub(crate) current_l3_items: Vec<L3MemoryItemView>,
    pub(crate) candidates: Vec<L3PromotionCandidate>,
    pub(crate) superseded_index: Vec<L3SupersededIndexItem>,
}

/// 计算 Global Consolidation 输入指纹，用于幂等跳过（0 Agent 调用）
pub(crate) fn compute_global_consolidation_fingerprint(input: &GlobalConsolidationInput) -> String {
    let serialized = serde_json::to_string(input).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Global Agent 输出的结构化 operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum GlobalConsolidationOperation {
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

/// Global Consolidation Agent 执行结果
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GlobalConsolidationResult {
    pub(crate) operations: Vec<GlobalConsolidationOperation>,
}

/// 全局长期记忆的 L3 视图（直接面向 AppService 与 Context）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3GlobalMemoryView {
    pub(crate) tenant_id: String,
    pub(crate) items: Vec<L3MemoryItemView>,
    pub(crate) last_successful_consolidation_at: Option<String>,
    pub(crate) revision_hash: String,
}

/// L3 Memory Item 完整视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3MemoryItemView {
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
    pub(crate) source_references: Vec<L3SourceReferenceView>,
    pub(crate) updated_at: String,
}

/// L3 引用视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3SourceReferenceView {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) reference_key: String,
    pub(crate) project_key: Option<String>,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
}

/// 被取代条目轻量索引，防止 Agent 复活旧结论
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct L3SupersededIndexItem {
    pub(crate) old_item_id: String,
    pub(crate) superseding_item_id: String,
    pub(crate) reason: Option<String>,
}
