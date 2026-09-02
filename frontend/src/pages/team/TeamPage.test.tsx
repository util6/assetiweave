// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
const startTeamMemberTurnMock = vi.hoisted(() => vi.fn());

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
  startTeamMemberTurn: startTeamMemberTurnMock,
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

let emitMemberSnapshot: ((snapshot: TeamMemberStreamSnapshot) => void) | null = null;

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
    emitMemberSnapshot = null;
    subscribeTeamMemberSessionsMock.mockImplementation(async (listener: (snapshot: TeamMemberStreamSnapshot) => void) => {
      emitMemberSnapshot = listener;
      return vi.fn();
    });
    startTeamMemberTurnMock.mockReset();
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

    await waitFor(() => {
      expect(screen.getByTestId("team-member-teammate").getAttribute("aria-selected")).toBe("true");
      expect(screen.getByTestId("team-active-recipient").textContent).toContain("Teammate");
      expect(screen.getByTestId("team-timeline").textContent).toContain("Teammate timeline");
    });
    expect(screen.getByTestId("team-member-leader-status").textContent).toContain("Working");
  });

  it("sends to the active teammate and shows one optimistic user item before backend acceptance", async () => {
    let resolveStart: ((snapshot: TeamMemberStreamSnapshot) => void) | null = null;
    startTeamMemberTurnMock.mockImplementation(() => new Promise<TeamMemberStreamSnapshot>((resolve) => {
      resolveStart = resolve;
    }));

    renderPage();
    await waitFor(() => expect(screen.getByTestId("team-chat-shell")).toBeTruthy());
    fireEvent.click(screen.getByTestId("team-member-teammate"));
    fireEvent.change(screen.getByLabelText("Message content"), { target: { value: "Inspect the cache" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    expect(startTeamMemberTurnMock).toHaveBeenCalledWith({
      team_id: "team-1",
      member_id: "teammate",
      message: "Inspect the cache",
      replay: false,
    });
    expect(screen.getByTestId("team-active-recipient").textContent).toContain("Teammate");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Inspect the cache");
    expect(screen.getByRole("button", { name: "Send" }).hasAttribute("disabled")).toBe(true);

    await act(async () => {
      resolveStart?.(streamSnapshotWithItems("teammate", "execution-direct", "Running", [
        sessionItem("teammate", "execution-direct", "workflow:user", "user_message", null, 1, "completed"),
      ]));
    });
    await waitFor(() => expect(screen.getAllByText("Inspect the cache")).toHaveLength(1));
  });

  it("uses the same composer for the default Leader recipient", async () => {
    startTeamMemberTurnMock.mockResolvedValue(streamSnapshotWithItems("leader", "execution-leader", "Running", [
      sessionItem("leader", "execution-leader", "workflow:user", "user_message", null, 1, "completed"),
    ]));

    renderPage();
    await waitFor(() => expect(screen.getByTestId("team-chat-shell")).toBeTruthy());
    fireEvent.change(screen.getByLabelText("Message content"), { target: { value: "Ask the leader" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => expect(startTeamMemberTurnMock).toHaveBeenCalledWith({
      team_id: "team-1",
      member_id: "leader",
      message: "Ask the leader",
      replay: false,
    }));
    expect(screen.getByTestId("team-timeline").textContent).toContain("Ask the leader");
  });

  it("marks a rejected message failed only in the active member timeline", async () => {
    const leader = memberStream("leader", "execution-leader", "Running", "Leader is still working");
    listTeamMemberTasksMock.mockResolvedValue([leader.task]);
    getTeamMemberStreamSnapshotMock.mockResolvedValue(leader);
    startTeamMemberTurnMock.mockRejectedValue(new Error("member_busy"));

    renderPage();
    await waitFor(() => expect(screen.getByTestId("team-chat-shell")).toBeTruthy());
    fireEvent.click(screen.getByTestId("team-member-teammate"));
    fireEvent.change(screen.getByLabelText("Message content"), { target: { value: "This should fail" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      expect(screen.getByTestId("team-timeline").textContent).toContain("member_busy");
      expect(screen.getByTestId("team-timeline").textContent).toContain("failed");
    });
    fireEvent.click(screen.getByTestId("team-member-leader"));
    expect(screen.getByTestId("team-timeline").textContent).toContain("Leader is still working");
    expect(screen.getByTestId("team-timeline").textContent).not.toContain("This should fail");
  });

  it("renders rich generic member events in place while inactive members keep running", async () => {
    const accepted = streamSnapshotWithItems("teammate", "execution-direct", "Running", [
      sessionItem("teammate", "execution-direct", "workflow:user", "user_message", null, 1, "completed"),
    ]);
    startTeamMemberTurnMock.mockResolvedValue(accepted);

    renderPage();
    await waitFor(() => expect(screen.getByTestId("team-chat-shell")).toBeTruthy());
    fireEvent.click(screen.getByTestId("team-member-teammate"));
    fireEvent.change(screen.getByLabelText("Message content"), { target: { value: "Stream the result" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(screen.getAllByText("Stream the result")).toHaveLength(1));
    await waitFor(() => expect(emitMemberSnapshot).not.toBeNull());

    await act(async () => {
      emitMemberSnapshot?.(streamSnapshotWithItems("teammate", "execution-direct", "Running", [
        sessionItem("teammate", "execution-direct", "workflow:user", "user_message", null, 1, "completed"),
        sessionItem("teammate", "execution-direct", "assistant-1", "assistant_text", "First delta", 2, "streaming"),
        sessionItem("teammate", "execution-direct", "processing-1", "processing", null, 3, "streaming"),
        sessionItem("teammate", "execution-direct", "tool-1", "tool", null, 4, "streaming"),
      ], 4));
    });
    expect(screen.getByTestId("team-timeline").textContent).toContain("First delta");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Processing");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Tool activity");

    await act(async () => {
      emitMemberSnapshot?.(streamSnapshotWithItems("teammate", "execution-direct", "Running", [
        sessionItem("teammate", "execution-direct", "workflow:user", "user_message", null, 1, "completed"),
        sessionItem("teammate", "execution-direct", "assistant-1", "assistant_text", "Final delta", 2, "streaming"),
        sessionItem("teammate", "execution-direct", "processing-1", "processing", null, 3, "completed"),
        sessionItem("teammate", "execution-direct", "thinking-1", "thinking", "Provider thought", 4, "streaming"),
        sessionItem("teammate", "execution-direct", "tool-1", "tool", null, 5, "succeeded"),
        sessionItem("teammate", "execution-direct", "terminal-1", "final_result", "Terminal result", 6, "completed"),
        sessionItem("teammate", "execution-direct", "error-1", "error", null, 7, "failed", null, "provider_error"),
      ], 7));
    });

    expect(screen.getByTestId("team-timeline").textContent).toContain("Final delta");
    expect(screen.getByTestId("team-timeline").textContent).not.toContain("First delta");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Provider thought");
    expect(screen.getByTestId("team-timeline").textContent).toContain("Terminal result");
    expect(screen.getByTestId("team-timeline").textContent).toContain("provider_error");
    expect(screen.getAllByRole("listitem")).toHaveLength(7);

    fireEvent.click(screen.getByTestId("team-member-leader"));
    expect(screen.getByLabelText("Message content").hasAttribute("disabled")).toBe(false);
    fireEvent.click(screen.getByTestId("team-member-teammate"));
    expect(screen.getByTestId("team-timeline").textContent).toContain("Final delta");
    expect(screen.getByTestId("team-member-teammate-status").textContent).toContain("Working");
  });
});

function memberStream(
  memberId: string,
  executionId: string,
  taskState: TeamMemberTaskSnapshot["state"],
  text: string,
): TeamMemberStreamSnapshot {
  return streamSnapshotWithItems(memberId, executionId, taskState, [
    sessionItem(memberId, executionId, `item-${memberId}`, "assistant_text", text, 1, "streaming"),
  ]);
}

function streamSnapshotWithItems(
  memberId: string,
  executionId: string,
  taskState: TeamMemberTaskSnapshot["state"],
  items: TeamMemberStreamSnapshot["stream"]["items"],
  sequence = items.length,
): TeamMemberStreamSnapshot {
  const task: TeamMemberTaskSnapshot = {
    task_id: `task-${executionId}`,
    kind: "TeamRun",
    dedup_key: null,
    state: taskState,
    progress: null,
    error: null,
    started_at: "2026-08-31T00:00:00Z",
    finished_at: ["Succeeded", "Failed", "Canceled"].includes(taskState)
      ? "2026-08-31T00:00:01Z"
      : null,
    detail: {
      workflow: "team_member_turn",
      tenant_id: "tenant-1",
      team_id: "team-1",
      member_id: memberId,
      execution_id: executionId,
      replay: false,
      phase: "prompting",
    },
    result: ["Succeeded", "Failed", "Canceled"].includes(taskState) ? {
      workflow: "team_member_turn",
      team_id: "team-1",
      member_id: memberId,
      execution_id: executionId,
      replay: false,
      terminal: true,
    } : null,
  };
  return {
    team_id: "team-1",
    member_id: memberId,
    execution_id: executionId,
    sequence,
    replay: false,
    task,
    stream: {
      revision: sequence,
      event_count: items.length,
      items,
    },
  };
}

function sessionItem(
  memberId: string,
  executionId: string,
  itemId: string,
  kind: TeamMemberStreamSnapshot["stream"]["items"][number]["kind"],
  text: string | null,
  sequence: number,
  state: TeamMemberStreamSnapshot["stream"]["items"][number]["state"],
  status: TeamMemberStreamSnapshot["stream"]["items"][number]["status"] = null,
  code: string | null = null,
) {
  return {
    identity: {
      session_id: `session-${memberId}`,
      member_id: memberId,
      execution_id: executionId,
      turn_id: executionId,
      item_id: itemId,
    },
    kind,
    sequence,
    delivery: "live" as const,
    state,
    text,
    status,
    code,
  };
}
