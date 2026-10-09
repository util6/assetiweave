use super::generation::{MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination};
use super::SourceAvailability;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// L2 晋升候选条目视图（通过应用准入漏斗后的合格候选）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2PromotionCandidate {
    pub item_id: String,
    pub item_revision_id: String,
    pub project_key: String,
    pub nomination: MemoryPromotionNomination,
    pub category: MemoryItemCategory,
    pub status: MemoryItemStatus,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub occurred_at: String,
    pub evidence_fingerprint: String,
    pub observation_count: usize,
    pub observed_snapshot_ids: Vec<String>,
    pub session_references: Vec<L2CandidateReferenceView>,
}

/// 候选引用视图，包含角色与可用性
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2CandidateReferenceView {
    pub source_id: String,
    pub session_id: String,
    pub reference_key: String,
    pub role: Option<String>,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub node_id: Option<String>,
    pub source_revision: i64,
    pub available: bool,
}

/// Project Consolidation 输入事实首包（无 Markdown）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConsolidationInput {
    pub project_key: String,
    pub project_title: String,
    pub project_path: Option<String>,
    pub current_l2_items: Vec<L2MemoryItemView>,
    pub candidates: Vec<L2PromotionCandidate>,
    pub source_availability_summary: Vec<L2SourceAvailabilityItem>,
    pub superseded_index: Vec<L2SupersededIndexItem>,
}

/// 计算 Project Consolidation 输入指纹，用于幂等跳过（0 Agent 调用）
pub fn compute_project_consolidation_fingerprint(input: &ProjectConsolidationInput) -> String {
    let serialized = serde_json::to_string(input).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Project Agent 输出的结构化 operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProjectConsolidationOperation {
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
pub struct ProjectConsolidationResult {
    pub operations: Vec<ProjectConsolidationOperation>,
}

/// 当前项目的 L2 视图（直接面向 AppService 与 Context）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2ProjectMemoryView {
    pub project_key: String,
    pub project_path: Option<String>,
    pub items: Vec<L2MemoryItemView>,
    pub last_successful_consolidation_at: Option<String>,
    pub revision_hash: String,
}

/// L2 Memory Item 完整视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2MemoryItemView {
    pub item_id: String,
    pub revision_id: String,
    pub revision_number: i64,
    pub category: MemoryItemCategory,
    pub status: MemoryItemStatus,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub lifecycle: String,
    pub source_availability: SourceAvailability,
    pub source_references: Vec<L2SourceReferenceView>,
    pub updated_at: String,
}

/// L2 引用视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2SourceReferenceView {
    pub source_id: String,
    pub session_id: String,
    pub reference_key: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// 引用可用性变更项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2SourceAvailabilityItem {
    pub reference_key: String,
    pub session_id: String,
    pub available: bool,
    pub reason: Option<String>,
}

/// 被取代条目轻量索引，防止 Agent 复活旧结论
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L2SupersededIndexItem {
    pub old_item_id: String,
    pub superseding_item_id: String,
    pub reason: Option<String>,
}
