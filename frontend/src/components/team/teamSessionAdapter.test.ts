import { describe, expect, it, vi } from "vitest";
import type {
  SessionItemSnapshot,
  TeamMember,
  TeamMemberSessionProjection,
} from "../../types/team";
import { adaptTeamSessionToWorkspaceProps } from "./teamSessionAdapter";

describe("teamSessionAdapter", () => {
  const member: TeamMember = {
    id: "member-1",
    team_id: "team-1",
    role: "teammate",
    sort_order: 0,
    agent_id: "researcher",
    model: "claude-3-5",
    execution_context_key: "ctx-1",
    created_at: "2026-09-01T00:00:00Z",
    updated_at: "2026-09-01T00:00:00Z",
  };

  const rawItem: SessionItemSnapshot = {
    identity: {
      session_id: "s-1",
      member_id: "member-1",
      execution_id: "e-1",
      turn_id: "t-1",
      item_id: "i-1",
    },
    kind: "assistant_text",
    sequence: 1,
    delivery: "live",
    state: "completed",
    text: "Analysis completed.",
    status: null,
    code: null,
  };

  const projection: TeamMemberSessionProjection = {
    team_id: "team-1",
    member_id: "member-1",
    execution_id: "e-1",
    sequence: 1,
    replay: false,
    restore_state: "ready",
    restore_error_code: null,
    unread: false,
    task: null,
    executions: {},
    stream: {
      revision: 1,
      event_count: 1,
      items: [rawItem],
    },
  };

  it("adapts Team member projection to domain-neutral workspace props", () => {
    const onSend = vi.fn();
    const onDraftChange = vi.fn();

    const props = adaptTeamSessionToWorkspaceProps({
      activeMember: member,
      activeSession: projection,
      activeTimelineItems: [rawItem],
      draft: "Next step",
      isLeader: false,
      onDraftChange,
      onSend,
      roleLabelText: "Teammate",
      status: {
        className: "text-status-create",
        label: "Ready",
      },
      testIdPrefix: "team",
    });

    expect(props.recipientTitle).toBe("Teammate");
    expect(props.status?.label).toBe("Ready");
    expect(props.draft).toBe("Next step");
    expect(props.capabilities.send).toBe(true);
    expect(props.items).toHaveLength(1);
    expect(props.items[0]).toEqual({
      id: "i-1",
      kind: "assistant_text",
      sequence: 1,
      delivery: "live",
      state: "completed",
      text: "Analysis completed.",
      status: null,
      code: null,
    });

    props.onSend?.("Next step");
    expect(onSend).toHaveBeenCalledWith("Next step");
  });
});
