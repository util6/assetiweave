import React, { useEffect, useState } from "react";
import { Plus, Users, Trash2, Edit2, Shield, User, Sparkles } from "lucide-react";
import { createTeam, deleteTeam, listTeams, updateTeam } from "../../services/team";
import type { TeamDetail, TeamMemberInput, TeamRole } from "../../types/team";

export function TeamPage() {
  const [teams, setTeams] = useState<TeamDetail[]>([]);
  const [selectedTeamId, setSelectedTeamId] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  // Dialog state
  const [isCreating, setIsCreating] = useState<boolean>(false);
  const [editingTeam, setEditingTeam] = useState<TeamDetail | null>(null);

  const [formName, setFormName] = useState<string>("");
  const [formDesc, setFormDesc] = useState<string>("");
  const [formMembers, setFormMembers] = useState<TeamMemberInput[]>([
    { role: "leader", agent_id: "claude-code", model: "claude-3-7-sonnet" },
    { role: "teammate", agent_id: "codex", model: "gpt-4o" },
  ]);

  const loadTeams = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await listTeams();
      setTeams(data);
      if (data.length > 0 && !selectedTeamId) {
        setSelectedTeamId(data[0].id);
      }
    } catch (err: any) {
      setError(err?.message || "Failed to load teams");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadTeams();
  }, []);

  const selectedTeam = teams.find((t) => t.id === selectedTeamId) ?? null;

  const handleOpenCreate = () => {
    setFormName("");
    setFormDesc("");
    setFormMembers([
      { role: "leader", agent_id: "claude-code", model: "claude-3-7-sonnet" },
      { role: "teammate", agent_id: "codex", model: "gpt-4o" },
    ]);
    setEditingTeam(null);
    setIsCreating(true);
  };

  const handleOpenEdit = (team: TeamDetail) => {
    setFormName(team.name);
    setFormDesc(team.description ?? "");
    setFormMembers(
      team.members.map((m) => ({
        id: m.id,
        role: m.role,
        sort_order: m.sort_order,
        agent_id: m.agent_id,
        model: m.model ?? undefined,
      })),
    );
    setEditingTeam(team);
    setIsCreating(true);
  };

  const handleAddMember = () => {
    setFormMembers([
      ...formMembers,
      { role: "teammate", agent_id: "codex", model: "gpt-4o" },
    ]);
  };

  const handleRemoveMember = (index: number) => {
    setFormMembers(formMembers.filter((_, idx) => idx !== index));
  };

  const handleMemberChange = (
    index: number,
    field: keyof TeamMemberInput,
    value: any,
  ) => {
    const updated = [...formMembers];
    updated[index] = { ...updated[index], [field]: value };
    setFormMembers(updated);
  };

  const handleSaveTeam = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      if (editingTeam) {
        const updated = await updateTeam({
          team_id: editingTeam.id,
          name: formName,
          description: formDesc || undefined,
          members: formMembers,
        });
        setTeams(teams.map((t) => (t.id === updated.id ? updated : t)));
        setIsCreating(false);
      } else {
        const created = await createTeam({
          name: formName,
          description: formDesc || undefined,
          members: formMembers,
        });
        setTeams([created, ...teams]);
        setSelectedTeamId(created.id);
        setIsCreating(false);
      }
    } catch (err: any) {
      setError(err?.message || "Failed to save team");
    }
  };

  const handleDeleteTeam = async (teamId: string) => {
    if (!confirm("Are you sure you want to delete this team?")) return;
    try {
      await deleteTeam(teamId);
      const remaining = teams.filter((t) => t.id !== teamId);
      setTeams(remaining);
      if (selectedTeamId === teamId) {
        setSelectedTeamId(remaining[0]?.id ?? null);
      }
    } catch (err: any) {
      setError(err?.message || "Failed to delete team");
    }
  };

  return (
    <div className="flex h-full w-full flex-col bg-background text-foreground">
      {/* Header */}
      <div className="flex h-14 items-center justify-between border-b px-6">
        <div className="flex items-center gap-3">
          <Users className="h-5 w-5 text-primary" />
          <h1 className="text-lg font-semibold tracking-tight">Teams & Rosters</h1>
        </div>
        <button
          type="button"
          onClick={handleOpenCreate}
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground shadow transition hover:opacity-90"
        >
          <Plus className="h-4 w-4" />
          Create Team
        </button>
      </div>

      {error && (
        <div className="border-b border-destructive/30 bg-destructive/10 px-6 py-2.5 text-xs text-destructive">
          {error}
        </div>
      )}

      {/* Main Split Layout */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left column: Team list */}
        <div className="w-80 flex-shrink-0 border-r overflow-y-auto p-4 space-y-2">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider px-2 mb-2">
            Teams ({teams.length})
          </div>
          {loading && teams.length === 0 ? (
            <div className="p-4 text-sm text-muted-foreground">Loading teams...</div>
          ) : teams.length === 0 ? (
            <div className="p-4 text-center text-sm text-muted-foreground">
              No teams configured yet. Click "Create Team" to get started.
            </div>
          ) : (
            teams.map((team) => {
              const isSelected = team.id === selectedTeamId;
              const leader = team.members.find((m) => m.role === "leader");
              return (
                <div
                  key={team.id}
                  onClick={() => setSelectedTeamId(team.id)}
                  className={`flex cursor-pointer flex-col gap-1 rounded-lg border p-3 text-left transition ${
                    isSelected
                      ? "border-primary/50 bg-primary/10 shadow-sm"
                      : "border-border/50 hover:bg-muted/50"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-medium text-sm text-foreground">{team.name}</span>
                    <span className="text-xs text-muted-foreground">
                      {team.members.length} members
                    </span>
                  </div>
                  {team.description && (
                    <p className="line-clamp-1 text-xs text-muted-foreground">
                      {team.description}
                    </p>
                  )}
                  {leader && (
                    <div className="mt-1 flex items-center gap-1 text-[11px] text-primary/80">
                      <Shield className="h-3 w-3" />
                      <span>{leader.agent_id} ({leader.model ?? "default"})</span>
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>

        {/* Right column: Selected Team Detail */}
        <div className="flex-1 overflow-y-auto p-6">
          {selectedTeam ? (
            <div className="max-w-3xl space-y-6">
              <div className="flex items-start justify-between border-b pb-4">
                <div>
                  <h2 className="text-xl font-bold">{selectedTeam.name}</h2>
                  {selectedTeam.description && (
                    <p className="mt-1 text-sm text-muted-foreground">
                      {selectedTeam.description}
                    </p>
                  )}
                  <div className="mt-2 text-xs text-muted-foreground">
                    Team ID: <code className="rounded bg-muted px-1 py-0.5">{selectedTeam.id}</code>
                  </div>
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => handleOpenEdit(selectedTeam)}
                    className="inline-flex items-center gap-1 rounded border px-2.5 py-1.5 text-xs font-medium hover:bg-muted"
                  >
                    <Edit2 className="h-3.5 w-3.5" />
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDeleteTeam(selectedTeam.id)}
                    className="inline-flex items-center gap-1 rounded border border-destructive/40 text-destructive px-2.5 py-1.5 text-xs font-medium hover:bg-destructive/10"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    Delete
                  </button>
                </div>
              </div>

              <div>
                <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
                  <Users className="h-4 w-4 text-primary" />
                  Roster & Roles ({selectedTeam.members.length})
                </h3>
                <div className="divide-y rounded-md border">
                  {selectedTeam.members.map((member, idx) => (
                    <div
                      key={member.id}
                      className="flex items-center justify-between p-3.5 hover:bg-muted/30"
                    >
                      <div className="flex items-center gap-3">
                        <div className="flex h-7 w-7 items-center justify-center rounded-full bg-muted text-xs font-medium">
                          {idx + 1}
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium">{member.agent_id}</span>
                            <span
                              className={`rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase ${
                                member.role === "leader"
                                  ? "bg-primary/20 text-primary"
                                  : "bg-muted text-muted-foreground"
                              }`}
                            >
                              {member.role}
                            </span>
                          </div>
                          <div className="mt-0.5 text-xs text-muted-foreground">
                            Model: {member.model || "Default"}
                          </div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="text-[11px] text-muted-foreground font-mono">
                          ctx: {member.execution_context_key.slice(0, 12)}...
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-muted-foreground">
              Select a team to view roster
            </div>
          )}
        </div>
      </div>

      {/* Dialog Modal for Create / Edit */}
      {isCreating && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-[rgb(var(--theme-scrim)/0.62)] p-4 backdrop-blur-sm">
          <div className="w-full max-w-xl rounded-lg bg-background border shadow-lg overflow-hidden flex flex-col max-h-[90vh]">
            <div className="flex items-center justify-between border-b px-5 py-3">
              <h3 className="font-semibold text-sm">
                {editingTeam ? "Edit Team Roster" : "Create New Team"}
              </h3>
              <button
                type="button"
                onClick={() => setIsCreating(false)}
                className="text-muted-foreground hover:text-foreground text-sm"
              >
                ✕
              </button>
            </div>

            <form onSubmit={handleSaveTeam} className="flex-1 overflow-y-auto p-5 space-y-4">
              <div>
                <label className="block text-xs font-medium mb-1">Team Name</label>
                <input
                  type="text"
                  required
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  placeholder="e.g. Autonomous Refactor Crew"
                  className="w-full rounded border bg-transparent px-3 py-1.5 text-sm"
                />
              </div>

              <div>
                <label className="block text-xs font-medium mb-1">Description (Optional)</label>
                <textarea
                  rows={2}
                  value={formDesc}
                  onChange={(e) => setFormDesc(e.target.value)}
                  placeholder="Brief description of the team purpose..."
                  className="w-full rounded border bg-transparent px-3 py-1.5 text-sm"
                />
              </div>

              <div className="pt-2">
                <div className="flex items-center justify-between mb-2">
                  <label className="text-xs font-medium">Team Members (1 Leader + ≥1 Teammates)</label>
                  <button
                    type="button"
                    onClick={handleAddMember}
                    className="text-xs text-primary hover:underline flex items-center gap-1"
                  >
                    <Plus className="h-3 w-3" /> Add Member
                  </button>
                </div>

                <div className="space-y-2 max-h-60 overflow-y-auto pr-1">
                  {formMembers.map((member, idx) => (
                    <div
                      key={idx}
                      className="flex items-center gap-2 rounded border p-2 bg-muted/20"
                    >
                      <select
                        value={member.role}
                        onChange={(e) =>
                          handleMemberChange(idx, "role", e.target.value as TeamRole)
                        }
                        className="rounded border bg-background px-2 py-1 text-xs"
                      >
                        <option value="leader">Leader</option>
                        <option value="teammate">Teammate</option>
                      </select>

                      <input
                        type="text"
                        required
                        value={member.agent_id}
                        onChange={(e) =>
                          handleMemberChange(idx, "agent_id", e.target.value)
                        }
                        placeholder="Agent ID (e.g. codex)"
                        className="flex-1 rounded border bg-background px-2 py-1 text-xs"
                      />

                      <input
                        type="text"
                        value={member.model || ""}
                        onChange={(e) =>
                          handleMemberChange(idx, "model", e.target.value || undefined)
                        }
                        placeholder="Model (optional)"
                        className="w-28 rounded border bg-background px-2 py-1 text-xs"
                      />

                      {formMembers.length > 2 && (
                        <button
                          type="button"
                          onClick={() => handleRemoveMember(idx)}
                          className="text-destructive hover:opacity-80 p-1"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              </div>

              <div className="flex justify-end gap-2 pt-4 border-t">
                <button
                  type="button"
                  onClick={() => setIsCreating(false)}
                  className="rounded border px-3 py-1.5 text-xs font-medium hover:bg-muted"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="rounded bg-primary text-primary-foreground px-4 py-1.5 text-xs font-medium hover:opacity-90"
                >
                  Save Team
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
