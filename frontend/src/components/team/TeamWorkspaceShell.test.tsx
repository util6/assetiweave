// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type {
  SessionItemSnapshot,
  TeamDetail,
  TeamMemberSessionProjection,
  TeamMemberStreamSnapshot,
} from "../../types/team";
import { TeamWorkspaceShell } from "./TeamWorkspaceShell";

const useTeamSessionMock = vi.hoisted(() => vi.fn());
const activeMemberChangeMock = vi.hoisted(() => vi.fn());
const startTurnMock = vi.hoisted(() => vi.fn());

vi.mock("../../app/backgroundTasks/TeamSessionProvider", () => ({
  useTeamSession: useTeamSessionMock,
}));

const team: TeamDetail = {
  id: "team-1",
  name: "UX crew",
  description: "A focused team",
  created_at: "2026-09-01T00:00:00Z",
  updated_at: "2026-09-01T00:00:00Z",
  members: [
    {
      id: "leader",
      team_id: "team-1",
      role: "leader",
      sort_order: 0,
      agent_id: "agent-a",
      model: "model-a",
      execution_context_key: "ctx-leader",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
    {
      id: "teammate",
      team_id: "team-1",
      role: "teammate",
      sort_order: 1,
      agent_id: "agent-b",
      model: "model-b",
      execution_context_key: "ctx-teammate",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
  ],
};

let projection: TeamMemberSessionProjection;

describe("TeamWorkspaceShell", () => {
  beforeEach(() => {
    projection = memberProjection("first");
    activeMemberChangeMock.mockReset();
    startTurnMock.mockReset().mockResolvedValue(streamSnapshot("accepted"));
    useTeamSessionMock.mockImplementation(() => ({
      getMember: () => projection,
      markSeen: vi.fn(),
      startTurn: startTurnMock,
      startReplay: vi.fn(),
      cancelTurn: vi.fn(),
    }));
  });

  afterEach(() => cleanup());

  it("follows new activity when the timeline is near the bottom", async () => {
    const view = renderShell();
    const timeline = configureTimeline();
    await act(async () => {});
    const scrollTo = timeline.scrollTo as unknown as ReturnType<typeof vi.fn>;
    scrollTo.mockClear();

    timeline.scrollTop = 400;
    fireEvent.scroll(timeline);
    projection = memberProjection("second");
    view.rerender(
      <I18nProvider>
        <TeamWorkspaceShell {...shellProps()} />
      </I18nProvider>,
    );

    expect(screen.queryByTestId("team-new-activity")).toBeNull();
    expect(scrollTo).toHaveBeenCalledWith({ behavior: "auto", top: 400 });
  });

  it("keeps the reader position and exposes a new activity affordance away from the bottom", async () => {
    const view = renderShell();
    const timeline = configureTimeline();
    await act(async () => {});
    const scrollTo = timeline.scrollTo as unknown as ReturnType<typeof vi.fn>;
    scrollTo.mockClear();

    timeline.scrollTop = 120;
    fireEvent.scroll(timeline);
    projection = memberProjection("second");
    view.rerender(
      <I18nProvider>
        <TeamWorkspaceShell {...shellProps()} />
      </I18nProvider>,
    );

    expect(screen.getByTestId("team-new-activity")).toBeTruthy();
    expect(scrollTo).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("team-new-activity"));
    expect(scrollTo).toHaveBeenCalledWith({ behavior: "smooth", top: 400 });
    expect(screen.queryByTestId("team-new-activity")).toBeNull();
  });

  it("moves member focus with arrow keys and keeps tool details collapsible", async () => {
    const view = renderShell();
    const leaderTab = screen.getByTestId("team-member-leader");
    const teammateTab = screen.getByTestId("team-member-teammate");
    leaderTab.focus();
    fireEvent.keyDown(leaderTab, { key: "ArrowDown" });

    expect(document.activeElement).toBe(teammateTab);
    expect(activeMemberChangeMock).toHaveBeenCalledWith("teammate");

    projection = toolProjection();
    view.rerender(
      <I18nProvider>
        <TeamWorkspaceShell {...shellProps()} />
      </I18nProvider>,
    );
    const details = screen.getByTestId("team-session-item-details-tool-1");
    expect(details.hasAttribute("open")).toBe(false);
    fireEvent.click(details.querySelector("summary") as HTMLElement);
    expect(details.hasAttribute("open")).toBe(true);
  });

  it("sends from the composer with the keyboard shortcut", async () => {
    projection = {
      ...memberProjection("first"),
      task: { ...memberProjection("first").task!, state: "Succeeded" },
    };
    renderShell();
    const composer = screen.getByLabelText("Message content");
    fireEvent.change(composer, { target: { value: "Keyboard message" } });
    fireEvent.keyDown(composer, { ctrlKey: true, key: "Enter" });

    await waitFor(() => expect(startTurnMock).toHaveBeenCalledWith("leader", "Keyboard message"));
  });
});

function renderShell() {
  return render(
    <I18nProvider>
      <TeamWorkspaceShell {...shellProps()} />
    </I18nProvider>,
  );
}

function shellProps() {
  return {
    team,
    activeMemberId: "leader",
    onActiveMemberChange: activeMemberChangeMock,
    onOpenDetails: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    runSnapshot: null,
    workflowBusy: false,
    workflowError: null,
    restoreTask: null,
    restoreResult: null,
    onStartTeamDraft: vi.fn(),
    onTaskChange: vi.fn(),
    onMoveTask: vi.fn(),
    onReview: vi.fn(),
    onConfirm: vi.fn(),
    onRestore: vi.fn(),
    onCancel: vi.fn(),
  };
}

function configureTimeline() {
  const timeline = screen.getByTestId("team-timeline");
  const scrollTo = vi.fn();
  Object.defineProperty(timeline, "clientHeight", { configurable: true, value: 100 });
  Object.defineProperty(timeline, "scrollHeight", { configurable: true, value: 500 });
  Object.defineProperty(timeline, "scrollTo", { configurable: true, value: scrollTo });
  return timeline;
}

function memberProjection(text: string): TeamMemberSessionProjection {
  const snapshot = streamSnapshot(text);
  return {
    team_id: "team-1",
    member_id: "leader",
    execution_id: snapshot.execution_id,
    sequence: snapshot.sequence,
    replay: false,
    stream: snapshot.stream,
    task: snapshot.task,
    unread: false,
    restore_state: "ready",
    restore_error_code: null,
    executions: {},
  };
}

function toolProjection(): TeamMemberSessionProjection {
  const current = memberProjection("tool");
  const tool: SessionItemSnapshot = {
    identity: {
      session_id: "session-leader",
      member_id: "leader",
      execution_id: "execution-leader",
      turn_id: "turn-leader",
      item_id: "tool-1",
    },
    kind: "tool",
    sequence: 2,
    delivery: "live",
    state: "succeeded",
    text: null,
    status: null,
    code: null,
  };
  return { ...current, stream: { ...current.stream, items: [tool] } };
}

function streamSnapshot(text: string): TeamMemberStreamSnapshot {
  const item: SessionItemSnapshot = {
    identity: {
      session_id: "session-leader",
      member_id: "leader",
      execution_id: "execution-leader",
      turn_id: "turn-leader",
      item_id: "assistant-1",
    },
    kind: "assistant_text",
    sequence: text === "first" ? 1 : 2,
    delivery: "live",
    state: "streaming",
    text,
    status: null,
    code: null,
  };
  return {
    team_id: "team-1",
    member_id: "leader",
    execution_id: "execution-leader",
    sequence: item.sequence,
    replay: false,
    task: {
      task_id: "task-leader",
      kind: "TeamRun",
      dedup_key: "dedup-leader",
      state: "Running",
      progress: null,
      error: null,
      started_at: "2026-09-01T00:00:00Z",
      finished_at: null,
      detail: {
        workflow: "team_member_turn",
        tenant_id: "tenant-1",
        team_id: "team-1",
        member_id: "leader",
        execution_id: "execution-leader",
        replay: false,
        phase: "prompting",
      },
      result: null,
    },
    stream: { revision: item.sequence, event_count: item.sequence, items: [item] },
  };
}
