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
  TeamMember,
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

// Team with 3 members: teammate-2 (sort_order 10), leader (sort_order 50, but role=leader), teammate-1 (sort_order 5)
const team3: TeamDetail = {
  id: "team-multi",
  name: "Design System Squad",
  description: "Cross-functional team",
  created_at: "2026-09-01T00:00:00Z",
  updated_at: "2026-09-01T00:00:00Z",
  members: [
    {
      id: "member-z",
      team_id: "team-multi",
      role: "teammate",
      sort_order: 10,
      agent_id: "agent-reviewer",
      model: "gpt-4o",
      execution_context_key: "ctx-z",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
    {
      id: "member-leader",
      team_id: "team-multi",
      role: "leader",
      sort_order: 50,
      agent_id: "agent-coordinator",
      model: "claude-3-5-sonnet",
      execution_context_key: "ctx-leader",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
    {
      id: "member-a",
      team_id: "team-multi",
      role: "teammate",
      sort_order: 5,
      agent_id: "agent-coder",
      model: "gemini-1.5-pro",
      execution_context_key: "ctx-a",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
    },
  ],
};

function createProjection(
  memberId: string,
  text: string,
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
    text,
    status: null,
    code: null,
  };

  return {
    team_id: "team-multi",
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

describe("TeamWorkspaceShell Parallel & Single Workspace (T07)", () => {
  let memberProjections: Record<string, TeamMemberSessionProjection>;

  beforeEach(() => {
    storageMock.clear();
    activeMemberChangeMock.mockReset();
    startTurnMock.mockReset().mockResolvedValue({
      execution_id: "new-exec",
      sequence: 2,
      task: { state: "Running" },
      stream: { revision: 2, event_count: 2, items: [] },
    });

    memberProjections = {
      "member-leader": createProjection("member-leader", "Hello from Leader"),
      "member-a": createProjection("member-a", "Hello from Coder A"),
      "member-z": createProjection("member-z", "Hello from Reviewer Z"),
    };

    useTeamSessionMock.mockImplementation(() => ({
      getMember: (id: string) => memberProjections[id] ?? null,
      markSeen: vi.fn(),
      startTurn: startTurnMock,
      startReplay: vi.fn(),
      cancelTurn: vi.fn(),
    }));
  });

  afterEach(() => {
    cleanup();
  });

  it("orders roster with Leader first, then remaining teammates by sort_order", () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    const tabs = screen.getAllByRole("tab");
    expect(tabs.length).toBe(3);
    // 1st: Leader (member-leader, even though sort_order is 50)
    expect(tabs[0].getAttribute("data-testid")).toBe(
      "team-member-member-leader",
    );
    // 2nd: Coder A (member-a, sort_order 5)
    expect(tabs[1].getAttribute("data-testid")).toBe("team-member-member-a");
    // 3rd: Reviewer Z (member-z, sort_order 10)
    expect(tabs[2].getAttribute("data-testid")).toBe("team-member-member-z");
  });

  it("renders parallel lanes for all members in parallel mode on wide screen", () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    // View toggle should exist
    expect(screen.getByTestId("team-view-toggle")).toBeTruthy();
    expect(screen.getByTestId("team-view-toggle-parallel")).toBeTruthy();
    expect(screen.getByTestId("team-view-toggle-single")).toBeTruthy();

    // In parallel mode, all 3 lanes are rendered
    expect(screen.getByTestId("team-member-lane-member-leader")).toBeTruthy();
    expect(screen.getByTestId("team-member-lane-member-a")).toBeTruthy();
    expect(screen.getByTestId("team-member-lane-member-z")).toBeTruthy();

    // Content of each member is visible in its lane
    expect(screen.getByText("Hello from Leader")).toBeTruthy();
    expect(screen.getByText("Hello from Coder A")).toBeTruthy();
    expect(screen.getByText("Hello from Reviewer Z")).toBeTruthy();
  });

  it("enforces 400px min width floor for 3+ members and allows 240px for 2 members", () => {
    // Render 3 members
    const { rerender } = render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    const lane3 = screen.getByTestId("team-member-lane-member-leader");
    expect(lane3.className).toContain("min-w-[400px]");

    // Render 2 members
    const team2: TeamDetail = {
      ...team3,
      members: team3.members.slice(0, 2),
    };

    rerender(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team2}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    const lane2 = screen.getByTestId("team-member-lane-member-leader");
    expect(lane2.className).toContain("min-w-[240px]");
  });

  it("switches to single mode focusing triggered member and switches back to parallel preserving active member", () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    // Switch to single mode
    fireEvent.click(screen.getByTestId("team-view-toggle-single"));

    // In single mode, only the active member lane is rendered
    expect(screen.getByTestId("team-member-lane-member-leader")).toBeTruthy();
    expect(screen.queryByTestId("team-member-lane-member-a")).toBeNull();
    expect(screen.queryByTestId("team-member-lane-member-z")).toBeNull();

    // Click member-a tab in single mode
    fireEvent.click(screen.getByTestId("team-member-member-a"));
    expect(activeMemberChangeMock).toHaveBeenCalledWith("member-a");

    // Switch back to parallel mode
    fireEvent.click(screen.getByTestId("team-view-toggle-parallel"));

    // All lanes rendered again
    expect(screen.getByTestId("team-member-lane-member-leader")).toBeTruthy();
    expect(screen.getByTestId("team-member-lane-member-a")).toBeTruthy();
    expect(screen.getByTestId("team-member-lane-member-z")).toBeTruthy();
  });

  it("provides independent composer and sending per lane", async () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    // Leader lane composer
    const leaderLane = screen.getByTestId("team-member-lane-member-leader");
    const leaderInput = leaderLane.querySelector(
      "textarea",
    ) as HTMLTextAreaElement;
    expect(leaderInput).toBeTruthy();

    // Coder A lane composer
    const aLane = screen.getByTestId("team-member-lane-member-a");
    const aInput = aLane.querySelector("textarea") as HTMLTextAreaElement;
    expect(aInput).toBeTruthy();

    // Typing in Coder A lane does not alter Leader lane
    fireEvent.change(aInput, { target: { value: "Task for Coder A" } });
    expect(aInput.value).toBe("Task for Coder A");
    expect(leaderInput.value).toBe("");

    // Submit message in Coder A lane
    fireEvent.keyDown(aInput, { ctrlKey: true, key: "Enter" });

    await waitFor(() =>
      expect(startTurnMock).toHaveBeenCalledWith(
        "member-a",
        "Task for Coder A",
      ),
    );
  });

  it("handles keyboard navigation across member tabs with ArrowLeft/Right/Home/End", () => {
    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    const leaderTab = screen.getByTestId("team-member-member-leader");
    const aTab = screen.getByTestId("team-member-member-a");
    const zTab = screen.getByTestId("team-member-member-z");

    leaderTab.focus();
    expect(document.activeElement).toBe(leaderTab);

    // ArrowRight moves to next tab
    fireEvent.keyDown(leaderTab, { key: "ArrowRight" });
    expect(document.activeElement).toBe(aTab);

    // End moves to last tab
    fireEvent.keyDown(aTab, { key: "End" });
    expect(document.activeElement).toBe(zTab);

    // Home moves to first tab
    fireEvent.keyDown(zTab, { key: "Home" });
    expect(document.activeElement).toBe(leaderTab);
  });

  it("forces single view on compact screen (<=768px) without overriding desktop preference in localStorage", () => {
    window.localStorage.setItem(
      "assetiweave:team-view-mode:team-multi",
      "parallel",
    );

    window.innerWidth = 500;
    fireEvent(window, new Event("resize"));

    render(
      <I18nProvider>
        <TeamWorkspaceShell
          team={team3}
          activeMemberId="member-leader"
          onActiveMemberChange={activeMemberChangeMock}
          onOpenDetails={vi.fn()}
          onEdit={vi.fn()}
          onDelete={vi.fn()}
          runSnapshot={null}
          workflowBusy={false}
          workflowError={null}
          onStartTeamDraft={vi.fn()}
          onTaskChange={vi.fn()}
          onMoveTask={vi.fn()}
          onReview={vi.fn()}
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </I18nProvider>,
    );

    // On compact screen, single lane is rendered
    expect(screen.getByTestId("team-single-lane")).toBeTruthy();
    expect(screen.queryByTestId("team-parallel-lanes")).toBeNull();

    // Desktop preference in localStorage remains untouched ("parallel")
    expect(
      window.localStorage.getItem("assetiweave:team-view-mode:team-multi"),
    ).toBe("parallel");

    // Restore window width
    window.innerWidth = 1200;
    fireEvent(window, new Event("resize"));
  });
});
