import type { ReactNode } from "react";

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
  | "pending"
  | "streaming"
  | "completed"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface AgentSessionItemView {
  id: string;
  kind: AgentSessionItemKind;
  sequence: number;
  delivery: "replay" | "live";
  state: AgentSessionItemState;
  text?: string | null;
  status?: string | null;
  code?: string | null;
  toolCallId?: string | null;
  toolName?: string | null;
  toolInput?: unknown;
  toolOutput?: unknown;
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
  status?: AgentSessionStatus;
  restoreState?: AgentSessionRestoreStateInfo | null;
  draft?: string;
  disabled?: boolean;
  canSend?: boolean;
  placeholder?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  emptyIcon?: ReactNode;
  submitLabel?: string;
  onSend?: (message: string) => void | Promise<void>;
  onDraftChange?: (draft: string) => void;
  headerActions?: ReactNode;
  composerExtra?: ReactNode;
  timelineExtra?: ReactNode;
  activityDependencyKey?: string;
  sessionResetKey?: string;
  testIdPrefix?: string;
  className?: string;
}
