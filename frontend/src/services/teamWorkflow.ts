import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  teamLeaderChatResultSchema,
  teamRunSnapshotSchema,
  teamMemberStreamSnapshotSchema,
  teamMemberTaskSnapshotSchema,
  teamRuntimeTaskSnapshotSchema,
} from "../schemas/teamWorkflow";
import { isTauriRuntime } from "./appUpdater";
import type {
  TeamConfirmInput,
  TeamDraftInput,
  TeamLeaderChatInput,
  TeamLeaderChatResult,
  TeamMemberStreamSnapshot,
  TeamMemberTaskSnapshot,
  TeamMemberTurnInput,
  TeamReviewInput,
  TeamRunSnapshot,
  TeamRuntimeTaskSnapshot,
} from "../types/team";

const DESKTOP_REQUIRED = "Team workflows require the desktop application runtime.";
export const TEAM_MEMBER_SESSION_UPDATED_EVENT = "team-member-session://updated";

function requireDesktop() {
  if (!isTauriRuntime()) throw new Error(DESKTOP_REQUIRED);
}

export async function teamLeaderChat(input: TeamLeaderChatInput): Promise<TeamLeaderChatResult> {
  requireDesktop();
  return teamLeaderChatResultSchema.parse(await invoke("team_leader_chat", { input }));
}

export async function draftTeam(input: TeamDraftInput): Promise<TeamRunSnapshot> {
  requireDesktop();
  return teamRunSnapshotSchema.parse(await invoke("team_run_draft", { input }));
}

export async function getTeamRun(runId: string): Promise<TeamRunSnapshot | null> {
  requireDesktop();
  const value = await invoke("team_run_get", { runId });
  return value == null ? null : teamRunSnapshotSchema.parse(value);
}

export async function getLatestTeamRun(teamId: string): Promise<TeamRunSnapshot | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("team_run_latest", { teamId });
  return value == null ? null : teamRunSnapshotSchema.parse(value);
}

export async function reviewTeamRun(input: TeamReviewInput): Promise<TeamRunSnapshot> {
  requireDesktop();
  return teamRunSnapshotSchema.parse(await invoke("team_run_review", { input }));
}

export async function confirmTeamRun(input: TeamConfirmInput): Promise<TeamRunSnapshot> {
  requireDesktop();
  return teamRunSnapshotSchema.parse(await invoke("team_run_confirm", { input }));
}

export async function restoreTeamRun(runId: string): Promise<TeamRuntimeTaskSnapshot> {
  requireDesktop();
  return teamRuntimeTaskSnapshotSchema.parse(await invoke("team_run_restore", { runId }));
}

export async function cancelTeamRun(runId: string): Promise<TeamRuntimeTaskSnapshot> {
  requireDesktop();
  return teamRuntimeTaskSnapshotSchema.parse(await invoke("team_run_cancel", { runId }));
}

export async function getTeamRunTask(taskId: string): Promise<TeamRuntimeTaskSnapshot | null> {
  requireDesktop();
  const value = await invoke("team_run_task", { taskId });
  return value == null ? null : teamRuntimeTaskSnapshotSchema.parse(value);
}

export async function listTeamRunTasks(): Promise<TeamRuntimeTaskSnapshot[]> {
  if (!isTauriRuntime()) return [];
  return teamRuntimeTaskSnapshotSchema.array().parse(await invoke("list_team_run_tasks"));
}

export async function startTeamMemberTurn(input: TeamMemberTurnInput): Promise<TeamMemberStreamSnapshot> {
  requireDesktop();
  return teamMemberStreamSnapshotSchema.parse(await invoke("team_member_turn_start", { input }));
}

export async function startTeamMemberReplay(teamId: string, memberId: string): Promise<TeamMemberStreamSnapshot> {
  requireDesktop();
  return teamMemberStreamSnapshotSchema.parse(await invoke("team_member_replay_start", {
    teamId: teamId.trim(),
    memberId: memberId.trim(),
  }));
}

export async function getTeamMemberStreamSnapshot(
  teamId: string,
  memberId: string,
  executionId: string,
): Promise<TeamMemberStreamSnapshot | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("team_member_stream_snapshot", {
    teamId: teamId.trim(),
    memberId: memberId.trim(),
    executionId: executionId.trim(),
  });
  return value == null ? null : teamMemberStreamSnapshotSchema.parse(value);
}

export async function getTeamMemberTask(taskId: string): Promise<TeamMemberTaskSnapshot | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("team_member_task_get", { taskId: taskId.trim() });
  return value == null ? null : teamMemberTaskSnapshotSchema.parse(value);
}

export async function listTeamMemberTasks(): Promise<TeamMemberTaskSnapshot[]> {
  if (!isTauriRuntime()) return [];
  return teamMemberTaskSnapshotSchema.array().parse(await invoke("team_member_tasks_list"));
}

export async function cancelTeamMemberTurn(
  teamId: string,
  memberId: string,
  executionId: string,
): Promise<TeamMemberStreamSnapshot> {
  requireDesktop();
  return teamMemberStreamSnapshotSchema.parse(await invoke("team_member_turn_cancel", {
    teamId: teamId.trim(),
    memberId: memberId.trim(),
    executionId: executionId.trim(),
  }));
}

export function subscribeTeamMemberSessions(listener: (snapshot: TeamMemberStreamSnapshot) => void) {
  if (!isTauriRuntime()) return Promise.resolve(() => undefined);
  return listen<unknown>(TEAM_MEMBER_SESSION_UPDATED_EVENT, (event) => {
    const parsed = teamMemberStreamSnapshotSchema.safeParse(event.payload);
    if (parsed.success) listener(parsed.data);
  });
}

export function subscribeTeamRunTasks(
  listener: (snapshot: TeamRuntimeTaskSnapshot) => void,
) {
  if (!isTauriRuntime()) return Promise.resolve(() => undefined);
  return listen<TeamRuntimeTaskSnapshot>("team-run-task-updated", (event) => {
    listener(event.payload);
  });
}

export { DESKTOP_REQUIRED };
