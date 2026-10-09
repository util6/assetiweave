use super::generation::{MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination};
use super::SourceAvailability;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// L3 晋升候选条目视图（通过应用准入漏斗后的合格候选）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3PromotionCandidate {
    pub item_id: String,
    pub item_revision_id: String,
    pub nomination: MemoryPromotionNomination,
    pub category: MemoryItemCategory,
    pub status: MemoryItemStatus,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub supporting_project_keys: Vec<String>,
    pub session_references: Vec<L3CandidateReferenceView>,
}

/// 候选引用视图，包含项目归属与可用性
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3CandidateReferenceView {
    pub project_key: String,
    pub source_id: String,
    pub session_id: String,
    pub reference_key: String,
    pub role: Option<String>,
    #[serde(default)]
    pub question_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub part_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    pub source_revision: i64,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// Global Consolidation 输入事实首包（无 Markdown）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConsolidationInput {
    pub tenant_id: String,
    pub current_l3_items: Vec<L3MemoryItemView>,
    pub candidates: Vec<L3PromotionCandidate>,
    pub superseded_index: Vec<L3SupersededIndexItem>,
}

/// 计算 Global Consolidation 输入指纹，用于幂等跳过（0 Agent 调用）
pub fn compute_global_consolidation_fingerprint(input: &GlobalConsolidationInput) -> String {
    let serialized = serde_json::to_string(input).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Global Agent 输出的结构化 operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GlobalConsolidationOperation {
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
pub struct GlobalConsolidationResult {
    pub operations: Vec<GlobalConsolidationOperation>,
}

/// 全局长期记忆的 L3 视图（直接面向 AppService 与 Context）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3GlobalMemoryView {
    pub tenant_id: String,
    pub items: Vec<L3MemoryItemView>,
    pub last_successful_consolidation_at: Option<String>,
    pub revision_hash: String,
}

/// L3 Memory Item 完整视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3MemoryItemView {
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
    pub source_references: Vec<L3SourceReferenceView>,
    pub updated_at: String,
}

/// L3 引用视图
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3SourceReferenceView {
    pub source_id: String,
    pub session_id: String,
    pub reference_key: String,
    pub project_key: Option<String>,
    #[serde(default)]
    pub question_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub part_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

/// 被取代条目轻量索引，防止 Agent 复活旧结论
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct L3SupersededIndexItem {
    pub old_item_id: String,
    pub superseding_item_id: String,
    pub reason: Option<String>,
}
