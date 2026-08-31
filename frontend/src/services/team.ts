import { invoke } from "@tauri-apps/api/core";
import { teamDetailSchema, teamListSchema } from "../schemas/team";
import type { CreateTeamInput, TeamDetail, UpdateTeamInput } from "../types/team";
import { isTauriRuntime } from "./appUpdater";

const DESKTOP_REQUIRED = "Team changes require the desktop application runtime.";

export async function listTeams(): Promise<TeamDetail[]> {
  if (!isTauriRuntime()) {
    // Browser preview is deliberately read-only. It must not grow a second
    // persistence or roster-validation implementation.
    return [];
  }
  return teamListSchema.parse(await invoke("list_teams"));
}

export async function getTeam(teamId: string): Promise<TeamDetail | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const result = await invoke("get_team", { teamId: teamId.trim() });
  return result == null ? null : teamDetailSchema.parse(result);
}

export async function createTeam(input: CreateTeamInput): Promise<TeamDetail> {
  if (!isTauriRuntime()) {
    throw new Error(DESKTOP_REQUIRED);
  }
  return teamDetailSchema.parse(await invoke("create_team", { input }));
}

export async function updateTeam(input: UpdateTeamInput): Promise<TeamDetail> {
  if (!isTauriRuntime()) {
    throw new Error(DESKTOP_REQUIRED);
  }
  return teamDetailSchema.parse(await invoke("update_team", { input }));
}

export async function deleteTeam(teamId: string): Promise<void> {
  if (!isTauriRuntime()) {
    throw new Error(DESKTOP_REQUIRED);
  }
  await invoke("delete_team", { teamId: teamId.trim() });
}

export { DESKTOP_REQUIRED };
