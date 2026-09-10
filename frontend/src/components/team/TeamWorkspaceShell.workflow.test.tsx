// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type {
  SessionItemSnapshot,
  TeamDetail,
  TeamMemberSessionProjection,
  TeamRunSnapshot,
  TeamTask,
} from "../../types/team";
import { TeamWorkspaceShell } from "./TeamWorkspaceShell";

const useTeamSessionMock = vi.hoisted(() => vi.fn());
const startTurnMock = vi.hoisted(() => vi.fn());
const cancelTurnMock = vi.hoisted(() => vi.fn());
const activeMemberChangeMock = vi.hoisted(() => vi.fn());
const startTeamDraftMock = vi.hoisted(() => vi.fn());
const onTaskChangeMock = vi.hoisted(() => vi.fn());
const onMoveTaskMock = vi.hoisted(() => vi.fn());
const onReviewMock = vi.hoisted(() => vi.fn());
const onConfirmMock = vi.hoisted(() => vi.fn());
const onCancelMock = vi.hoisted(() => vi.fn());

vi.mock("../../app/backgroundTasks/TeamSessionProvider", () => ({
  useTeamSession: useTeamSessionMock,
}));

const team: TeamDetail = {
  id: "team-workflow",
  name: "Workflow Squad",
  description: "Workflow verification squad",
  created_at: "2026-09-01T00:00:00Z",
  updated_at: "2026-09-01T00:00:00Z",
  members: [
    {
      id: "leader",
      team_id: "team-workflow",
      role: "leader",
      sort_order: 0,
      agent_id: "agent-leader",
      model: "claude-3-5-sonnet",
      execution_context_key: "ctx-leader",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
    {
      id: "teammate",
      team_id: "team-workflow",
      role: "teammate",
      sort_order: 1,
      agent_id: "agent-worker",
      model: "gpt-4o",
      execution_context_key: "ctx-teammate",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
  ],
};

function createProjection(
  memberId: string,
  overrides?: Partial<TeamMemberSessionProjection>,
): TeamMemberSessionProjection {
  const item: SessionItemSnapshot = {
    identity: {
      session_id: `session-${memberId}`,
      member_id: memberId,
      execution_id: `exec-${memberId}`,
      turn_id: `turn-${memberId}`,
      item_id: `item-${memberId}-1`,
    },
    kind: "assistant_text",
    sequence: 1,
    delivery: "live",
    state: "completed",
    text: `History for ${memberId}`,
    status: null,
    code: null,
  };

  return {
    team_id: "team-workflow",
    member_id: memberId,
    execution_id: `exec-${memberId}`,
    sequence: 1,
    replay: false,
    task: null,
    unread: false,
    restore_state: "ready",
    restore_error_code: null,
    executions: {},
    stream: {
      revision: 1,
      event_count: 1,
      items: [item],
    },
    ...overrides,
  };
}

const storageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value);
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
  };
})();

Object.defineProperty(window, "localStorage", {
  value: storageMock,
  writable: true,
});

describe("TeamWorkspaceShell Workflow & Migration (T08)", () => {
  let memberProjections: Record<string, TeamMemberSessionProjection>;

  beforeEach(() => {
    storageMock.clear();
    startTurnMock.mockReset().mockResolvedValue({
      execution_id: "new-exec",
      sequence: 2,
      task: { state: "Running" },
      stream: { revision: 2, event_count: 2, items: [] },
    });
    cancelTurnMock.mockReset().mockResolvedValue(undefined);
    activeMemberChangeMock.mockReset();
    startTeamDraftMock.mockReset();
    onTaskChangeMock.mockReset();
    onMoveTaskMock.mockReset();
    onReviewMock.mockReset();
    onConfirmMock.mockReset();
    onCancelMock.mockReset();

    memberProjections = {
      leader: createProjection("leader"),
      teammate: createProjection("teammate"),
    };

    useTeamSessionMock.mockImplementation(() => ({
      getMember: (id: string) => memberProjections[id] ?? null,
      markSeen: vi.fn(),
      startTurn: startTurnMock,
      startReplay: vi.fn(),
      cancelTurn: cancelTurnMock,
    }));
  });

  afterEach(() => {
    cleanup();
  });

  it("exposes normal/task mode toggle in Leader lane and omits task mode in Teammate lane", () => {
    // Parallel mode shows both lanes
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    // Leader lane has composer mode toggle with Normal and Task mode
    const leaderLane = screen.getByTestId("team-member-lane-leader");
    expect(leaderLane.querySelector("[aria-label='Message mode']")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Team task/i })).toBeTruthy();

    // Teammate lane does NOT have composer mode toggle
    const teammateLane = screen.getByTestId("team-member-lane-teammate");
    expect(teammateLane.querySelector("[aria-label='Message mode']")).toBeNull();
  });

  it("starts a team draft when submitting in Leader task mode", () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    // Switch to Task mode
    const taskModeBtn = screen.getByRole("button", { name: /Team task/i });
    fireEvent.click(taskModeBtn);

    // Enter draft prompt in Leader composer
    const composers = screen.getAllByLabelText("Message content");
    const leaderComposer = composers[0];
    fireEvent.change(leaderComposer, { target: { value: "Draft team plan for v2" } });

    // Click submit (Generate draft)
    const draftSubmitBtn = screen.getByRole("button", { name: /Generate draft|Draft/i });
    fireEvent.click(draftSubmitBtn);

    expect(startTeamDraftMock).toHaveBeenCalledWith("Draft team plan for v2");
  });

  it("renders Plan card only on the Leader lane during awaiting_review", () => {
    const runSnapshot: TeamRunSnapshot = {
      run: {
        id: "run-1",
        team_id: "team-workflow",
        state: "awaiting_review",
        revision: 2,
        leader_member_id: "leader",
        roster_snapshot: [],
        created_at: "2026-09-01T00:00:00Z",
        updated_at: "2026-09-01T00:00:00Z",
        finished_at: null,
        error_code: null,
      },
      tasks: [
        {
          id: "task-1",
          run_id: "run-1",
          team_id: "team-workflow",
          title: "Setup Database",
          description: "Initialize tables",
          sort_order: 0,
          recommended_member_id: "teammate",
          owner_member_id: "teammate",
          state: "draft",
          revision: 1,
          result: null,
          error_code: null,
          created_at: "2026-09-01T00:00:00Z",
          updated_at: "2026-09-01T00:00:00Z",
        },
      ],
      unread_mailbox_count: 0,
    };

    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={runSnapshot}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    const leaderLane = screen.getByTestId("team-member-lane-leader");
    const teammateLane = screen.getByTestId("team-member-lane-teammate");

    expect(leaderLane.querySelector("[data-testid='team-plan-card']")).toBeTruthy();
    expect(teammateLane.querySelector("[data-testid='team-plan-card']")).toBeNull();
  });

  it("projects confirmed tasks only to their respective owner's lane", () => {
    const runSnapshot: TeamRunSnapshot = {
      run: {
        id: "run-1",
        team_id: "team-workflow",
        state: "executing",
        revision: 3,
        leader_member_id: "leader",
        roster_snapshot: [],
        created_at: "2026-09-01T00:00:00Z",
        updated_at: "2026-09-01T00:00:00Z",
        finished_at: null,
        error_code: null,
      },
      tasks: [
        {
          id: "task-worker",
          run_id: "run-1",
          team_id: "team-workflow",
          title: "Build backend",
          description: "Rust implementation",
          sort_order: 0,
          recommended_member_id: "teammate",
          owner_member_id: "teammate",
          state: "running",
          revision: 2,
          result: null,
          error_code: null,
          created_at: "2026-09-01T00:00:00Z",
          updated_at: "2026-09-01T00:00:00Z",
        },
      ],
      unread_mailbox_count: 0,
    };

    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={runSnapshot}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    const leaderLane = screen.getByTestId("team-member-lane-leader");
    const teammateLane = screen.getByTestId("team-member-lane-teammate");

    // Task card is in teammate lane, NOT leader lane
    expect(teammateLane.querySelector("[data-testid='team-task-card-task-worker']")).toBeTruthy();
    expect(leaderLane.querySelector("[data-testid='team-task-card-task-worker']")).toBeNull();
  });

  it("cancelling or stopping an executing member only targets that specific member", async () => {
    memberProjections = {
      leader: createProjection("leader"),
      teammate: createProjection("teammate", {
        execution_id: "exec-worker-running",
        task: {
          task_id: "task-worker",
          kind: "TeamRun",
          dedup_key: "dedup-worker",
          state: "Running",
          progress: null,
          error: null,
          started_at: "2026-09-01T00:00:00Z",
          finished_at: null,
          detail: {
            workflow: "team_member_turn",
            tenant_id: "tenant-1",
            team_id: "team-workflow",
            member_id: "teammate",
            execution_id: "exec-worker-running",
            replay: false,
            phase: "running",
          },
          result: null,
        } as any,
      }),
    };

    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    // Teammate lane has a Stop button
    const teammateLane = screen.getByTestId("team-member-lane-teammate");
    const stopBtn = teammateLane.querySelector("[data-testid='team-teammate-stop']");
    expect(stopBtn).toBeTruthy();

    fireEvent.click(stopBtn!);

    await waitFor(() => {
      expect(cancelTurnMock).toHaveBeenCalledWith("teammate", "exec-worker-running");
    });
    // Ensure leader turn was NOT cancelled
    expect(cancelTurnMock).not.toHaveBeenCalledWith("leader", expect.anything());
  });

  it("preserves historical messages when member is unavailable or restore failed", () => {
    memberProjections = {
      leader: createProjection("leader"),
      teammate: createProjection("teammate", {
        restore_state: "unavailable",
        restore_error_code: "provider_offline",
      }),
    };

    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team}
          activeMemberId="leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={startTeamDraftMock}
          onTaskChange={onTaskChangeMock}
          onMoveTask={onMoveTaskMock}
          onReview={onReviewMock}
          onConfirm={onConfirmMock}
          onCancel={onCancelMock}
        />
      </I18nProvider>,
    );

    const teammateLane = screen.getByTestId("team-member-lane-teammate");
    // History is still visible in the lane
    expect(teammateLane.textContent).toContain("History for teammate");
    // Status reflects unavailable
    expect(teammateLane.textContent).toContain("Unavailable");
  });
});
