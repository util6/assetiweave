import type { ReactNode } from "react";
import type { SessionItemSnapshot } from "./team";

// ==========================================
// DTO & Backend Execution Scene Projections
// ==========================================

export interface AgentSessionRef {
  schemaVersion: number;
  value: string;
}

export interface AgentInfoView {
  id: string;
  displayName?: string | null;
  model?: string | null;
  protocol: string;
}

export interface AgentSessionContextView {
  teamId?: string | null;
  memberId?: string | null;
  memoryScope?: string | null;
  memoryJobId?: string | null;
  taskId?: string | null;
}

export interface AgentSessionTerminalView {
  state: string;
  code?: string | null;
  message?: string | null;
  retryable: boolean;
}

export interface AgentSessionCapabilitiesView {
  read: boolean;
  send: boolean;
  stop: boolean;
  retry: boolean;
  queue: boolean;
  interrupt: boolean;
  attach: boolean;
  mention: boolean;
  slashCommand: boolean;
  modelSelect?: boolean;
  permissionResponse?: boolean;
  copy?: boolean;
  openArtifact?: boolean;
}

export interface SessionRetentionView {
  maxItems: number;
  maxEvents: number;
  maxBytes: number;
  truncated: boolean;
  evictedItemCount: number;
  rejectedEventCount: number;
}

export interface AgentSessionUnavailableView {
  schemaVersion: number;
  sessionRef: AgentSessionRef;
  state: "unavailable";
  reason: string;
}

export interface AgentSessionView {
  schemaVersion: number;
  sessionRef: AgentSessionRef;
  executionId: string;
  purpose: string;
  mode: string;
  tenantId?: string | null;
  agent: AgentInfoView;
  context: AgentSessionContextView;
  state: "active" | "terminal" | string;
  terminal?: AgentSessionTerminalView | null;
  capabilities: AgentSessionCapabilitiesView;
  revision: number;
  eventCount: number;
  items: SessionItemSnapshot[];
  retention: SessionRetentionView;
  startedAt?: string | null;
  updatedAt: string;
  finishedAt?: string | null;
}

export type AgentSessionGetResult =
  AgentSessionUnavailableView | AgentSessionView;

export function isAgentSessionAvailable(
  result: AgentSessionGetResult | null | undefined,
): result is AgentSessionView {
  return Boolean(result && result.state !== "unavailable");
}

export interface AgentSessionGetParams {
  sessionRef: AgentSessionRef;
}

export interface AgentSessionUpdatedEvent {
  sessionRef: AgentSessionRef;
  revision: number;
}

// ==========================================
// UI Component & Workspace Projections
// ==========================================

export interface AgentSessionCapabilities {
  send: boolean;
  stop: boolean;
  retry: boolean;
  queue: boolean;
  interrupt: boolean;
  attach: boolean;
  mention: boolean;
  slashCommand: boolean;
  modelSelect: boolean;
  permissionResponse: boolean;
  copy: boolean;
  openArtifact: boolean;
}

export const DEFAULT_OBSERVER_CAPABILITIES: AgentSessionCapabilities = {
  send: false,
  stop: false,
  retry: false,
  queue: false,
  interrupt: false,
  attach: false,
  mention: false,
  slashCommand: false,
  modelSelect: false,
  permissionResponse: false,
  copy: true,
  openArtifact: true,
};

export const DEFAULT_INTERACTIVE_CAPABILITIES: AgentSessionCapabilities = {
  send: true,
  stop: true,
  retry: true,
  queue: false,
  interrupt: false,
  attach: false,
  mention: false,
  slashCommand: false,
  modelSelect: false,
  permissionResponse: false,
  copy: true,
  openArtifact: true,
};

export type AgentSessionItemKind =
  | "user_message"
  | "assistant_text"
  | "processing"
  | "thinking"
  | "tool"
  | "task"
  | "notice"
  | "final_result"
  | "cancelled"
  | "error";

export type AgentSessionItemState =
  "pending" | "streaming" | "completed" | "succeeded" | "failed" | "cancelled";

export interface TruncationInfoView {
  originalBytes: number;
  retainedBytes: number;
  strategy: string;
}

export interface AgentSessionItemView {
  id: string;
  kind: AgentSessionItemKind;
  sequence: number;
  delivery: "replay" | "live";
  state: AgentSessionItemState;
  text?: string | null;
  status?: string | null;
  code?: string | null;
  partial?: boolean;
  truncation?: TruncationInfoView | null;
  turnId?: string | null;
  toolCallId?: string | null;
  toolName?: string | null;
  toolInput?: unknown;
  toolOutput?: unknown;
}

export type ToolContentBlock =
  | { type: "text"; text: string; language: string | null }
  | { type: "json"; value: unknown; formatted: string }
  | { type: "command"; command: string; cwd: string | null }
  | { type: "terminal"; stdout: string; stderr: string; ansiStripped: true }
  | {
      type: "diff";
      path: string;
      oldText: string | null;
      newText: string | null;
      unifiedDiff: string | null;
      isTruncated?: boolean;
    }
  | {
      type: "location";
      path: string;
      line: number | null;
      column: number | null;
    }
  | { type: "image"; path: string; mimeType: string | null; alt: string | null }
  | {
      type: "artifact";
      artifactId: string;
      renderer: string;
      title: string | null;
    }
  | { type: "unknown"; providerType: string; display: string };

export type AgentSessionStepGroupStatus =
  "pending" | "running" | "succeeded" | "failed" | "cancelled";

export type AgentSessionContentBlock =
  | { type: "user_request"; item: AgentSessionItemView }
  | { type: "assistant_text"; item: AgentSessionItemView }
  | { type: "thinking"; item: AgentSessionItemView }
  | { type: "processing"; item: AgentSessionItemView }
  | {
      type: "step_group";
      groupId: string;
      items: AgentSessionItemView[];
      logicalCount: number;
      status: AgentSessionStepGroupStatus;
    }
  | { type: "task"; item: AgentSessionItemView }
  | { type: "notice"; item: AgentSessionItemView }
  | { type: "terminal"; item: AgentSessionItemView }
  | { type: "error"; item: AgentSessionItemView };

export interface AgentSessionTurnView {
  turnId: string;
  blocks: AgentSessionContentBlock[];
}

export interface AgentSessionStatus {
  className: string;
  icon?: ReactNode;
  label: string;
}

export interface AgentSessionRestoreStateInfo {
  state: string;
  label: string;
  className?: string;
  icon?: ReactNode;
  errorCode?: string | null;
}

export interface AgentSessionWorkspaceProps {
  capabilities: AgentSessionCapabilities;
  items: AgentSessionItemView[];
  recipientTitle?: ReactNode;
  model?: string | null;
  status?: AgentSessionStatus;
  restoreState?: AgentSessionRestoreStateInfo | null;
  draft?: string;
  disabled?: boolean;
  canSend?: boolean;
  isExecuting?: boolean;
  placeholder?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  emptyIcon?: ReactNode;
  submitLabel?: string;
  stopLabel?: string;
  onSend?: (message: string) => void | Promise<void>;
  onStop?: () => void | Promise<void>;
  onInterrupt?: () => void | Promise<void>;
  onQueue?: (message: string) => void | Promise<void>;
  onDraftChange?: (draft: string) => void;
  headerActions?: ReactNode;
  composerExtra?: ReactNode;
  timelineExtra?: ReactNode;
  activityDependencyKey?: string;
  sessionResetKey?: string;
  testIdPrefix?: string;
  className?: string;
  unavailable?: boolean;
  unavailableDescription?: string;
  isReadOnly?: boolean;
}
