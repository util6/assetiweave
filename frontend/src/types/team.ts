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
  tasks: Array<{ task_id: string; owner_member_id: string; sort_order: number }>;
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
