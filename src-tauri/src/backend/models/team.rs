use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Leader,
    Teammate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub role: TeamRole,
    pub sort_order: i32,
    pub agent_id: String,
    pub model: Option<String>,
    pub execution_context_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamDetail {
    #[serde(flatten)]
    pub team: Team,
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMemberInput {
    pub id: Option<String>,
    pub role: TeamRole,
    pub sort_order: Option<i32>,
    pub agent_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateTeamInput {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<TeamMemberInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateTeamInput {
    pub team_id: String,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<TeamMemberInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamRunState {
    Drafting,
    AwaitingReview,
    Executing,
    Terminal,
}

impl TeamRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Drafting => "drafting",
            Self::AwaitingReview => "awaiting_review",
            Self::Executing => "executing",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskState {
    Draft,
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

impl TeamTaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamRosterSnapshotMember {
    pub member_id: String,
    pub role: TeamRole,
    pub sort_order: i32,
    pub agent_id: String,
    pub model: Option<String>,
    pub execution_context_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamTaskDraft {
    pub id: Option<String>,
    pub title: String,
    pub description: String,
    pub recommended_member_id: String,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamRun {
    pub id: String,
    pub team_id: String,
    pub state: TeamRunState,
    pub revision: i64,
    pub leader_member_id: String,
    pub roster_snapshot: Vec<TeamRosterSnapshotMember>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamTask {
    pub id: String,
    pub run_id: String,
    pub team_id: String,
    pub title: String,
    pub description: String,
    pub sort_order: i32,
    pub recommended_member_id: String,
    pub owner_member_id: Option<String>,
    pub state: TeamTaskState,
    pub revision: i64,
    pub result: Option<String>,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMailboxMessage {
    pub id: String,
    pub team_id: String,
    pub run_id: String,
    pub task_id: Option<String>,
    pub sender_member_id: String,
    pub recipient_member_id: String,
    pub message_type: String,
    pub body: String,
    pub created_at: String,
    pub read_at: Option<String>,
    pub acked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamRunSnapshot {
    pub run: TeamRun,
    pub tasks: Vec<TeamTask>,
    pub unread_mailbox_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TeamMemberRestoreState {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMemberRestoreStatus {
    pub member_id: String,
    pub role: TeamRole,
    pub state: TeamMemberRestoreState,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamRestoreSnapshot {
    pub run: TeamRunSnapshot,
    pub leader: Option<TeamLeaderChatResult>,
    pub leader_error_code: Option<String>,
    pub members: Vec<TeamMemberRestoreStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamRestoreTaskResult {
    pub run_id: String,
    pub leader_error_code: Option<String>,
    pub members: Vec<TeamMemberRestoreStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamLeaderChatInput {
    pub team_id: String,
    pub message: String,
    pub replay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamLeaderChatResult {
    pub team_id: String,
    pub member_id: String,
    pub execution_id: String,
    pub text: String,
    pub replay: bool,
}

/// A direct turn/replay request scoped to one Team member.
///
/// `replay` deliberately shares the same request shape as a live turn so the
/// application workflow cannot grow a separate Leader/Teammate path. Provider
/// anchors are resolved from Agent Execution by `execution_context_key` and
/// never arrive from a transport or the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMemberTurnInput {
    pub team_id: String,
    pub member_id: String,
    pub message: String,
    pub replay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamDraftInput {
    pub team_id: String,
    pub leader_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamReviewInput {
    pub run_id: String,
    pub revision: i64,
    pub tasks: Vec<TeamReviewTaskInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamReviewTaskInput {
    pub task_id: String,
    pub owner_member_id: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamConfirmInput {
    pub run_id: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMailboxSendInput {
    pub team_id: String,
    pub run_id: String,
    pub task_id: Option<String>,
    pub sender_member_id: String,
    pub recipient_member_id: String,
    pub message_type: String,
    pub body: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamMailboxReadInput {
    pub team_id: String,
    pub run_id: String,
    pub recipient_member_id: String,
    pub ack: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamToolCredentialInput {
    pub team_id: String,
    pub run_id: String,
    pub member_id: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamToolCredential {
    pub credential: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamToolTaskListInput {
    pub team_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamTaskUpdateInput {
    pub team_id: String,
    pub run_id: String,
    pub task_id: String,
    pub member_id: String,
    pub state: TeamTaskState,
    pub result: Option<String>,
    pub error_code: Option<String>,
}
