import type { AgentSessionItemView } from "../types/agentSession";
import type {
  AgentWorkspaceDefinition,
  TeamDefinition,
} from "../types/agentWorkspace";

const TEAM_STORAGE_KEY = "assetiweave_agent_teams_v1";

export const SUPPORTED_AGENTS: AgentWorkspaceDefinition[] = [
  {
    id: "codex",
    displayName: "Codex (CLI / ACP)",
    provider: "codex",
    command: "codex",
    description: "强大的代码编写与项目级重构智能体，支持深层上下文与工具调用。",
    availableModels: [
      {
        id: "o3-mini",
        label: "o3-mini (推荐)",
        description: "超快推理，深度代码理解",
      },
      { id: "o1", label: "o1", description: "最高智商复杂算法推理" },
      { id: "gpt-4.5", label: "GPT-4.5", description: "大上下文综合开发" },
      { id: "gpt-4o", label: "GPT-4o", description: "高性价比日常编码" },
    ],
    defaultModel: "o3-mini",
  },
  {
    id: "antigravity",
    displayName: "Antigravity (AGY / ACP)",
    provider: "antigravity",
    command: "agy",
    description: "Google Antigravity 官方自主智能体，擅长工具编排与极速生成。",
    availableModels: [
      {
        id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro (推荐)",
        description: "超长 2M 上下文，卓越多模态与代码能力",
      },
      {
        id: "gemini-2.5-flash",
        label: "Gemini 2.5 Flash",
        description: "极致毫秒级响应，轻量任务首选",
      },
    ],
    defaultModel: "gemini-2.5-pro",
  },
  {
    id: "claude",
    displayName: "Claude Code",
    provider: "claude",
    command: "claude",
    description: "Anthropic 原生终端 Agent，严谨规范与高质量代码修改。",
    availableModels: [
      {
        id: "claude-3-7-sonnet",
        label: "Claude 3.7 Sonnet (Hybrid)",
        description: "混合思维，超精细代码定位",
      },
      {
        id: "claude-3-5-sonnet",
        label: "Claude 3.5 Sonnet",
        description: "业界代码基准黄金标准",
      },
      {
        id: "claude-3-5-haiku",
        label: "Claude 3.5 Haiku",
        description: "极速响应日常检索",
      },
    ],
    defaultModel: "claude-3-7-sonnet",
  },
];

const INITIAL_TEAMS: TeamDefinition[] = [
  {
    id: "team_fullstack",
    name: "全栈架构重构小队",
    description:
      "由架构师、前端专家与测试专家组成的协同开发组，每个成员分配了独立绑定的账号以保障配额充足。",
    leaderMemberId: "m_architect",
    members: [
      {
        memberId: "m_architect",
        name: "系统架构师",
        role: "leader",
        agentId: "codex",
        model: "o3-mini",
        accountId: "acc_codex_work",
      },
      {
        memberId: "m_frontend",
        name: "前端界面专家",
        role: "teammate",
        agentId: "antigravity",
        model: "gemini-2.5-pro",
        accountId: "acc_agy_pro",
      },
      {
        memberId: "m_qa",
        name: "测试与代码审查员",
        role: "teammate",
        agentId: "codex",
        model: "gpt-4.5",
        accountId: "acc_codex_personal",
      },
    ],
    createdAt: "2026-09-18T10:00:00Z",
    updatedAt: "2026-09-20T18:00:00Z",
  },
  {
    id: "team_devops",
    name: "自动化运维与巡检团队",
    description: "多智能体自动监控系统状态、检查日志并生成运维报告。",
    leaderMemberId: "m_sre",
    members: [
      {
        memberId: "m_sre",
        name: "SRE 调度总管",
        role: "leader",
        agentId: "codex",
        model: "o3-mini",
        accountId: "acc_codex_personal",
      },
      {
        memberId: "m_cloud",
        name: "云原生巡检员",
        role: "teammate",
        agentId: "antigravity",
        model: "gemini-2.5-flash",
        accountId: "acc_agy_spare",
      },
    ],
    createdAt: "2026-09-19T14:30:00Z",
    updatedAt: "2026-09-20T19:00:00Z",
  },
];

export function loadStoredTeams(): TeamDefinition[] {
  if (typeof window === "undefined") return INITIAL_TEAMS;
  try {
    const raw = localStorage.getItem(TEAM_STORAGE_KEY);
    if (!raw) {
      localStorage.setItem(TEAM_STORAGE_KEY, JSON.stringify(INITIAL_TEAMS));
      return INITIAL_TEAMS;
    }
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.length > 0) {
      return parsed;
    }
    return INITIAL_TEAMS;
  } catch {
    return INITIAL_TEAMS;
  }
}

export function saveStoredTeams(teams: TeamDefinition[]): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(TEAM_STORAGE_KEY, JSON.stringify(teams));
  } catch (e) {
    console.error("Failed to save teams:", e);
  }
}

export async function listTeams(): Promise<TeamDefinition[]> {
  return loadStoredTeams();
}

export async function getTeamById(id: string): Promise<TeamDefinition | null> {
  const teams = loadStoredTeams();
  return teams.find((t) => t.id === id) ?? null;
}

export async function createTeam(
  team: Omit<TeamDefinition, "id" | "createdAt" | "updatedAt">,
): Promise<TeamDefinition> {
  const teams = loadStoredTeams();
  const now = new Date().toISOString();
  const newTeam: TeamDefinition = {
    ...team,
    id: `team_${Date.now()}`,
    createdAt: now,
    updatedAt: now,
  };
  teams.unshift(newTeam);
  saveStoredTeams(teams);
  return newTeam;
}

export async function updateTeam(
  id: string,
  updates: Partial<Omit<TeamDefinition, "id" | "createdAt">>,
): Promise<TeamDefinition | null> {
  const teams = loadStoredTeams();
  const index = teams.findIndex((t) => t.id === id);
  if (index === -1) return null;
  const updated: TeamDefinition = {
    ...teams[index],
    ...updates,
    updatedAt: new Date().toISOString(),
  };
  teams[index] = updated;
  saveStoredTeams(teams);
  return updated;
}

export async function deleteTeam(id: string): Promise<boolean> {
  const teams = loadStoredTeams();
  const filtered = teams.filter((t) => t.id !== id);
  if (filtered.length === teams.length) return false;
  saveStoredTeams(filtered);
  return true;
}

// ==========================================
// Mock Live Streaming Simulation
// ==========================================

export interface TurnUpdateCallback {
  (items: AgentSessionItemView[], isFinished: boolean): void;
}

export function simulateAgentStream(
  prompt: string,
  agentDisplayName: string,
  accountDisplayName: string,
  onUpdate: TurnUpdateCallback,
): () => void {
  let isCancelled = false;
  const turnId = `turn_${Date.now()}`;
  const timestamp = Date.now();

  const userItem: AgentSessionItemView = {
    id: `msg_user_${timestamp}`,
    kind: "user_message",
    sequence: 1,
    delivery: "live",
    state: "completed",
    text: prompt,
    turnId,
  };

  const thinkingItem: AgentSessionItemView = {
    id: `msg_think_${timestamp}`,
    kind: "thinking",
    sequence: 2,
    delivery: "live",
    state: "running",
    text: `正在以账号 [${accountDisplayName}] 调度 ${agentDisplayName} 分析任务需求...`,
    turnId,
  };

  const currentItems: AgentSessionItemView[] = [userItem, thinkingItem];
  onUpdate([...currentItems], false);

  const timer1 = setTimeout(() => {
    if (isCancelled) return;
    thinkingItem.state = "completed";
    thinkingItem.text += `\n分析完成：已识别项目上下文与操作边界。准备执行工具调用读取现有工程依赖。`;

    const toolItem: AgentSessionItemView = {
      id: `msg_tool_${timestamp}`,
      kind: "tool",
      sequence: 3,
      delivery: "live",
      state: "running",
      toolName: "read_project_manifest",
      toolInput: { path: "Cargo.toml", account: accountDisplayName },
      status: "正在读取系统依赖与配置清单...",
      turnId,
    };
    currentItems.push(toolItem);
    onUpdate([...currentItems], false);

    const timer2 = setTimeout(() => {
      if (isCancelled) return;
      toolItem.state = "completed";
      toolItem.status = "已读取 18 行依赖配置，环境检查正常。";
      toolItem.toolOutput = {
        status: "success",
        parsed_features: ["acp", "multi_account"],
      };

      const assistantItem: AgentSessionItemView = {
        id: `msg_asst_${timestamp}`,
        kind: "assistant_text",
        sequence: 4,
        delivery: "live",
        state: "streaming",
        text: `我已经使用 **${agentDisplayName}**（绑定账号：\`${accountDisplayName}\`）完成了环境检测与任务评估。\n\n针对您的指令：\n> ${prompt}\n\n建议按以下步骤实施：\n1. **沙箱上下文装配**：确保当前工作区的环境变量正确注入；\n2. **成员协同调度**：若在团队模式中，可将此拆解为多个子任务指派给对应成员；\n3. **结果验证与回滚**：实时监控执行步骤与退出状态码。`,
        turnId,
      };
      currentItems.push(assistantItem);
      onUpdate([...currentItems], false);

      const timer3 = setTimeout(() => {
        if (isCancelled) return;
        assistantItem.state = "completed";
        onUpdate([...currentItems], true);
      }, 500);

      timers.push(timer3);
    }, 700);

    timers.push(timer2);
  }, 600);

  const timers = [timer1];

  return () => {
    isCancelled = true;
    timers.forEach(clearTimeout);
  };
}
