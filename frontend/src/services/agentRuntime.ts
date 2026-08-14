import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./appUpdater";

export type AgentConnectionCheckMode = "installation" | "connection";

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
  connection_method: "acp" | "cli_version" | "cli_fallback" | null;
  error_code: string | null;
  error: string | null;
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
