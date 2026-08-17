import { Bot, Code2, Cpu, Terminal, Zap, type LucideIcon } from "lucide-react";
import type { AgentInstallationView, AgentMarketItem } from "../../services/agentRuntime";

/** Runtime Agent IDs come from the curated Market catalog, not a frontend union. */
export type AgentId = string;

export type AgentFilter = "all" | "available" | "unavailable";
export type AgentConnectionState =
  | "checking"
  | "available"
  | "failed"
  | "not-installed"
  | "not-tested";
export type AgentConnectionMode = "registry" | "legacy";

export interface AgentCatalogItem {
  id: AgentId;
  name: string;
  command: string;
  protocol: string;
  description: string;
  icon: LucideIcon;
  connectionMode: AgentConnectionMode;
  installed?: AgentInstallationView | null;
  marketVersion?: string;
  updateAvailable?: boolean;
}

const agentPresentationMetadata: Record<string, { name: string; icon: LucideIcon }> = {
  opencode: { name: "OpenCode", icon: Bot },
  gemini: { name: "Gemini CLI", icon: Zap },
  kiro: { name: "Kiro", icon: Code2 },
  antigravity: { name: "Antigravity", icon: Zap },
  claude: { name: "Claude Code", icon: Cpu },
  codex: { name: "Codex CLI", icon: Terminal },
  hermes: { name: "Hermes", icon: Bot },
  pi: { name: "Pi", icon: Code2 },
  qoder: { name: "Qoder", icon: Cpu },
};

const legacyPresentationItems: Array<[string, string]> = [
  ["opencode", "ACP Agent"],
  ["gemini", "ACP Agent"],
  ["kiro", "ACP Agent"],
  ["antigravity", "Native Agent"],
  ["claude", "ACP Agent"],
  ["codex", "ACP Agent"],
  ["hermes", "ACP Agent"],
  ["pi", "ACP Agent"],
  ["qoder", "ACP Agent"],
];

/**
 * Compatibility fallback for older desktop builds/browser previews. Runtime
 * commands, package names and distribution details must come from Market DTOs.
 */
export const agentCatalog: AgentCatalogItem[] = legacyPresentationItems.map(([id, protocol]) => ({
  id,
  name: agentPresentationMetadata[id].name,
  command: "managed runtime",
  protocol,
  description: "Agent Market runtime metadata is unavailable in this build.",
  icon: agentPresentationMetadata[id].icon,
  connectionMode: "registry",
}));

/*
  Presentation-only metadata intentionally does not include executable
  commands, package names, versions, or distribution choices.
*/
export const initialConnectionStates: Record<AgentId, AgentConnectionState> = {
  opencode: "checking",
  gemini: "not-tested",
  kiro: "checking",
  antigravity: "checking",
  claude: "checking",
  codex: "checking",
  hermes: "checking",
  pi: "checking",
  qoder: "checking",
};

export const registryAgentIds = agentCatalog
  .filter((agent) => agent.connectionMode === "registry")
  .map((agent) => agent.id);

export function marketItemToCatalogItem(item: AgentMarketItem): AgentCatalogItem {
  const known = agentPresentationMetadata[item.id];
  const recommended = item.distributions.find((distribution) => distribution.recommended);
  return {
    id: item.id,
    name: item.displayName || known?.name || item.id,
    command: item.installed?.displayInstallPath
      || recommended?.targetPath
      || recommended?.distributionId
      || item.id,
    protocol: item.protocol.toUpperCase(),
    description: item.description,
    icon: known?.icon ?? Bot,
    connectionMode: "registry",
    installed: item.installed,
    marketVersion: item.version,
    updateAvailable: item.updateAvailable,
  };
}
