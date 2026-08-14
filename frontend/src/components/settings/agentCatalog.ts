import { Bot, Code2, Cpu, Terminal, Zap, type LucideIcon } from "lucide-react";

export type AgentId =
  | "opencode"
  | "gemini"
  | "kiro"
  | "antigravity"
  | "claude"
  | "codex"
  | "hermes"
  | "pi"
  | "qoder";

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
  iconTone: "primary" | "update" | "create" | "neutral";
  connectionMode: AgentConnectionMode;
}

export const agentCatalog: AgentCatalogItem[] = [
  {
    id: "opencode",
    name: "OpenCode",
    command: "opencode acp",
    protocol: "ACP over stdio + CLI fallback",
    description: "AssetIWeave 内置的 ACP Agent，连接检测保留 CLI 兜底。",
    icon: Bot,
    iconTone: "primary",
    connectionMode: "registry",
  },
  {
    id: "gemini",
    name: "Gemini CLI",
    command: "gemini --acp",
    protocol: "ACP over stdio",
    description: "使用 Gemini CLI 的 ACP 入口加载会话和模型配置。",
    icon: Zap,
    iconTone: "update",
    connectionMode: "registry",
  },
  {
    id: "kiro",
    name: "Kiro",
    command: "kiro-cli-chat acp",
    protocol: "ACP over stdio",
    description: "使用 Kiro CLI 的 ACP 入口检测安装和会话初始化。",
    icon: Code2,
    iconTone: "create",
    connectionMode: "registry",
  },
  {
    id: "antigravity",
    name: "Antigravity",
    command: "agy",
    protocol: "Direct CLI (stream-json)",
    description: "通过 agy CLI 原生流式协议执行卡片翻译与模型发现。",
    icon: Zap,
    iconTone: "update",
    connectionMode: "registry",
  },
  {
    id: "claude",
    name: "Claude Code",
    command: "npx -y @agentclientprotocol/claude-agent-acp@0.58.1",
    protocol: "ACP over stdio",
    description: "使用 Claude Code 官方 ACP bridge 执行初始化和 session/new 检测。",
    icon: Cpu,
    iconTone: "create",
    connectionMode: "registry",
  },
  {
    id: "codex",
    name: "Codex CLI",
    command: "npx -y @agentclientprotocol/codex-acp@1.1.2",
    protocol: "ACP over stdio",
    description: "使用 Codex CLI ACP bridge 执行初始化和 session/new 检测。",
    icon: Terminal,
    iconTone: "primary",
    connectionMode: "registry",
  },
  {
    id: "hermes",
    name: "Hermes",
    command: "hermes acp",
    protocol: "ACP over stdio",
    description: "通过 Hermes 的原生 ACP 子命令检测连接。",
    icon: Bot,
    iconTone: "neutral",
    connectionMode: "registry",
  },
  {
    id: "pi",
    name: "Pi",
    command: "npx -y pi-acp@0.0.33",
    protocol: "ACP over stdio",
    description: "沿用 AionUI 的 pi-acp bridge，并先检测 pi CLI。",
    icon: Code2,
    iconTone: "update",
    connectionMode: "registry",
  },
  {
    id: "qoder",
    name: "Qoder",
    command: "qodercli --acp",
    protocol: "ACP over stdio",
    description: "使用 qodercli 的 ACP 子命令检测安装和连接。",
    icon: Cpu,
    iconTone: "neutral",
    connectionMode: "registry",
  },
];

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
