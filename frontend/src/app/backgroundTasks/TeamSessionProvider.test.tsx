// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TeamSessionProvider, useTeamSession } from "./TeamSessionProvider";
import type { TeamMemberStreamSnapshot, TeamMemberTaskSnapshot } from "../../types/team";

const listTasksMock = vi.hoisted(() => vi.fn());
const getStreamMock = vi.hoisted(() => vi.fn());
const subscribeMock = vi.hoisted(() => vi.fn());
const startTurnMock = vi.hoisted(() => vi.fn());
const startReplayMock = vi.hoisted(() => vi.fn());
const cancelTurnMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/teamWorkflow", () => ({
  cancelTeamMemberTurn: cancelTurnMock,
  getTeamMemberStreamSnapshot: getStreamMock,
  listTeamMemberTasks: listTasksMock,
  startTeamMemberReplay: startReplayMock,
  startTeamMemberTurn: startTurnMock,
  subscribeTeamMemberSessions: subscribeMock,
}));

describe("TeamSessionProvider", () => {
  beforeEach(() => {
    listTasksMock.mockReset().mockResolvedValue([]);
    getStreamMock.mockReset().mockResolvedValue(null);
    subscribeMock.mockReset().mockResolvedValue(vi.fn());
    startTurnMock.mockReset();
    startReplayMock.mockReset();
    cancelTurnMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("keeps each member timeline separate and merges a missed event through polling", async () => {
    vi.useFakeTimers();
    const leaderRunning = streamSnapshot("leader", "execution-leader", "Running", [item("leader-item", "leader text")]);
    const teammateRunning = streamSnapshot("teammate", "execution-teammate", "Running", [item("teammate-item", "teammate text")]);
    const leaderSucceeded = streamSnapshot("leader", "execution-leader", "Succeeded", [item("leader-item", "leader final")], 2);
    listTasksMock
      .mockResolvedValueOnce([leaderRunning.task, teammateRunning.task])
      .mockResolvedValueOnce([leaderSucceeded.task, teammateRunning.task]);
    getStreamMock
      .mockResolvedValueOnce(leaderRunning)
      .mockResolvedValueOnce(teammateRunning)
      .mockResolvedValueOnce(leaderSucceeded)
      .mockResolvedValueOnce(teammateRunning);

    render(
      <TeamSessionProvider teamId="team-1">
        <Harness />
      </TeamSessionProvider>,
    );
    await act(async () => {});
    expect(screen.getByTestId("leader").textContent).toBe("leader text:Running");
    expect(screen.getByTestId("teammate").textContent).toBe("teammate text:Running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("leader").textContent).toBe("leader final:Succeeded");
    expect(screen.getByTestId("teammate").textContent).toBe("teammate text:Running");
    expect(screen.getByTestId("leader-unread").textContent).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Mark leader seen" }));
    expect(screen.getByTestId("leader-unread").textContent).toBe("false");
  });

  it("cleans up the old scoped listener when the Team changes", async () => {
    const unsubscribers = [vi.fn(), vi.fn()];
    subscribeMock
      .mockResolvedValueOnce(unsubscribers[0])
      .mockResolvedValueOnce(unsubscribers[1]);
    const view = render(
      <TeamSessionProvider teamId="team-1">
        <Harness />
      </TeamSessionProvider>,
    );
    await act(async () => {});
    view.rerender(
      <TeamSessionProvider teamId="team-2">
        <Harness />
      </TeamSessionProvider>,
    );
    await act(async () => {});

    expect(unsubscribers[0]).toHaveBeenCalledTimes(1);
    expect(subscribeMock).toHaveBeenCalledTimes(2);
  });

  it("starts, replays, and cancels through typed member actions", async () => {
    const running = streamSnapshot("leader", "execution-leader", "Running", []);
    const replayed = streamSnapshot("leader", "execution-replay", "Succeeded", [item("replay-item", "history")], 1, true);
    startTurnMock.mockResolvedValue(running);
    startReplayMock.mockResolvedValue(replayed);
    cancelTurnMock.mockResolvedValue({ ...running, task: { ...running.task, state: "Canceled" } });

    render(
      <TeamSessionProvider teamId="team-1">
        <Harness />
      </TeamSessionProvider>,
    );
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() => expect(screen.getByTestId("leader").textContent).toContain("Running"));
    fireEvent.click(screen.getByRole("button", { name: "Replay" }));
    await waitFor(() => expect(screen.getByTestId("restore").textContent).toBe("ready"));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(cancelTurnMock).toHaveBeenCalledWith("team-1", "leader", "execution-leader"));
  });
});

function Harness() {
  const session = useTeamSession();
  const leader = session.getMember("leader");
  const teammate = session.getMember("teammate");
  return (
    <>
      <output data-testid="leader">{leader?.stream.items[0]?.text ?? "empty"}:{leader?.task?.state ?? "none"}</output>
      <output data-testid="teammate">{teammate?.stream.items[0]?.text ?? "empty"}:{teammate?.task?.state ?? "none"}</output>
      <output data-testid="restore">{leader?.restore_state ?? "not-started"}</output>
      <output data-testid="leader-unread">{leader?.unread ? "true" : "false"}</output>
      <button onClick={() => void session.startTurn("leader", "hello")} type="button">Start</button>
      <button onClick={() => void session.startReplay("leader")} type="button">Replay</button>
      <button onClick={() => leader?.execution_id && void session.cancelTurn("leader", leader.execution_id)} type="button">Cancel</button>
      <button onClick={() => session.markSeen("leader")} type="button">Mark leader seen</button>
    </>
  );
}

function streamSnapshot(
  memberId: string,
  executionId: string,
  taskState: TeamMemberTaskSnapshot["state"],
  items: TeamMemberStreamSnapshot["stream"]["items"],
  sequence = 1,
  replay = false,
): TeamMemberStreamSnapshot {
  return {
    team_id: "team-1",
    member_id: memberId,
    execution_id: executionId,
    sequence,
    replay,
    task: {
      task_id: `task-${executionId}`,
      kind: "TeamRun",
      dedup_key: null,
      state: taskState,
      progress: null,
      error: null,
      started_at: "2026-08-31T00:00:00Z",
      finished_at: taskState === "Succeeded" || taskState === "Canceled" ? "2026-08-31T00:00:01Z" : null,
      detail: {
        workflow: "team_member_turn",
        tenant_id: "tenant-1",
        team_id: "team-1",
        member_id: memberId,
        execution_id: executionId,
        replay,
        phase: taskState === "Succeeded" ? "cleaning_up" : "prompting",
      },
      result: taskState === "Succeeded" ? {
        workflow: "team_member_turn",
        team_id: "team-1",
        member_id: memberId,
        execution_id: executionId,
        replay,
        terminal: true,
      } : null,
    },
    stream: { revision: sequence, event_count: items.length, items },
  };
}

function item(itemId: string, text: string) {
  return {
    identity: {
      session_id: "session-1",
      member_id: "leader",
      execution_id: "execution-leader",
      turn_id: "turn-1",
      item_id: itemId,
    },
    kind: "assistant_text" as const,
    sequence: 1,
    delivery: "live" as const,
    state: "streaming" as const,
    text,
    status: null,
    code: null,
  };
}
