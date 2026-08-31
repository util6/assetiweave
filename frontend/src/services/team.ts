import { invoke } from "@tauri-apps/api/core";
import { teamDetailSchema, teamListSchema } from "../schemas/team";
import type { CreateTeamInput, TeamDetail, UpdateTeamInput } from "../types/team";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

let mockTeams: TeamDetail[] = [];

export async function listTeams(): Promise<TeamDetail[]> {
  if (!isTauriRuntime()) {
    return mockTeams;
  }
  const result = await invoke("list_teams");
  return teamListSchema.parse(result);
}

export async function getTeam(teamId: string): Promise<TeamDetail | null> {
  if (!isTauriRuntime()) {
    return mockTeams.find((t) => t.id === teamId) ?? null;
  }
  const result = await invoke("get_team", { teamId });
  if (!result) {
    return null;
  }
  return teamDetailSchema.parse(result);
}

export async function createTeam(input: CreateTeamInput): Promise<TeamDetail> {
  if (!isTauriRuntime()) {
    const newTeam: TeamDetail = {
      id: input.id || `mock-team-${Date.now()}`,
      name: input.name,
      description: input.description ?? null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      members: input.members.map((m, index) => ({
        id: m.id || `mock-mem-${Date.now()}-${index}`,
        team_id: input.id || `mock-team-${Date.now()}`,
        role: m.role,
        sort_order: m.sort_order ?? index,
        agent_id: m.agent_id,
        model: m.model ?? null,
        execution_context_key: `mock-ctx-${Date.now()}-${index}`,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      })),
    };
    mockTeams.unshift(newTeam);
    return newTeam;
  }
  const result = await invoke("create_team", { input });
  return teamDetailSchema.parse(result);
}

export async function updateTeam(input: UpdateTeamInput): Promise<TeamDetail> {
  if (!isTauriRuntime()) {
    const idx = mockTeams.findIndex((t) => t.id === input.team_id);
    if (idx === -1) {
      throw new Error(`Team not found: ${input.team_id}`);
    }
    const updated: TeamDetail = {
      ...mockTeams[idx],
      name: input.name,
      description: input.description ?? null,
      updated_at: new Date().toISOString(),
      members: input.members.map((m, index) => ({
        id: m.id || `mock-mem-${Date.now()}-${index}`,
        team_id: input.team_id,
        role: m.role,
        sort_order: m.sort_order ?? index,
        agent_id: m.agent_id,
        model: m.model ?? null,
        execution_context_key: `mock-ctx-${m.id ?? index}`,
        created_at: mockTeams[idx].created_at,
        updated_at: new Date().toISOString(),
      })),
    };
    mockTeams[idx] = updated;
    return updated;
  }
  const result = await invoke("update_team", { input });
  return teamDetailSchema.parse(result);
}

export async function deleteTeam(teamId: string): Promise<void> {
  if (!isTauriRuntime()) {
    mockTeams = mockTeams.filter((t) => t.id !== teamId);
    return;
  }
  await invoke("delete_team", { teamId });
}
