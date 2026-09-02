import { describe, expect, it } from "vitest";
import {
  applyTeamMemberStreamSnapshot,
  createTeamSessionStoreState,
  selectTeamMemberSession,
} from "./TeamSessionStore";
import type {
  SessionItemSnapshot,
  TeamMemberStreamSnapshot,
  TeamMemberTaskSnapshot,
} from "../../types/team";

describe("TeamSessionStore", () => {
  it("keeps member timelines isolated and ignores an older duplicate snapshot", () => {
    let state = createTeamSessionStoreState("team-1");
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 1, [
      item("assistant-a", "assistant_text", "hello", 1),
    ]));
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("teammate", "execution-b", 1, [
      item("assistant-b", "assistant_text", "teammate", 1),
    ]));
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 2, [
      item("assistant-a", "assistant_text", "hello world", 2),
    ]));
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 1, [
      item("assistant-a", "assistant_text", "stale", 1),
    ]));

    expect(selectTeamMemberSession(state, "team-1", "leader")?.stream.items).toHaveLength(1);
    expect(selectTeamMemberSession(state, "team-1", "leader")?.stream.items[0]?.text).toBe("hello world");
    expect(selectTeamMemberSession(state, "team-1", "teammate")?.stream.items[0]?.text).toBe("teammate");
  });

  it("merges tool lifecycle and replay/live overlap into one logical item", () => {
    let state = createTeamSessionStoreState("team-1");
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 1, [
      item("tool-a", "tool", "provider payload", 1, "pending", "replay"),
    ], true));
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 2, [
      item("tool-a", "tool", null, 2, "succeeded", "live"),
    ]));

    const session = selectTeamMemberSession(state, "team-1", "leader");
    expect(session?.stream.items).toHaveLength(1);
    expect(session?.stream.items[0]).toMatchObject({
      delivery: "live",
      state: "succeeded",
      text: null,
    });
  });

  it("keeps the latest task status even when a task update reuses the stream sequence", () => {
    let state = createTeamSessionStoreState("team-1");
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 3, [], false, "Running"));
    state = applyTeamMemberStreamSnapshot(state, streamSnapshot("leader", "execution-a", 3, [], false, "Succeeded"));

    expect(selectTeamMemberSession(state, "team-1", "leader")?.task?.state).toBe("Succeeded");
    expect(selectTeamMemberSession(state, "team-1", "leader")?.restore_state).toBe("ready");
  });

  it("exposes restoring, partial, and unavailable recovery states and bounds the timeline", () => {
    let restoring = createTeamSessionStoreState("team-1");
    restoring = applyTeamMemberStreamSnapshot(restoring, streamSnapshot("leader", "replay-running", 1, [], true));
    expect(selectTeamMemberSession(restoring, "team-1", "leader")?.restore_state).toBe("restoring");

    let partial = createTeamSessionStoreState("team-1");
    partial = applyTeamMemberStreamSnapshot(partial, streamSnapshot("leader", "replay-partial", 1, [], true, "Succeeded"));
    expect(selectTeamMemberSession(partial, "team-1", "leader")?.restore_state).toBe("partial");

    let unavailable = createTeamSessionStoreState("team-1");
    unavailable = applyTeamMemberStreamSnapshot(unavailable, {
      ...streamSnapshot("leader", "replay-failed", 1, [], true, "Failed"),
      task: {
        ...streamSnapshot("leader", "replay-failed", 1, [], true, "Failed").task,
        error: { code: "resume_unavailable", message: "unavailable", retryable: false },
      },
    });
    expect(selectTeamMemberSession(unavailable, "team-1", "leader")?.restore_state).toBe("unavailable");

    const manyItems = Array.from({ length: 300 }, (_, index) => item(`item-${index}`, "assistant_text", `${index}`, index + 1));
    let bounded = createTeamSessionStoreState("team-1");
    bounded = applyTeamMemberStreamSnapshot(bounded, streamSnapshot("leader", "execution-many", 300, manyItems));
    expect(selectTeamMemberSession(bounded, "team-1", "leader")?.stream.items).toHaveLength(256);
  });
});

function streamSnapshot(
  memberId: string,
  executionId: string,
  sequence: number,
  items: SessionItemSnapshot[],
  replay = false,
  taskState: TeamMemberTaskSnapshot["state"] = "Running",
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
      finished_at: taskState === "Succeeded" ? "2026-08-31T00:00:01Z" : null,
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
    stream: {
      revision: sequence,
      event_count: items.length,
      items,
    },
  };
}

function item(
  itemId: string,
  kind: SessionItemSnapshot["kind"],
  text: string | null,
  sequence: number,
  state: SessionItemSnapshot["state"] = "streaming",
  delivery: SessionItemSnapshot["delivery"] = "live",
): SessionItemSnapshot {
  return {
    identity: {
      session_id: "session-1",
      member_id: "leader",
      execution_id: "execution-a",
      turn_id: "turn-1",
      item_id: itemId,
    },
    kind,
    sequence,
    delivery,
    state,
    text,
    status: null,
    code: null,
  };
}
