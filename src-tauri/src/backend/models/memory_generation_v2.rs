use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemCategory {
    Progress,
    Decision,
    Research,
    Verification,
    Blocker,
    FollowUp,
}

impl MemoryItemCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Decision => "decision",
            Self::Research => "research",
            Self::Verification => "verification",
            Self::Blocker => "blocker",
            Self::FollowUp => "follow_up",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemStatus {
    Active,
    Blocked,
    Waiting,
    Completed,
    Verified,
    Abandoned,
    Superseded,
}

impl MemoryItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Waiting => "waiting",
            Self::Completed => "completed",
            Self::Verified => "verified",
            Self::Abandoned => "abandoned",
            Self::Superseded => "superseded",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Verified | Self::Abandoned | Self::Superseded
        )
    }

    pub fn is_continuable(&self) -> bool {
        matches!(self, Self::Active | Self::Blocked | Self::Waiting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPromotionNomination {
    None,
    ProjectDecision,
    ProjectConstraint,
    RecurringBlocker,
    RecurringTodo,
    ResearchConclusion,
    GlobalRule,
    CrossProjectPattern,
}

impl MemoryPromotionNomination {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ProjectDecision => "project_decision",
            Self::ProjectConstraint => "project_constraint",
            Self::RecurringBlocker => "recurring_blocker",
            Self::RecurringTodo => "recurring_todo",
            Self::ResearchConclusion => "research_conclusion",
            Self::GlobalRule => "global_rule",
            Self::CrossProjectPattern => "cross_project_pattern",
        }
    }
}

fn default_promotion_nomination() -> MemoryPromotionNomination {
    MemoryPromotionNomination::None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGenerationItemV2 {
    pub continues_item_id: Option<String>,
    pub category: MemoryItemCategory,
    pub status: MemoryItemStatus,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub occurred_at: String,
    pub recommendation_rank: Option<i64>,
    pub source_refs: Vec<String>,
    #[serde(default = "default_promotion_nomination")]
    pub promotion_nomination: MemoryPromotionNomination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGenerationProjectV2 {
    pub project_key: String,
    pub summary: String,
    #[serde(default)]
    pub no_material_change: bool,
    pub source_sessions: Vec<String>,
    pub items: Vec<MemoryGenerationItemV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGenerationCoverageV2 {
    pub covered_sessions: Vec<String>,
    pub no_memory_sessions: Vec<String>,
    #[serde(default)]
    pub unreadable_sessions: Vec<String>,
    #[serde(default)]
    pub budget_exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGenerationResultV2 {
    pub schema_version: i64,
    pub projects: Vec<MemoryGenerationProjectV2>,
    pub coverage: MemoryGenerationCoverageV2,
    #[serde(default)]
    pub unknowns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSession {
    pub tenant_id: String,
    pub session_id: String,
    pub source_id: String,
    pub session_title: String,
    pub source_agent: String,
    pub project_key: String,
    pub project_path: Option<String>,
    pub last_activity_at: String,
    pub source_revision: i64,
    pub short_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEvidenceRef {
    pub short_ref: String,
    pub source_id: String,
    pub session_id: String,
    pub session_title: String,
    pub source_agent: String,
    pub last_activity_at: String,
    pub reference_key: String,
    pub source_revision: i64,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub node_id: Option<String>,
}
