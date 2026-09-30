import type { AgentAccount, AgentProvider } from "../types/agentWorkspace";

const STORAGE_KEY = "assetiweave_agent_accounts_v1";

const INITIAL_MOCK_ACCOUNTS: AgentAccount[] = [
  {
    id: "acc_codex_work",
    provider: "codex",
    displayName: "Codex 工作主号",
    email: "dev@company.com",
    authType: "oauth",
    state: "active",
    quotaSummary: {
      used: "68,000 tokens",
      total: "100,000 tokens",
      percentage: 68,
      resetTime: "明日 08:00",
    },
    createdAt: "2026-09-01T10:00:00Z",
    lastUsedAt: "2026-09-20T22:30:00Z",
  },
  {
    id: "acc_codex_personal",
    provider: "codex",
    displayName: "Codex 个人备用号",
    email: "util6@personal.me",
    authType: "oauth",
    state: "active",
    quotaSummary: {
      used: "25,000 tokens",
      total: "100,000 tokens",
      percentage: 25,
      resetTime: "明日 08:00",
    },
    createdAt: "2026-09-05T14:00:00Z",
    lastUsedAt: "2026-09-18T16:00:00Z",
  },
  {
    id: "acc_agy_pro",
    provider: "antigravity",
    displayName: "Antigravity 团队版",
    email: "util6@ai.org",
    authType: "oauth",
    state: "active",
    quotaSummary: {
      used: "420 requests",
      total: "1,000 requests",
      percentage: 42,
      resetTime: "本周五",
    },
    createdAt: "2026-09-02T11:00:00Z",
    lastUsedAt: "2026-09-20T21:15:00Z",
  },
  {
    id: "acc_agy_spare",
    provider: "antigravity",
    displayName: "Antigravity 个人号",
    email: "util6@gmail.com",
    authType: "token",
    state: "active",
    quotaSummary: {
      used: "12 requests",
      total: "200 requests",
      percentage: 6,
      resetTime: "下月 1 日",
    },
    createdAt: "2026-09-10T09:00:00Z",
    lastUsedAt: "2026-09-15T12:00:00Z",
  },
  {
    id: "acc_claude_max",
    provider: "claude",
    displayName: "Claude Code 旗舰号",
    email: "corp@anthropic.ai",
    authType: "oauth",
    state: "active",
    quotaSummary: {
      used: "550 requests",
      total: "1,000 requests",
      percentage: 55,
      resetTime: "明日 12:00",
    },
    createdAt: "2026-09-12T08:00:00Z",
    lastUsedAt: "2026-09-19T18:00:00Z",
  },
];

type AccountsListener = (accounts: AgentAccount[]) => void;
const listeners = new Set<AccountsListener>();

function notifyListeners(accounts: AgentAccount[]) {
  listeners.forEach((listener) => {
    try {
      listener(accounts);
    } catch (e) {
      console.error("Account listener error:", e);
    }
  });
}

export function loadStoredAccounts(): AgentAccount[] {
  if (typeof window === "undefined") return INITIAL_MOCK_ACCOUNTS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(INITIAL_MOCK_ACCOUNTS));
      return INITIAL_MOCK_ACCOUNTS;
    }
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed) && parsed.length > 0) {
      return parsed;
    }
    return INITIAL_MOCK_ACCOUNTS;
  } catch {
    return INITIAL_MOCK_ACCOUNTS;
  }
}

export function saveStoredAccounts(accounts: AgentAccount[]): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(accounts));
    notifyListeners(accounts);
  } catch (e) {
    console.error("Failed to save accounts:", e);
  }
}

export async function listAgentAccounts(
  provider?: AgentProvider,
): Promise<AgentAccount[]> {
  const accounts = loadStoredAccounts();
  if (provider) {
    return accounts.filter((a) => a.provider === provider);
  }
  return accounts;
}

export async function getAgentAccountById(
  id: string,
): Promise<AgentAccount | null> {
  const accounts = loadStoredAccounts();
  return accounts.find((a) => a.id === id) ?? null;
}

export async function createAgentAccount(
  account: Omit<AgentAccount, "id" | "createdAt">,
): Promise<AgentAccount> {
  const accounts = loadStoredAccounts();
  const newAccount: AgentAccount = {
    ...account,
    id: `acc_${account.provider}_${Date.now()}`,
    createdAt: new Date().toISOString(),
    lastUsedAt: new Date().toISOString(),
  };
  const updated = [...accounts, newAccount];
  saveStoredAccounts(updated);
  return newAccount;
}

export async function updateAgentAccount(
  id: string,
  updates: Partial<Omit<AgentAccount, "id" | "createdAt">>,
): Promise<AgentAccount | null> {
  const accounts = loadStoredAccounts();
  const index = accounts.findIndex((a) => a.id === id);
  if (index === -1) return null;
  const updatedAccount: AgentAccount = {
    ...accounts[index],
    ...updates,
    lastUsedAt: new Date().toISOString(),
  };
  accounts[index] = updatedAccount;
  saveStoredAccounts(accounts);
  return updatedAccount;
}

export async function deleteAgentAccount(id: string): Promise<boolean> {
  const accounts = loadStoredAccounts();
  const filtered = accounts.filter((a) => a.id !== id);
  if (filtered.length === accounts.length) return false;
  saveStoredAccounts(filtered);
  return true;
}

export function subscribeAgentAccounts(listener: AccountsListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
