use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod evidence;
pub mod generation;
pub mod global;
pub mod global_consolidation;
pub mod project;
pub mod project_consolidation;
pub mod recent_snapshot;
pub mod session;
pub mod work_order;

pub use evidence::*;
pub use generation::*;
pub use global::*;
pub use global_consolidation::*;
pub use project::*;
pub use project_consolidation::*;
pub use recent_snapshot::*;
pub use session::*;
pub use work_order::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAvailability {
    Available,
    PartiallyUnavailable,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecordKind {
    Session,
    Web,
}

impl MemoryRecordKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Web => "web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecallQuestionRef {
    pub record_kind: MemoryRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchHit {
    pub record_kind: MemoryRecordKind,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: Option<String>,
    pub question_id: String,
    pub question_index: i64,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
    pub card_type: String,
    pub snippet: String,
    pub lexical_score: u64,
    pub semantic_score: u64,
    pub score: u64,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallSearchResult {
    pub query: String,
    pub backend: String,
    pub total_count: usize,
    pub hits: Vec<MemoryRecallSearchHit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallSessionStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    ResumeUnavailable,
}

impl MemoryRecallSessionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::ResumeUnavailable => "resume_unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallTurnStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    ResumeUnavailable,
}

impl MemoryRecallTurnStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::ResumeUnavailable => "resume_unavailable",
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::ResumeUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallSessionReference {
    pub record_kind: MemoryRecordKind,
    pub session_id: String,
    pub question_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallContentReference {
    pub record_kind: MemoryRecordKind,
    pub session_id: String,
    pub question_id: String,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub block_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRecallStructuredOutput {
    pub answer: String,
    #[serde(alias = "session_references")]
    pub session_references: Vec<MemoryRecallSessionReference>,
    #[serde(alias = "content_references")]
    pub content_references: Vec<MemoryRecallContentReference>,
    #[serde(alias = "follow_up_suggestions")]
    pub follow_up_suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallTurn {
    pub id: String,
    pub session_id: String,
    pub sequence: i64,
    pub conversation_session_id: String,
    pub conversation_turn_id: String,
    pub status: MemoryRecallTurnStatus,
    pub user_text: String,
    pub structured_output: Option<MemoryRecallStructuredOutput>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallSession {
    pub id: String,
    pub status: MemoryRecallSessionStatus,
    pub scope: MemoryScope,
    pub execution_context_key: String,
    pub agent_id: String,
    pub model: Option<String>,
    pub turn_count: i64,
    pub active_turn_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub turns: Vec<MemoryRecallTurn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryScope {
    pub app_id: Option<String>,
    pub source_id: Option<String>,
    pub project_path: Option<String>,
    pub session_id: Option<String>,
}

impl MemoryScope {
    pub fn fingerprint(&self) -> Result<String, String> {
        let payload = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        Ok(format!("{:x}", Sha256::digest(payload)))
    }
}

pub const DEFAULT_MEMORY_CONTRACT_VERSION: &str = "memory.contract.v1";
pub const DEFAULT_BUDGET_POLICY_VERSION: &str = "budget.v1";
pub const DEFAULT_PROJECTION_POLICY_VERSION: &str = "projection.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundedMemoryBudgetPolicy {
    pub initial_pack_max_chars: usize,
    pub initial_pack_node_limit: usize,
    pub single_item_max_chars: usize,
    pub tool_response_max_chars: usize,
    pub tool_cumulative_max_chars: usize,
    pub tool_call_limit: usize,
    pub agent_output_max_chars: usize,
}

impl Default for BoundedMemoryBudgetPolicy {
    fn default() -> Self {
        Self {
            initial_pack_max_chars: 32_000,
            initial_pack_node_limit: 32,
            single_item_max_chars: 2_000,
            tool_response_max_chars: 4_000,
            tool_cumulative_max_chars: 20_000,
            tool_call_limit: 10,
            agent_output_max_chars: 8_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecipe {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub focus_areas: Vec<String>,
    pub ignored_topics: Vec<String>,
    pub terminology: Vec<String>,
    pub custom_instructions: Option<String>,
}

impl MemoryRecipe {
    pub fn default_builtin() -> Self {
        Self {
            id: "default".to_string(),
            revision: 1,
            name: "Default Balanced Recipe".to_string(),
            focus_areas: vec![
                "User goals, requirements and decisions".to_string(),
                "Architecture decisions and trade-offs".to_string(),
                "Verification outcomes, bugs and regressions".to_string(),
            ],
            ignored_topics: vec![
                "Transient debugging steps and compiler spam".to_string(),
                "Sensitive credentials and tokens".to_string(),
            ],
            terminology: vec![],
            custom_instructions: None,
        }
    }

    pub fn content_hash(&self) -> String {
        let payload = serde_json::to_vec(self).unwrap_or_default();
        format!("{:x}", Sha256::digest(payload))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecipeSnapshot {
    pub recipe_id: String,
    pub revision: i64,
    pub content_hash: String,
    pub focus_areas: Vec<String>,
    pub ignored_topics: Vec<String>,
    pub terminology: Vec<String>,
    pub custom_instructions: Option<String>,
}

impl From<&MemoryRecipe> for MemoryRecipeSnapshot {
    fn from(recipe: &MemoryRecipe) -> Self {
        Self {
            recipe_id: recipe.id.clone(),
            revision: recipe.revision,
            content_hash: recipe.content_hash(),
            focus_areas: recipe.focus_areas.clone(),
            ignored_topics: recipe.ignored_topics.clone(),
            terminology: recipe.terminology.clone(),
            custom_instructions: recipe.custom_instructions.clone(),
        }
    }
}

impl MemoryRecipeSnapshot {
    pub fn to_recipe(&self) -> MemoryRecipe {
        MemoryRecipe {
            id: self.recipe_id.clone(),
            revision: self.revision,
            name: "Bound Recipe".to_string(),
            focus_areas: self.focus_areas.clone(),
            ignored_topics: self.ignored_topics.clone(),
            terminology: self.terminology.clone(),
            custom_instructions: self.custom_instructions.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExecutionWorkOrder {
    pub work_order_id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub budget_policy_version: String,
    pub budget_policy: BoundedMemoryBudgetPolicy,
    pub recipe: MemoryRecipeSnapshot,
    pub input_fingerprint: String,
    pub created_at: String,
}

impl MemoryExecutionWorkOrder {
    pub fn compute_input_fingerprint(
        source_fingerprint: &str,
        recipe_hash: &str,
        contract_version: &str,
        budget_version: &str,
    ) -> String {
        let combined = format!(
            "{}:{}:{}:{}",
            source_fingerprint, recipe_hash, contract_version, budget_version
        );
        format!("{:x}", Sha256::digest(combined.as_bytes()))
    }

    pub fn new(
        work_order_id: String,
        session_id: String,
        source_id: String,
        source_revision: i64,
        source_fingerprint: String,
        recipe: &MemoryRecipe,
        budget_policy: BoundedMemoryBudgetPolicy,
        created_at: String,
    ) -> Self {
        let recipe_snapshot = MemoryRecipeSnapshot::from(recipe);
        let input_fingerprint = Self::compute_input_fingerprint(
            &source_fingerprint,
            &recipe_snapshot.content_hash,
            DEFAULT_MEMORY_CONTRACT_VERSION,
            DEFAULT_BUDGET_POLICY_VERSION,
        );
        Self {
            work_order_id,
            session_id,
            source_id,
            source_revision,
            source_fingerprint,
            contract_version: DEFAULT_MEMORY_CONTRACT_VERSION.to_string(),
            budget_policy_version: DEFAULT_BUDGET_POLICY_VERSION.to_string(),
            budget_policy,
            recipe: recipe_snapshot,
            input_fingerprint,
            created_at,
        }
    }

    pub fn is_allowed_tool(&self, tool_name: &str) -> bool {
        const ALLOWED_TOOLS: &[&str] = &[
            "get_session_outline",
            "search_session_content",
            "read_question_content",
            "read_content_node",
        ];
        ALLOWED_TOOLS.contains(&tool_name)
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
