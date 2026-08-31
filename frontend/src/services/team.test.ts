import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTeam, deleteTeam, getTeam, listTeams, updateTeam } from "./team";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("team service", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("routes every roster operation through the typed Tauri commands", async () => {
    const detail = teamDetail();
    invokeMock
      .mockResolvedValueOnce([detail])
      .mockResolvedValueOnce(detail)
      .mockResolvedValueOnce(detail)
      .mockResolvedValueOnce(detail)
      .mockResolvedValueOnce(undefined);

    await expect(listTeams()).resolves.toEqual([detail]);
    await expect(getTeam("team-1")).resolves.toEqual(detail);
    await expect(createTeam({ name: "Crew", members: members() })).resolves.toEqual(detail);
    await expect(updateTeam({ team_id: "team-1", name: "Crew", members: members() })).resolves.toEqual(detail);
    await expect(deleteTeam("team-1")).resolves.toBeUndefined();

    expect(invokeMock).toHaveBeenNthCalledWith(1, "list_teams");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_team", { teamId: "team-1" });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "create_team", {
      input: { name: "Crew", members: members() },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "update_team", {
      input: { team_id: "team-1", name: "Crew", members: members() },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(5, "delete_team", { teamId: "team-1" });
  });

  it("keeps browser preview read-only instead of simulating Team persistence", async () => {
    vi.stubGlobal("window", {});

    await expect(listTeams()).resolves.toEqual([]);
    await expect(getTeam("team-1")).resolves.toBeNull();
    await expect(createTeam({ name: "Crew", members: members() })).rejects.toThrow("desktop application");
    await expect(updateTeam({ team_id: "team-1", name: "Crew", members: members() })).rejects.toThrow("desktop application");
    await expect(deleteTeam("team-1")).rejects.toThrow("desktop application");
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

function members() {
  return [
    { role: "leader" as const, agent_id: "agent-a", model: "model-a" },
    { role: "teammate" as const, agent_id: "agent-b", model: null },
  ];
}

function teamDetail() {
  return {
    id: "team-1",
    name: "Crew",
    description: null,
    created_at: "2026-08-31T00:00:00Z",
    updated_at: "2026-08-31T00:00:00Z",
    members: [
      {
        id: "member-1",
        team_id: "team-1",
        role: "leader" as const,
        sort_order: 0,
        agent_id: "agent-a",
        model: "model-a",
        execution_context_key: "ctx-1",
        created_at: "2026-08-31T00:00:00Z",
        updated_at: "2026-08-31T00:00:00Z",
      },
      {
        id: "member-2",
        team_id: "team-1",
        role: "teammate" as const,
        sort_order: 1,
        agent_id: "agent-b",
        model: null,
        execution_context_key: "ctx-2",
        created_at: "2026-08-31T00:00:00Z",
        updated_at: "2026-08-31T00:00:00Z",
      },
    ],
  };
}
