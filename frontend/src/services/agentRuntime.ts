import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "./appUpdater";

export type AgentConnectionCheckMode = "installation" | "connection";

const AGENT_LIFECYCLE_TASK_UPDATED_EVENT = "agent-market://lifecycle-task-updated";

export interface AgentRuntimeCatalogEntry {
  id: string;
  display_name: string;
  command: string;
  args: string[];
  availability_command: string;
  protocol: "acp" | "native";
}

export interface AgentConnectionResult {
  agent_id: string;
  available: boolean;
  installed: boolean;
  connected: boolean;
  version: string | null;
  connection_method: "acp" | "cli_version" | "native" | null;
  error_code: string | null;
  error: string | null;
  installation_status?: string | null;
  runtime_status?: string | null;
  protocol_status?: string | null;
  execution_ready?: boolean;
  health_stale?: boolean;
}

export interface AgentModelOption {
  id: string;
  label: string;
  description: string | null;
}

export interface AgentModelsResult {
  agent_id: string;
  available: boolean;
  models: AgentModelOption[];
  current_model_id: string | null;
  error_code: string | null;
  error: string | null;
}

export type AgentMarketProtocol = "acp" | "native";
export type AgentDistributionType = "system" | "binary" | "npx" | "uvx";
export type AgentOwnership = "system" | "managed";

export interface AgentDistributionCandidate {
  distributionId: string;
  distributionType: AgentDistributionType;
  selectable: boolean;
  recommended: boolean;
  ownership: AgentOwnership;
  reasonCode: string | null;
  requiredRuntime: string | null;
  resolvedVersion: string | null;
  downloadSize: number | null;
  targetPath: string | null;
}

export interface AgentInstallationView {
  agentId: string;
  displayName: string;
  version: string;
  protocol: AgentMarketProtocol;
  distributionId: string;
  distributionType: AgentDistributionType;
  ownership: AgentOwnership;
  displayInstallPath: string | null;
  enabled: boolean;
  installed: boolean;
  installationStatus: string;
  runtimeStatus: string;
  protocolStatus: string;
  connected: boolean;
  executionReady: boolean;
  healthStale: boolean;
  selectedModelId: string | null;
  modelStatus: string | null;
  updateAvailable: boolean;
  operation: string | null;
  lastCheckedAt: string | null;
  error: AgentMarketErrorView | null;
  warnings: string[];
}

export interface AgentMarketErrorView {
  code: string;
  message: string;
  agentId: string | null;
  phase: string | null;
  retryable: boolean;
  action: string | null;
}

export interface AgentMarketItem {
  id: string;
  catalogVersion: string;
  displayName: string;
  description: string;
  protocol: AgentMarketProtocol;
  version: string;
  coreCompatible: boolean;
  capabilities: {
    purposes: string[];
    textPrompt: boolean;
    modelDiscovery: boolean;
  };
  verification: {
    status: "tested" | "experimental";
    testedAt: string;
    evidenceId: string | null;
  };
  distributions: AgentDistributionCandidate[];
  recommendedDistributionId: string | null;
  installed: AgentInstallationView | null;
  updateAvailable: boolean;
}

export interface AgentMarketRefreshResult {
  status: "updated" | "not_modified";
  catalogVersion: string;
  itemCount: number;
  source: string;
  etag: string | null;
}

export type AgentMarketRefreshTaskState = "running" | "succeeded" | "failed";

export interface AgentMarketRefreshTaskSnapshot {
  id: string;
  state: AgentMarketRefreshTaskState;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
  result: AgentMarketRefreshResult | null;
  error: string | null;
}

export interface AgentInstallPreview {
  agentId: string;
  catalogVersion: string;
  action: string;
  selectedDistribution: AgentDistributionCandidate;
  alternatives: AgentDistributionCandidate[];
  currentInstallation: AgentInstallationView | null;
  targetVersion: string;
  ownership: AgentOwnership;
  targetPath: string | null;
  downloadSize: number | null;
  runtimeRequirements: string[];
  conflicts: string[];
  warnings: string[];
  confirmationRequired: boolean;
  previewToken: string;
}

export interface AgentUninstallPreview {
  agentId: string;
  currentInstallation: AgentInstallationView;
  ownership: AgentOwnership;
  targetPath: string | null;
  capabilityAssignments: string[];
  conflicts: string[];
  warnings: string[];
  confirmationRequired: boolean;
  previewToken: string;
}

export type AgentLifecycleTaskState = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type AgentLifecycleTaskPhase =
  | "queued" | "preparing" | "probing_runtime" | "downloading" | "installing"
  | "validating_integrity" | "validating_layout" | "probing_protocol"
  | "activating_database" | "reloading_registry" | "cleaning_up"
  | "succeeded" | "failed" | "cancelled";

export interface AgentLifecycleTaskSnapshot {
  id: string;
  agentId: string;
  action: string;
  state: AgentLifecycleTaskState;
  phase: AgentLifecycleTaskPhase;
  catalogVersion: string | null;
  agentVersion: string | null;
  distributionId: string | null;
  distributionType: AgentDistributionType | null;
  ownership: AgentOwnership | null;
  progress: {
    completedUnits: number;
    totalUnits: number | null;
    downloadedBytes: number | null;
    totalBytes: number | null;
  };
  cancellable: boolean;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
  result: AgentInstallationView | null;
  error: AgentMarketErrorView | null;
  warnings: string[];
}

export interface AgentMarketListRequest {
  query?: string;
  protocol?: AgentMarketProtocol;
  installedOnly?: boolean;
  includeIncompatible?: boolean;
}

export interface AgentInstallPreviewRequest {
  agentId: string;
  catalogVersion?: string;
  agentVersion?: string;
  distributionId?: string;
  action: "install" | "update" | "reinstall";
}

export interface AgentInstallStartRequest {
  agentId: string;
  catalogVersion: string;
  agentVersion: string;
  distributionId: string;
  previewToken: string;
}

export interface AgentUninstallStartRequest {
  agentId: string;
  clearCapabilityAssignments?: string[];
  previewToken: string;
}

export async function listAgentCatalog(): Promise<AgentRuntimeCatalogEntry[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  return invoke<AgentRuntimeCatalogEntry[]>("list_agent_catalog");
}

export async function checkAgentConnection(
  agentId: string,
  mode: AgentConnectionCheckMode,
): Promise<AgentConnectionResult> {
  if (!isTauriRuntime()) {
    return {
      agent_id: agentId,
      available: false,
      installed: false,
      connected: false,
      version: null,
      connection_method: null,
      error_code: "desktop_runtime_required",
      error: "Agent connection checks require the desktop app runtime.",
    };
  }

  return invoke<AgentConnectionResult>("check_agent_connection", {
    params: { agent_id: agentId, mode },
  });
}

export async function listAgentModels(agentId: string): Promise<AgentModelsResult> {
  if (!isTauriRuntime()) {
    return {
      agent_id: agentId,
      available: false,
      models: [],
      current_model_id: null,
      error_code: "desktop_runtime_required",
      error: "Agent model discovery requires the desktop app runtime.",
    };
  }

  return invoke<AgentModelsResult>("list_agent_models", {
    params: { agent_id: agentId },
  });
}

export async function listAgentMarket(request: AgentMarketListRequest = {}): Promise<AgentMarketItem[]> {
  if (!isTauriRuntime()) return [];
  return invoke<AgentMarketItem[]>("list_agent_market", { params: request });
}

export async function inspectAgentMarketItem(agentId: string): Promise<AgentMarketItem> {
  if (!isTauriRuntime()) throw new Error("Agent Market requires the desktop app runtime.");
  return invoke<AgentMarketItem>("inspect_agent_market_item", { agentId });
}

export async function refreshAgentMarket(): Promise<AgentMarketRefreshTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent Market requires the desktop app runtime.");
  return invoke<AgentMarketRefreshTaskSnapshot>("refresh_agent_market");
}

export async function getAgentMarketRefreshTask(taskId: string): Promise<AgentMarketRefreshTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent Market requires the desktop app runtime.");
  return invoke<AgentMarketRefreshTaskSnapshot>("get_agent_market_refresh_task", { taskId });
}

export async function listAgentMarketRefreshTasks(): Promise<AgentMarketRefreshTaskSnapshot[]> {
  if (!isTauriRuntime()) return [];
  return invoke<AgentMarketRefreshTaskSnapshot[]>("list_agent_market_refresh_tasks");
}

export async function previewAgentInstallation(request: AgentInstallPreviewRequest): Promise<AgentInstallPreview> {
  if (!isTauriRuntime()) throw new Error("Agent installation requires the desktop app runtime.");
  return invoke<AgentInstallPreview>("preview_agent_installation", { params: request });
}

export async function listInstalledAgents(): Promise<AgentInstallationView[]> {
  if (!isTauriRuntime()) return [];
  return invoke<AgentInstallationView[]>("list_installed_agents");
}

export async function getInstalledAgent(agentId: string): Promise<AgentInstallationView> {
  if (!isTauriRuntime()) throw new Error("Agent management requires the desktop app runtime.");
  return invoke<AgentInstallationView>("get_installed_agent", { agentId });
}

export async function checkAgentRuntime(agentId: string): Promise<AgentInstallationView> {
  if (!isTauriRuntime()) throw new Error("Agent runtime checks require the desktop app runtime.");
  return invoke<AgentInstallationView>("check_agent_runtime", { agentId });
}

export async function previewAgentUninstall(agentId: string): Promise<AgentUninstallPreview> {
  if (!isTauriRuntime()) throw new Error("Agent uninstall requires the desktop app runtime.");
  return invoke<AgentUninstallPreview>("preview_agent_uninstall", { agentId });
}

export async function startAgentInstallation(request: AgentInstallStartRequest): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent installation requires the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("start_agent_installation", { params: request });
}

export async function startAgentUpdate(request: AgentInstallStartRequest): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent update requires the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("start_agent_update", { params: request });
}

export async function startAgentReinstallation(request: AgentInstallStartRequest): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent reinstallation requires the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("start_agent_reinstallation", { params: request });
}

export async function startAgentUninstall(request: AgentUninstallStartRequest): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent uninstall requires the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("start_agent_uninstall", { params: request });
}

export async function getAgentLifecycleTask(taskId: string): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent lifecycle tasks require the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("get_agent_lifecycle_task", { taskId });
}

export async function listAgentLifecycleTasks(): Promise<AgentLifecycleTaskSnapshot[]> {
  if (!isTauriRuntime()) return [];
  return invoke<AgentLifecycleTaskSnapshot[]>("list_agent_lifecycle_tasks");
}

export function subscribeAgentLifecycleTasks(listener: (snapshot: AgentLifecycleTaskSnapshot) => void) {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<AgentLifecycleTaskSnapshot>(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, (event) => {
    listener(event.payload);
  });
}

export async function cancelAgentLifecycleTask(taskId: string): Promise<AgentLifecycleTaskSnapshot> {
  if (!isTauriRuntime()) throw new Error("Agent lifecycle tasks require the desktop app runtime.");
  return invoke<AgentLifecycleTaskSnapshot>("cancel_agent_lifecycle_task", { taskId });
}

export async function enableAgent(agentId: string): Promise<AgentInstallationView> {
  if (!isTauriRuntime()) throw new Error("Agent management requires the desktop app runtime.");
  return invoke<AgentInstallationView>("enable_agent", { agentId });
}

export async function disableAgent(agentId: string): Promise<AgentInstallationView> {
  if (!isTauriRuntime()) throw new Error("Agent management requires the desktop app runtime.");
  return invoke<AgentInstallationView>("disable_agent", { agentId });
}
