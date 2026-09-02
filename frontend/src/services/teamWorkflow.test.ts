import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelTeamMemberTurn,
  getTeamMemberStreamSnapshot,
  getTeamMemberTask,
  listTeamMemberTasks,
  startTeamMemberReplay,
  startTeamMemberTurn,
  subscribeTeamMemberSessions,
} from "./teamWorkflow";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

describe("team member workflow service", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset().mockResolvedValue(vi.fn());
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  });

  afterEach(() => vi.unstubAllGlobals());

  it("keeps member turn transport typed and uses the snake-case command boundary", async () => {
    const snapshot = streamSnapshot();
    invokeMock
      .mockResolvedValueOnce(snapshot)
      .mockResolvedValueOnce(snapshot)
      .mockResolvedValueOnce(snapshot)
      .mockResolvedValueOnce(snapshot.task)
      .mockResolvedValueOnce([snapshot.task])
      .mockResolvedValueOnce(snapshot);

    await expect(startTeamMemberTurn({
      team_id: "team-1",
      member_id: "member-1",
      message: "hello",
      replay: false,
    })).resolves.toEqual(snapshot);
    await expect(startTeamMemberReplay(" team-1 ", " member-1 ")).resolves.toEqual(snapshot);
    await expect(getTeamMemberStreamSnapshot("team-1", "member-1", "execution-1")).resolves.toEqual(snapshot);
    await expect(getTeamMemberTask(" task-1 ")).resolves.toEqual(snapshot.task);
    await expect(listTeamMemberTasks()).resolves.toEqual([snapshot.task]);
    await expect(cancelTeamMemberTurn("team-1", "member-1", "execution-1")).resolves.toEqual(snapshot);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "team_member_turn_start", {
      input: {
        team_id: "team-1",
        member_id: "member-1",
        message: "hello",
        replay: false,
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "team_member_replay_start", {
      teamId: "team-1",
      memberId: "member-1",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "team_member_stream_snapshot", {
      teamId: "team-1",
      memberId: "member-1",
      executionId: "execution-1",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(6, "team_member_turn_cancel", {
      teamId: "team-1",
      memberId: "member-1",
      executionId: "execution-1",
    });
  });

  it("parses member session events in the service and ignores malformed payloads", async () => {
    const listener = vi.fn();
    let eventListener: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (_name: string, callback: typeof eventListener) => {
      eventListener = callback;
      return vi.fn();
    });

    await subscribeTeamMemberSessions(listener);
    eventListener?.({ payload: streamSnapshot() });
    eventListener?.({ payload: { malformed: true } });

    expect(listenMock).toHaveBeenCalledWith("team-member-session://updated", expect.any(Function));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(streamSnapshot());
  });
});

function streamSnapshot() {
  const task = {
    task_id: "task-1",
    kind: "TeamRun" as const,
    dedup_key: null,
    state: "Running" as const,
    progress: null,
    error: null,
    started_at: "2026-08-31T00:00:00Z",
    finished_at: null,
    detail: {
      workflow: "team_member_turn" as const,
      tenant_id: "tenant-1",
      team_id: "team-1",
      member_id: "member-1",
      execution_id: "execution-1",
      replay: false,
      phase: "prompting",
    },
    result: null,
  };
  return {
    team_id: "team-1",
    member_id: "member-1",
    execution_id: "execution-1",
    sequence: 1,
    replay: false,
    task,
    stream: {
      revision: 1,
      event_count: 1,
      items: [{
        identity: {
          session_id: "session-1",
          member_id: "member-1",
          execution_id: "execution-1",
          turn_id: "turn-1",
          item_id: "item-1",
        },
        kind: "assistant_text" as const,
        sequence: 1,
        delivery: "live" as const,
        state: "streaming" as const,
        text: "hello",
        status: null,
        code: null,
      }],
    },
  };
}
