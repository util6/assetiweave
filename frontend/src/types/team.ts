export type TeamRole = "leader" | "teammate";

export interface Team {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface TeamMember {
  id: string;
  team_id: string;
  role: TeamRole;
  sort_order: number;
  agent_id: string;
  model: string | null;
  execution_context_key: string;
  created_at: string;
  updated_at: string;
}

export interface TeamDetail {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
  members: TeamMember[];
}

export interface TeamMemberInput {
  id?: string;
  role: TeamRole;
  sort_order?: number;
  agent_id: string;
  model?: string | null;
}

export interface CreateTeamInput {
  id?: string;
  name: string;
  description?: string | null;
  members: TeamMemberInput[];
}

export interface UpdateTeamInput {
  team_id: string;
  name: string;
  description?: string | null;
  members: TeamMemberInput[];
}

export type TeamRunState = "drafting" | "awaiting_review" | "executing" | "terminal";
export type TeamTaskState = "draft" | "queued" | "running" | "succeeded" | "failed" | "canceled";

export interface TeamRosterSnapshotMember {
  member_id: string;
  role: TeamRole;
  sort_order: number;
  agent_id: string;
  model: string | null;
  execution_context_key: string;
}

export interface TeamRun {
  id: string;
  team_id: string;
  state: TeamRunState;
  revision: number;
  leader_member_id: string;
  roster_snapshot: TeamRosterSnapshotMember[];
  created_at: string;
  updated_at: string;
  finished_at: string | null;
  error_code: string | null;
}

export interface TeamTask {
  id: string;
  run_id: string;
  team_id: string;
  title: string;
  description: string;
  sort_order: number;
  recommended_member_id: string;
  owner_member_id: string | null;
  state: TeamTaskState;
  revision: number;
  result: string | null;
  error_code: string | null;
  created_at: string;
  updated_at: string;
}

export interface TeamRunSnapshot {
  run: TeamRun;
  tasks: TeamTask[];
  unread_mailbox_count: number;
}

export interface TeamLeaderChatInput {
  team_id: string;
  message: string;
  replay: boolean;
}

export interface TeamLeaderChatResult {
  team_id: string;
  member_id: string;
  execution_id: string;
  text: string;
  replay: boolean;
}

export interface TeamRestoreMemberStatus {
  member_id: string;
  role: TeamRole;
  state: "ready" | "unavailable";
  error_code: string | null;
}

export interface TeamRestoreSnapshot {
  run: TeamRunSnapshot;
  leader: TeamLeaderChatResult | null;
  leader_error_code: string | null;
  members: TeamRestoreMemberStatus[];
}

export interface TeamRestoreTaskResult {
  run_id: string;
  leader_error_code: string | null;
  members: TeamRestoreMemberStatus[];
}

export interface TeamDraftInput {
  team_id: string;
  leader_message: string;
}

export interface TeamReviewInput {
  run_id: string;
  revision: number;
  tasks: Array<{
    task_id: string;
    title?: string;
    description?: string;
    owner_member_id: string;
    sort_order: number;
  }>;
}

export interface TeamConfirmInput {
  run_id: string;
  revision: number;
}

export interface TeamRuntimeTaskSnapshot {
  task_id: string;
  kind: "TeamRun";
  tenant_id?: string;
  dedup_key: string | null;
  state: "Pending" | "Running" | "Cancelling" | "Succeeded" | "Failed" | "Canceled";
  progress: { current: number; total: number | null; note: string | null } | null;
  error: { code: string; message: string; retryable: boolean; details?: unknown } | null;
  started_at: string;
  finished_at: string | null;
  detail: unknown;
  result: unknown;
}

export type SessionEventDelivery = "live" | "replay";
export type SessionProcessingState = "started" | "active" | "completed";
export type SessionToolState = "running" | "succeeded" | "failed" | "cancelled";
export type SessionTaskStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface SessionEventIdentity {
  session_id: string;
  member_id: string;
  execution_id: string;
  turn_id: string;
  item_id: string;
  event_id: string;
}

export interface SessionItemIdentity {
  session_id: string;
  member_id: string;
  execution_id: string;
  turn_id: string;
  item_id: string;
}

export type SessionEventKind =
  | { type: "user_message_acknowledged"; accepted: boolean }
  | { type: "assistant_text_delta"; text: string }
  | { type: "assistant_text_snapshot"; text: string }
  | { type: "processing"; state: SessionProcessingState }
  | { type: "thinking_delta"; text: string }
  | { type: "thinking_snapshot"; text: string }
  | { type: "tool_start"; name: string | null }
  | { type: "tool_update"; state: SessionToolState; detail: string | null }
  | { type: "tool_result"; success: boolean; detail: string | null }
  | { type: "task_projection"; task_id: string }
  | { type: "task_status"; status: SessionTaskStatus }
  | { type: "task_result"; success: boolean; detail: string | null }
  | { type: "notice"; code: string; detail: string | null }
  | { type: "terminal_result"; text: string | null }
  | { type: "cancel" }
  | { type: "error"; code: string; retryable: boolean };

export interface SessionEvent {
  identity: SessionEventIdentity;
  sequence: number;
  delivery: SessionEventDelivery;
  kind: SessionEventKind;
}

export type SessionItemKind =
  | "user_message"
  | "assistant_text"
  | "processing"
  | "thinking"
  | "tool"
  | "task"
  | "notice"
  | "final_result"
  | "cancelled"
  | "error";

export type SessionItemState =
  | "pending"
  | "streaming"
  | "completed"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface SessionItemSnapshot {
  identity: SessionItemIdentity;
  kind: SessionItemKind;
  sequence: number;
  delivery: SessionEventDelivery;
  state: SessionItemState;
  text: string | null;
  status: SessionTaskStatus | null;
  code: string | null;
}

export interface SessionSnapshot {
  revision: number;
  event_count: number;
  items: SessionItemSnapshot[];
}

export interface TeamMemberTurnInput {
  team_id: string;
  member_id: string;
  message: string;
  replay: boolean;
}

export type TeamMemberTaskPhase =
  | "queued"
  | "resolving"
  | "spawning"
  | "initializing"
  | "creating_session"
  | "configuring"
  | "prompting"
  | "cancelling"
  | "closing"
  | "cleaning_up"
  | string;

export interface TeamMemberTaskCleanup {
  process_reaped: boolean;
  workspace_removed: boolean;
  failure_count: number;
  session_closed: boolean | null;
  session_deleted: boolean | null;
  session_delete_method: "acp" | "provider_fallback" | null;
}

export interface TeamMemberTaskDetail {
  workflow: "team_member_turn";
  tenant_id: string;
  team_id: string;
  member_id: string;
  execution_id: string;
  replay: boolean;
  phase: TeamMemberTaskPhase;
  cleanup?: TeamMemberTaskCleanup;
}

export interface TeamMemberTaskResult {
  workflow: "team_member_turn";
  team_id: string;
  member_id: string;
  execution_id: string;
  replay: boolean;
  terminal: true;
}

export interface TeamMemberTaskSnapshot extends Omit<TeamRuntimeTaskSnapshot, "detail" | "result"> {
  detail: TeamMemberTaskDetail;
  result: TeamMemberTaskResult | null;
}

export interface TeamMemberStreamSnapshot {
  team_id: string;
  member_id: string;
  execution_id: string;
  sequence: number;
  replay: boolean;
  task: TeamMemberTaskSnapshot;
  stream: SessionSnapshot;
}

export type TeamMemberRestoreState = "not-started" | "restoring" | "ready" | "partial" | "unavailable";

export interface TeamMemberExecutionProjection {
  team_id: string;
  member_id: string;
  execution_id: string;
  sequence: number;
  replay: boolean;
  task: TeamMemberTaskSnapshot;
  stream: SessionSnapshot;
}

export interface TeamMemberSessionProjection {
  team_id: string;
  member_id: string;
  execution_id: string | null;
  sequence: number;
  replay: boolean;
  stream: SessionSnapshot;
  task: TeamMemberTaskSnapshot | null;
  unread: boolean;
  restore_state: TeamMemberRestoreState;
  restore_error_code: string | null;
  executions: Record<string, TeamMemberExecutionProjection>;
}

export interface TeamSessionStoreState {
  team_id: string | null;
  members: Record<string, TeamMemberSessionProjection>;
}
