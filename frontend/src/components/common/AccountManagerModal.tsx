import { useState, useEffect } from "react";
import {
  KeyRound,
  Plus,
  Trash2,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Cpu,
  Sparkles,
  Bot,
  ExternalLink,
} from "lucide-react";
import { DialogFrame } from "../foundation/DialogFrame";
import { PillTabs, type PillTabItem } from "./PillTabs";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Badge } from "../foundation/Badge";
import {
  loadStoredAccounts,
  createAgentAccount,
  deleteAgentAccount,
  subscribeAgentAccounts,
} from "../../services/agentAccountService";
import type { AgentAccount, AgentProvider } from "../../types/agentWorkspace";

export interface AccountManagerModalProps {
  isOpen: boolean;
  onClose: () => void;
  activeAccountId?: string;
  onAccountSelect?: (account: AgentAccount) => void;
  initialProvider?: AgentProvider;
}

const PROVIDER_ICONS: Record<AgentProvider, typeof Cpu> = {
  codex: Cpu,
  antigravity: Sparkles,
  claude: Bot,
  generic: KeyRound,
};

const PROVIDER_LABELS: Record<AgentProvider, string> = {
  codex: "Codex",
  antigravity: "Antigravity",
  claude: "Claude Code",
  generic: "通用 Agent",
};

export function AccountManagerModal({
  isOpen,
  onClose,
  activeAccountId,
  onAccountSelect,
  initialProvider,
}: AccountManagerModalProps) {
  const [accounts, setAccounts] = useState<AgentAccount[]>(loadStoredAccounts);
  const [selectedFilter, setSelectedFilter] = useState<string>(
    initialProvider ?? "all",
  );
  const [isAdding, setIsAdding] = useState(false);

  // New account form state
  const [formProvider, setFormProvider] = useState<AgentProvider>(
    initialProvider ?? "codex",
  );
  const [formDisplayName, setFormDisplayName] = useState("");
  const [formEmail, setFormEmail] = useState("");
  const [formAuthType, setFormAuthType] = useState<"oauth" | "token">("oauth");

  useEffect(() => {
    return subscribeAgentAccounts((updated) => {
      setAccounts(updated);
    });
  }, []);

  if (!isOpen) return null;

  const filterTabs: PillTabItem[] = [
    { id: "all", label: "全部", count: accounts.length },
    {
      id: "codex",
      label: "Codex",
      count: accounts.filter((a) => a.provider === "codex").length,
    },
    {
      id: "antigravity",
      label: "Antigravity",
      count: accounts.filter((a) => a.provider === "antigravity").length,
    },
    {
      id: "claude",
      label: "Claude Code",
      count: accounts.filter((a) => a.provider === "claude").length,
    },
  ];

  const filteredAccounts = accounts.filter((a) =>
    selectedFilter === "all" ? true : a.provider === selectedFilter,
  );

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formDisplayName.trim()) return;

    await createAgentAccount({
      provider: formProvider,
      displayName: formDisplayName.trim(),
      email: formEmail.trim() || undefined,
      authType: formAuthType,
      state: "active",
      quotaSummary: {
        used: "0 requests",
        total: "1,000 requests",
        percentage: 0,
        resetTime: "下月",
      },
    });

    setFormDisplayName("");
    setFormEmail("");
    setIsAdding(false);
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    if (confirm("确定要移除该账号凭据配置吗？")) {
      await deleteAgentAccount(id);
    }
  };

  return (
    <DialogFrame
      layer="default"
      onClose={onClose}
      size="lg"
      icon={<KeyRound className="h-5 w-5" />}
      title="智能体账号凭据管理"
      description="管理 Codex、Antigravity 与 Claude Code 多账号，支持原生沙箱环境切号与配额隔离"
    >
      <div className="space-y-4">
        {/* Header Tabs & Add Button */}
        <div className="flex items-center justify-between gap-2 border-b border-theme-border/40 pb-3">
          <PillTabs
            activeId={selectedFilter}
            items={filterTabs}
            onSelect={(id) => setSelectedFilter(id)}
            size="sm"
          />
          <Button
            onClick={() => setIsAdding(!isAdding)}
            size="sm"
            variant={isAdding ? "outline" : "default"}
          >
            <Plus className="mr-1.5 h-3.5 w-3.5" />
            {isAdding ? "取消添加" : "添加账号"}
          </Button>
        </div>

        {/* Add Form */}
        {isAdding && (
          <form
            onSubmit={handleCreate}
            className="space-y-3.5 rounded-xl border border-primary-strong/30 bg-primary-subtle/10 p-4"
          >
            <div className="text-body-sm font-semibold text-theme-text">
              录入新账号凭据
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="mb-1 block text-body-xs text-theme-muted">
                  所属平台
                </label>
                <select
                  aria-label="所属平台"
                  className="w-full rounded-lg border border-theme-border/60 bg-theme-panel/80 px-2.5 py-1.5 text-body-xs text-theme-text focus:outline-none focus:ring-1 focus:ring-primary-strong"
                  value={formProvider}
                  onChange={(e) =>
                    setFormProvider(e.target.value as AgentProvider)
                  }
                >
                  <option value="codex">Codex CLI / ACP</option>
                  <option value="antigravity">Antigravity (AGY / ACP)</option>
                  <option value="claude">Claude Code</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-body-xs text-theme-muted">
                  认证方式
                </label>
                <select
                  aria-label="认证方式"
                  className="w-full rounded-lg border border-theme-border/60 bg-theme-panel/80 px-2.5 py-1.5 text-body-xs text-theme-text focus:outline-none focus:ring-1 focus:ring-primary-strong"
                  value={formAuthType}
                  onChange={(e) =>
                    setFormAuthType(e.target.value as "oauth" | "token")
                  }
                >
                  <option value="oauth">
                    OAuth 自动同步 (系统凭据/Keychain)
                  </option>
                  <option value="token">API Key / Token 注入</option>
                </select>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="mb-1 block text-body-xs text-theme-muted">
                  账号名称 / 备注 *
                </label>
                <Input
                  placeholder="例如: 个人主号、团队备用"
                  value={formDisplayName}
                  onChange={(e) => setFormDisplayName(e.target.value)}
                  required
                />
              </div>
              <div>
                <label className="mb-1 block text-body-xs text-theme-muted">
                  绑定的邮箱或账户 ID (可选)
                </label>
                <Input
                  placeholder="user@example.com"
                  type="email"
                  value={formEmail}
                  onChange={(e) => setFormEmail(e.target.value)}
                />
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setIsAdding(false)}
              >
                取消
              </Button>
              <Button type="submit" size="sm" variant="default">
                确认添加
              </Button>
            </div>
          </form>
        )}

        {/* Account List */}
        <div className="max-h-[380px] space-y-2.5 overflow-y-auto pr-1">
          {filteredAccounts.length === 0 ? (
            <div className="rounded-xl border border-dashed border-theme-border/60 p-8 text-center text-body-sm text-theme-muted">
              当前分类暂无已配置账号，点击右上角“添加账号”录入。
            </div>
          ) : (
            filteredAccounts.map((acc) => {
              const ProviderIcon = PROVIDER_ICONS[acc.provider] ?? KeyRound;
              const isActive = acc.id === activeAccountId;

              return (
                <div
                  key={acc.id}
                  onClick={() => onAccountSelect?.(acc)}
                  className={`group relative flex cursor-pointer items-center justify-between rounded-xl border p-3.5 transition-all ${
                    isActive
                      ? "border-primary-strong/60 bg-primary-subtle/15 shadow-[0_0_12px_rgba(59,130,246,0.12)]"
                      : "border-theme-border/40 bg-theme-panel/40 hover:border-theme-border-hover hover:bg-theme-panel/60"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-theme-panel/80 text-theme-text shadow-sm">
                      <ProviderIcon className="h-5 w-5 text-primary-strong" />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-body-sm font-semibold text-theme-text">
                          {acc.displayName}
                        </span>
                        <Badge tone="neutral">
                          {PROVIDER_LABELS[acc.provider]}
                        </Badge>
                        {isActive && <Badge tone="primary">当前选择</Badge>}
                      </div>
                      <div className="mt-0.5 flex items-center gap-2 text-body-xs text-theme-muted">
                        <span>{acc.email || "未绑定邮箱"}</span>
                        <span>•</span>
                        <span className="capitalize">{acc.authType}</span>
                        {acc.quotaSummary?.percentage !== undefined && (
                          <>
                            <span>•</span>
                            <span>已用配额 {acc.quotaSummary.percentage}%</span>
                          </>
                        )}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    {/* Status Dot */}
                    <div className="flex items-center gap-1.5 text-body-xs">
                      {acc.state === "active" ? (
                        <>
                          <span className="h-2 w-2 rounded-full bg-status-create shadow-[0_0_6px_rgba(16,185,129,0.5)]" />
                          <span className="text-status-create">正常</span>
                        </>
                      ) : acc.state === "rate_limited" ? (
                        <>
                          <span className="h-2 w-2 rounded-full bg-status-warning shadow-[0_0_6px_rgba(245,158,11,0.5)]" />
                          <span className="text-status-warning">已限流</span>
                        </>
                      ) : (
                        <>
                          <span className="h-2 w-2 rounded-full bg-status-conflict shadow-[0_0_6px_rgba(244,63,94,0.5)]" />
                          <span className="text-status-conflict">已失效</span>
                        </>
                      )}
                    </div>

                    {/* Delete button */}
                    <button
                      type="button"
                      aria-label="删除账号"
                      onClick={(e) => handleDelete(acc.id, e)}
                      className="rounded-lg p-1.5 text-theme-muted opacity-0 transition-all hover:bg-status-conflict/10 hover:text-status-conflict group-hover:opacity-100"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </DialogFrame>
  );
}
