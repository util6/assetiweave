import { useState, useRef, useEffect } from "react";
import {
  ChevronDown,
  Cpu,
  Sparkles,
  Bot,
  KeyRound,
  Plus,
  Check,
  ExternalLink,
} from "lucide-react";
import {
  loadStoredAccounts,
  subscribeAgentAccounts,
} from "../../services/agentAccountService";
import type { AgentAccount, AgentProvider } from "../../types/agentWorkspace";
import { AccountManagerModal } from "./AccountManagerModal";

export interface AccountPillProps {
  provider: AgentProvider;
  currentAccountId?: string;
  onSelectAccount: (accountId: string) => void;
  className?: string;
  size?: "sm" | "md";
}

const PROVIDER_ICONS: Record<AgentProvider, typeof Cpu> = {
  codex: Cpu,
  antigravity: Sparkles,
  claude: Bot,
  generic: KeyRound,
};

export function AccountPill({
  provider,
  currentAccountId,
  onSelectAccount,
  className = "",
  size = "md",
}: AccountPillProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [isManagerOpen, setIsManagerOpen] = useState(false);
  const [accounts, setAccounts] = useState<AgentAccount[]>(loadStoredAccounts);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    return subscribeAgentAccounts((updated) => {
      setAccounts(updated);
    });
  }, []);

  // Close on outside click
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [isOpen]);

  const providerAccounts = accounts.filter((a) => a.provider === provider);
  const currentAccount =
    providerAccounts.find((a) => a.id === currentAccountId) ??
    providerAccounts[0];

  const ProviderIcon = PROVIDER_ICONS[provider] ?? KeyRound;

  return (
    <>
      <div
        className={`relative inline-block text-left ${className}`}
        ref={dropdownRef}
      >
        <button
          type="button"
          onClick={() => setIsOpen(!isOpen)}
          aria-expanded={isOpen}
          aria-haspopup="true"
          title={`当前账号: ${currentAccount ? currentAccount.displayName : "未绑定账号"} (点击切号)`}
          className={`group flex items-center gap-1.5 rounded-full border border-theme-border/60 bg-theme-panel/70 backdrop-blur-md transition-all duration-150 hover:border-theme-border-hover hover:bg-theme-panel focus:outline-none focus:ring-1 focus:ring-primary-strong/40 shadow-sm ${
            size === "sm"
              ? "px-2.5 py-1 text-body-xs"
              : "px-3 py-1.5 text-body-xs"
          }`}
        >
          <ProviderIcon className="h-3.5 w-3.5 text-primary-strong transition-transform group-hover:scale-105" />

          {currentAccount ? (
            <div className="flex items-center gap-1.5 max-w-[160px] truncate">
              {/* Status indicator dot */}
              <span
                className={`inline-block h-1.5 w-1.5 rounded-full ${
                  currentAccount.state === "active"
                    ? "bg-status-create shadow-[0_0_4px_rgba(16,185,129,0.6)]"
                    : currentAccount.state === "rate_limited"
                      ? "bg-status-warning shadow-[0_0_4px_rgba(245,158,11,0.6)]"
                      : "bg-status-conflict shadow-[0_0_4px_rgba(244,63,94,0.6)]"
                }`}
              />
              <span className="font-medium text-theme-text truncate">
                {currentAccount.displayName}
              </span>
            </div>
          ) : (
            <span className="text-theme-muted">未选择账号</span>
          )}

          <ChevronDown
            className={`h-3 w-3 text-theme-muted transition-transform duration-150 ${
              isOpen ? "rotate-180 text-theme-text" : ""
            }`}
          />
        </button>

        {/* Dropdown Menu */}
        {isOpen && (
          <div className="absolute left-0 mt-1.5 w-64 origin-top-left rounded-2xl border border-theme-border/80 bg-theme-surface/95 p-1.5 shadow-xl backdrop-blur-xl z-50 animate-in fade-in zoom-in-95 duration-100">
            <div className="px-2.5 py-1.5 text-body-xs font-semibold uppercase tracking-wider text-theme-muted">
              切换 {provider.toUpperCase()} 账号
            </div>

            <div className="max-h-56 space-y-1 overflow-y-auto">
              {providerAccounts.length === 0 ? (
                <div className="px-3 py-2 text-body-xs text-theme-muted">
                  暂无此平台账号，请添加
                </div>
              ) : (
                providerAccounts.map((acc) => {
                  const isSelected = acc.id === currentAccount?.id;
                  return (
                    <button
                      key={acc.id}
                      type="button"
                      onClick={() => {
                        onSelectAccount(acc.id);
                        setIsOpen(false);
                      }}
                      className={`flex w-full items-center justify-between rounded-xl px-2.5 py-2 text-left text-body-xs transition-colors ${
                        isSelected
                          ? "bg-primary-subtle/30 text-primary-strong font-medium"
                          : "text-theme-text hover:bg-theme-panel/70"
                      }`}
                    >
                      <div className="flex items-center gap-2 truncate">
                        <span
                          className={`h-1.5 w-1.5 rounded-full ${
                            acc.state === "active"
                              ? "bg-status-create"
                              : acc.state === "rate_limited"
                                ? "bg-status-warning"
                                : "bg-status-conflict"
                          }`}
                        />
                        <div className="truncate">
                          <div className="truncate font-medium">
                            {acc.displayName}
                          </div>
                          {acc.email && (
                            <div className="text-[11px] text-theme-muted truncate">
                              {acc.email}
                            </div>
                          )}
                        </div>
                      </div>
                      {isSelected && (
                        <Check className="h-4 w-4 shrink-0 text-primary-strong" />
                      )}
                    </button>
                  );
                })
              )}
            </div>

            <div className="mt-1 border-t border-theme-border/40 pt-1">
              <button
                type="button"
                onClick={() => {
                  setIsOpen(false);
                  setIsManagerOpen(true);
                }}
                className="flex w-full items-center gap-1.5 rounded-xl px-2.5 py-1.5 text-body-xs font-medium text-theme-muted hover:bg-theme-panel hover:text-theme-text transition-colors"
              >
                <Plus className="h-3.5 w-3.5" />
                <span>管理与添加账号...</span>
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Account Manager Modal */}
      <AccountManagerModal
        isOpen={isManagerOpen}
        onClose={() => setIsManagerOpen(false)}
        activeAccountId={currentAccount?.id}
        initialProvider={provider}
        onAccountSelect={(acc) => {
          if (acc.provider === provider) {
            onSelectAccount(acc.id);
          }
          setIsManagerOpen(false);
        }}
      />
    </>
  );
}
