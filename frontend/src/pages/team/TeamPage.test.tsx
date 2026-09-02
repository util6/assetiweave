// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { TeamPage } from "./TeamPage";
import type { TeamMemberStreamSnapshot, TeamMemberTaskSnapshot } from "../../types/team";

const listTeamsMock = vi.hoisted(() => vi.fn());
const createTeamMock = vi.hoisted(() => vi.fn());
const updateTeamMock = vi.hoisted(() => vi.fn());
const deleteTeamMock = vi.hoisted(() => vi.fn());
const listAgentCatalogMock = vi.hoisted(() => vi.fn());
const listAgentMarketMock = vi.hoisted(() => vi.fn());
const listAgentModelsMock = vi.hoisted(() => vi.fn());
const listTeamMemberTasksMock = vi.hoisted(() => vi.fn());
const getTeamMemberStreamSnapshotMock = vi.hoisted(() => vi.fn());
const subscribeTeamMemberSessionsMock = vi.hoisted(() => vi.fn());
const getLatestTeamRunMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/team", () => ({
  createTeam: createTeamMock,
  deleteTeam: deleteTeamMock,
  listTeams: listTeamsMock,
  updateTeam: updateTeamMock,
}));

vi.mock("../../services/agentRuntime", () => ({
  listAgentCatalog: listAgentCatalogMock,
  listAgentMarket: listAgentMarketMock,
  listAgentModels: listAgentModelsMock,
}));

vi.mock("../../services/teamWorkflow", () => ({
  cancelTeamMemberTurn: vi.fn(),
  cancelTeamRun: vi.fn(),
  confirmTeamRun: vi.fn(),
  draftTeam: vi.fn(),
  getLatestTeamRun: getLatestTeamRunMock,
  getTeamMemberStreamSnapshot: getTeamMemberStreamSnapshotMock,
  getTeamRun: vi.fn(),
  listTeamMemberTasks: listTeamMemberTasksMock,
  restoreTeamRun: vi.fn(),
  reviewTeamRun: vi.fn(),
  startTeamMemberReplay: vi.fn(),
  startTeamMemberTurn: vi.fn(),
  subscribeTeamMemberSessions: subscribeTeamMemberSessionsMock,
  teamLeaderChat: vi.fn(),
}));

const fixture = {
  id: "team-1",
  name: "Refactor crew",
  description: "Fixed roster",
  created_at: "2026-08-31T00:00:00Z",
  updated_at: "2026-08-31T00:00:00Z",
  members: [
    { id: "leader", team_id: "team-1", role: "leader" as const, sort_order: 0, agent_id: "agent-a", model: "model-a", execution_context_key: "ctx-leader", created_at: "2026-08-31T00:00:00Z", updated_at: "2026-08-31T00:00:00Z" },
    { id: "teammate", team_id: "team-1", role: "teammate" as const, sort_order: 1, agent_id: "agent-b", model: "model-b", execution_context_key: "ctx-teammate", created_at: "2026-08-31T00:00:00Z", updated_at: "2026-08-31T00:00:00Z" },
  ],
};

function renderPage() {
  return render(<I18nProvider><TeamPage /></I18nProvider>);
}

describe("TeamPage", () => {
  beforeEach(() => {
    listTeamsMock.mockResolvedValue([fixture]);
    createTeamMock.mockResolvedValue({ ...fixture, id: "team-2", name: "New crew" });
    updateTeamMock.mockResolvedValue(fixture);
    deleteTeamMock.mockResolvedValue(undefined);
    listAgentCatalogMock.mockResolvedValue([
      { id: "agent-a", display_name: "Agent A", command: "agent-a", args: [], availability_command: "agent-a", protocol: "acp", capabilities: { text_prompt: true, resume: true, history_replay: true, live_events: true, rich_history_replay: false, team_tools: true, resume_args: null } },
      { id: "agent-b", display_name: "Agent B", command: "agent-b", args: [], availability_command: "agent-b", protocol: "native", capabilities: { text_prompt: true, resume: true, history_replay: true, live_events: true, rich_history_replay: false, team_tools: true, resume_args: null } },
    ]);
    listAgentMarketMock.mockResolvedValue([]);
    listAgentModelsMock.mockImplementation(async (agentId: string) => ({ agent_id: agentId, available: true, models: [{ id: agentId === "agent-a" ? "model-a" : "model-b", label: agentId === "agent-a" ? "Model A" : "Model B", description: null }], current_model_id: agentId === "agent-a" ? "model-a" : "model-b", error_code: null, error: null }));
    listTeamMemberTasksMock.mockResolvedValue([]);
    getTeamMemberStreamSnapshotMock.mockResolvedValue(null);
    subscribeTeamMemberSessionsMock.mockResolvedValue(vi.fn());
    getLatestTeamRunMock.mockResolvedValue(null);
  });

  afterEach(() => cleanup());

  it("loads a roster and moves a member before saving", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeTruthy());

    const upButtons = screen.getAllByRole("button", { name: "Move up" });
    fireEvent.click(upButtons[1]);
    fireEvent.click(screen.getByRole("button", { name: "Save team" }));

    await waitFor(() => expect(updateTeamMock).toHaveBeenCalledWith(expect.objectContaining({
      team_id: "team-1",
      members: expect.arrayContaining([
        expect.objectContaining({ id: "teammate", sort_order: 0 }),
        expect.objectContaining({ id: "leader", sort_order: 1 }),
      ]),
    })));
  });

  it("creates through the service with catalog-selected members", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Create team" }));
    fireEvent.change(screen.getByLabelText("Team name"), { target: { value: "New crew" } });
    fireEvent.click(screen.getByRole("button", { name: "Save team" }));
    await waitFor(() => expect(createTeamMock).toHaveBeenCalledWith(expect.objectContaining({ name: "New crew" })));
  });

  it("filters members by the shared Resume, Replay, and Live Events facts", async () => {
    listAgentMarketMock.mockResolvedValue([
      { id: "agent-a", installed: { enabled: true, executionReady: true }, capabilities: { resume: true, historyReplay: true, liveEvents: true } },
      { id: "agent-b", installed: { enabled: true, executionReady: true }, capabilities: { resume: true, historyReplay: true, liveEvents: false } },
    ]);

    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Create team" }));

    expect(screen.getAllByRole("option", { name: /Agent A/ }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("option", { name: /Agent B/ })).toBeNull();
  });

  it("uses the shared confirm dialog for deletion", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(deleteTeamMock).toHaveBeenCalledWith("team-1"));
  });

  it("opens on the Leader session and switches one active timeline without changing member activity", async () => {
    const leader = memberStream("leader", "execution-leader", "Running", "Leader timeline");
    const teammate = memberStream("teammate", "execution-teammate", "Running", "Teammate timeline");
    listTeamMemberTasksMock.mockResolvedValue([leader.task, teammate.task]);
    getTeamMemberStreamSnapshotMock.mockImplementation(async (_teamId: string, memberId: string) => memberId === "leader" ? leader : teammate);

    renderPage();
    await waitFor(() => expect(screen.getByTestId("team-chat-shell")).toBeTruthy());

    expect(screen.getByTestId("team-member-leader").getAttribute("aria-selected")).toBe("true");
    expect(screen.getByTestId("team-active-recipient").textContent).toContain("Leader");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Leader timeline");
    expect(screen.getByTestId("team-member-teammate-status").textContent).toContain("Working");

    fireEvent.click(screen.getByTestId("team-member-teammate"));

    expect(screen.getByTestId("team-member-teammate").getAttribute("aria-selected")).toBe("true");
    expect(screen.getByTestId("team-active-recipient").textContent).toContain("Teammate");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Teammate timeline");
    expect(screen.getByTestId("team-member-leader-status").textContent).toContain("Working");
  });
});

function memberStream(
  memberId: string,
  executionId: string,
  taskState: TeamMemberTaskSnapshot["state"],
  text: string,
): TeamMemberStreamSnapshot {
  const task: TeamMemberTaskSnapshot = {
    task_id: `task-${executionId}`,
    kind: "TeamRun",
    dedup_key: null,
    state: taskState,
    progress: null,
    error: null,
    started_at: "2026-08-31T00:00:00Z",
    finished_at: null,
    detail: {
      workflow: "team_member_turn",
      tenant_id: "tenant-1",
      team_id: "team-1",
      member_id: memberId,
      execution_id: executionId,
      replay: false,
      phase: "prompting",
    },
    result: null,
  };
  return {
    team_id: "team-1",
    member_id: memberId,
    execution_id: executionId,
    sequence: 1,
    replay: false,
    task,
    stream: {
      revision: 1,
      event_count: 1,
      items: [{
        identity: {
          session_id: `session-${memberId}`,
          member_id: memberId,
          execution_id: executionId,
          turn_id: "turn-1",
          item_id: `item-${memberId}`,
        },
        kind: "assistant_text",
        sequence: 1,
        delivery: "live",
        state: "streaming",
        text,
        status: null,
        code: null,
      }],
    },
  };
}
