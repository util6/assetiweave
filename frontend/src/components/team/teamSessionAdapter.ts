import type { ReactNode } from "react";
import {
  DEFAULT_INTERACTIVE_CAPABILITIES,
  type AgentSessionCapabilities,
  type AgentSessionItemView,
  type AgentSessionRestoreStateInfo,
  type AgentSessionStatus,
  type AgentSessionWorkspaceProps,
} from "../../types/agentSession";
import type {
  SessionItemSnapshot,
  TeamMember,
  TeamMemberSessionProjection,
} from "../../types/team";

export interface AdaptTeamSessionOptions {
  activeMember: TeamMember | null;
  activeSession: TeamMemberSessionProjection | null;
  activeTimelineItems: SessionItemSnapshot[];
  roleLabelText: string;
  status: AgentSessionStatus;
  draft: string;
  onDraftChange: (draft: string) => void;
  onSend: (message: string) => void | Promise<void>;
  canSend?: boolean;
  disabled?: boolean;
  isLeader?: boolean;
  placeholder?: string;
  submitLabel?: string;
  restoreStatus?: AgentSessionRestoreStateInfo | null;
  timelineExtra?: ReactNode;
  composerExtra?: ReactNode;
  activityDependencyKey?: string;
  sessionResetKey?: string;
  testIdPrefix?: string;
}

export function mapSessionItemSnapshotToView(
  item: SessionItemSnapshot,
): AgentSessionItemView {
  return {
    id: item.identity.item_id,
    kind: item.kind,
    sequence: item.sequence,
    delivery: item.delivery,
    state: item.state,
    text: item.text,
    status: item.status,
    code: item.code,
    toolCallId: item.tool_call_id,
    toolName: item.tool_name,
    toolInput: item.tool_input,
    toolOutput: item.tool_output,
  };
}

export function adaptTeamSessionToWorkspaceProps({
  activeMember,
  activeTimelineItems,
  activityDependencyKey,
  canSend,
  composerExtra,
  disabled = false,
  draft,
  onDraftChange,
  onSend,
  placeholder,
  restoreStatus,
  roleLabelText,
  sessionResetKey,
  status,
  submitLabel,
  testIdPrefix = "team",
  timelineExtra,
}: AdaptTeamSessionOptions): AgentSessionWorkspaceProps {
  const capabilities: AgentSessionCapabilities = {
    ...DEFAULT_INTERACTIVE_CAPABILITIES,
    send: Boolean(activeMember),
  };

  return {
    capabilities,
    items: activeTimelineItems.map(mapSessionItemSnapshotToView),
    recipientTitle: roleLabelText,
    status,
    restoreState: restoreStatus,
    draft,
    onDraftChange,
    onSend,
    canSend,
    disabled,
    placeholder,
    submitLabel,
    timelineExtra,
    composerExtra,
    activityDependencyKey,
    sessionResetKey,
    testIdPrefix,
  };
}
