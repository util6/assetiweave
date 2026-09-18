use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryItemCategory {
    #[serde(alias = "update", alias = "task", alias = "implementation")]
    Progress,
    #[serde(alias = "architecture", alias = "design")]
    Decision,
    #[serde(alias = "investigation", alias = "exploration")]
    Research,
    #[serde(alias = "test", alias = "validation")]
    Verification,
    #[serde(alias = "issue", alias = "problem", alias = "bug")]
    Blocker,
    #[serde(alias = "todo", alias = "next_step", alias = "followup")]
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
    #[serde(
        alias = "working",
        alias = "in_progress",
        alias = "ongoing",
        alias = "progress",
        alias = "open"
    )]
    Active,
    #[serde(alias = "block", alias = "blocking")]
    Blocked,
    #[serde(alias = "wait", alias = "pending")]
    Waiting,
    #[serde(
        alias = "complete",
        alias = "done",
        alias = "finished",
        alias = "resolved"
    )]
    Completed,
    #[serde(alias = "verify", alias = "tested", alias = "confirmed")]
    Verified,
    #[serde(alias = "abandon", alias = "cancelled", alias = "dropped")]
    Abandoned,
    #[serde(alias = "supersede", alias = "replaced")]
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
    pub project_key: String,
    pub session_title: String,
    pub source_agent: String,
    pub last_activity_at: String,
    pub reference_key: String,
    pub source_revision: i64,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub node_id: Option<String>,
}
