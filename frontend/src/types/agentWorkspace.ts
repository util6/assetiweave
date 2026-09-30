import type { AgentSessionItemView } from "./agentSession";

export type AgentProvider = "codex" | "antigravity" | "claude" | "generic";

export type AccountAuthState = "active" | "expired" | "rate_limited";

export interface AccountQuotaSummary {
  used?: string;
  total?: string;
  percentage?: number;
  resetTime?: string;
}

export interface AgentAccount {
  id: string;
  provider: AgentProvider;
  displayName: string;
  email?: string;
  authType: "oauth" | "api_key" | "token";
  state: AccountAuthState;
  quotaSummary?: AccountQuotaSummary;
  createdAt: string;
  lastUsedAt?: string;
}

export interface AgentModelDef {
  id: string;
  label: string;
  description?: string;
}

export interface AgentWorkspaceDefinition {
  id: string;
  displayName: string;
  provider: AgentProvider;
  command: string;
  description: string;
  availableModels: AgentModelDef[];
  defaultModel: string;
}

export type TeamMemberRole = "leader" | "teammate";

export interface TeamMemberConfig {
  memberId: string;
  name: string;
  role: TeamMemberRole;
  agentId: string;
  model: string;
  accountId: string; // The crucial Cockpit-style account binding
}

export interface TeamDefinition {
  id: string;
  name: string;
  description?: string;
  leaderMemberId: string;
  members: TeamMemberConfig[];
  createdAt: string;
  updatedAt: string;
}

export type TeamViewMode = "parallel" | "single";

export interface TeamLaneSession {
  memberId: string;
  sessionRef: string;
  items: AgentSessionItemView[];
  isExecuting: boolean;
  draft: string;
  unreadCount?: number;
}
