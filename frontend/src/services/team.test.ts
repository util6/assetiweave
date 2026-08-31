import { describe, expect, it } from "vitest";
import { createTeam, getTeam, listTeams, updateTeam, deleteTeam } from "./team";

describe("team service", () => {
  it("creates, lists, updates and deletes teams in preview fallback", async () => {
    const created = await createTeam({
      name: "Engineering Team",
      description: "Frontend & Backend agents",
      members: [
        {
          role: "leader",
          agent_id: "claude-code",
          model: "claude-3-7-sonnet",
        },
        {
          role: "teammate",
          agent_id: "codex",
          model: "gpt-4o",
        },
      ],
    });

    expect(created.name).toBe("Engineering Team");
    expect(created.members.length).toBe(2);
    expect(created.members[0].role).toBe("leader");
    expect(created.members[1].role).toBe("teammate");

    const teams = await listTeams();
    expect(teams.some((t) => t.id === created.id)).toBe(true);

    const fetched = await getTeam(created.id);
    expect(fetched?.id).toBe(created.id);

    const updated = await updateTeam({
      team_id: created.id,
      name: "Engineering Team Renamed",
      members: created.members,
    });
    expect(updated.name).toBe("Engineering Team Renamed");

    await deleteTeam(created.id);
    const afterDelete = await getTeam(created.id);
    expect(afterDelete).toBeNull();
  });
});
