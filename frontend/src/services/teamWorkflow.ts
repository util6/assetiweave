import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  teamLeaderChatResultSchema,
  teamRunSnapshotSchema,
  teamRuntimeTaskSnapshotSchema,
} from "../schemas/teamWorkflow";
import { isTauriRuntime } from "./appUpdater";
import type {
  TeamConfirmInput,
  TeamDraftInput,
  TeamLeaderChatInput,
  TeamLeaderChatResult,
  TeamReviewInput,
  TeamRunSnapshot,
  TeamRuntimeTaskSnapshot,
} from "../types/team";

const DESKTOP_REQUIRED = "Team workflows require the desktop application runtime.";

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

export function subscribeTeamRunTasks(
  listener: (snapshot: TeamRuntimeTaskSnapshot) => void,
) {
  if (!isTauriRuntime()) return Promise.resolve(() => undefined);
  return listen<TeamRuntimeTaskSnapshot>("team-run-task-updated", (event) => {
    listener(event.payload);
  });
}

export { DESKTOP_REQUIRED };
