import { RotateCcw, Sparkles, Bot, Cpu, Plus, ChevronDown } from "lucide-react";
import { useState, useRef, useEffect } from "react";
import { AccountPill } from "../../common/AccountPill";
import { Badge } from "../../foundation/Badge";
import { Button } from "../../ui/button";
import type { AgentWorkspaceDefinition } from "../../../types/agentWorkspace";

export interface SingleChatHeaderProps {
  agents: AgentWorkspaceDefinition[];
  selectedAgentId: string;
  selectedModel: string;
  selectedAccountId?: string;
  isExecuting?: boolean;
  onSelectAgent: (agentId: string) => void;
  onSelectModel: (model: string) => void;
  onSelectAccount: (accountId: string) => void;
  onNewChat: () => void;
}

const AGENT_ICONS: Record<string, typeof Cpu> = {
  codex: Cpu,
  antigravity: Sparkles,
  claude: Bot,
};

export function SingleChatHeader({
  agents,
  selectedAgentId,
  selectedModel,
  selectedAccountId,
  isExecuting = false,
  onSelectAgent,
  onSelectModel,
  onSelectAccount,
  onNewChat,
}: SingleChatHeaderProps) {
  const currentAgent =
    agents.find((a) => a.id === selectedAgentId) ?? agents[0];
  const [isAgentMenuOpen, setIsAgentMenuOpen] = useState(false);
  const [isModelMenuOpen, setIsModelMenuOpen] = useState(false);

  const agentMenuRef = useRef<HTMLDivElement>(null);
  const modelMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        agentMenuRef.current &&
        !agentMenuRef.current.contains(event.target as Node)
      ) {
        setIsAgentMenuOpen(false);
      }
      if (
        modelMenuRef.current &&
        !modelMenuRef.current.contains(event.target as Node)
      ) {
        setIsModelMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  const AgentIcon = AGENT_ICONS[currentAgent?.id] ?? Cpu;
  const currentModelDef =
    currentAgent?.availableModels.find((m) => m.id === selectedModel) ??
    currentAgent?.availableModels[0];

  return (
    <header className="flex h-13 items-center justify-between border-b border-theme-border/60 bg-theme-panel/40 px-4 backdrop-blur-md">
      {/* Left controls: Agent Selector, Model Selector, AccountPill */}
      <div className="flex items-center gap-2.5">
        {/* Agent Dropdown */}
        <div className="relative" ref={agentMenuRef}>
          <button
            type="button"
            onClick={() => setIsAgentMenuOpen(!isAgentMenuOpen)}
            className="flex items-center gap-2 rounded-xl border border-theme-border/60 bg-theme-panel/60 px-3 py-1.5 text-body-xs font-semibold text-theme-text transition-all hover:border-theme-border-hover hover:bg-theme-panel shadow-sm"
          >
            <AgentIcon className="h-4 w-4 text-primary-strong" />
            <span>{currentAgent?.displayName.split(" ")[0]}</span>
            <ChevronDown
              className={`h-3 w-3 text-theme-muted transition-transform ${
                isAgentMenuOpen ? "rotate-180" : ""
              }`}
            />
          </button>

          {isAgentMenuOpen && (
            <div className="absolute left-0 mt-1.5 w-60 rounded-2xl border border-theme-border/80 bg-theme-surface/95 p-1.5 shadow-xl backdrop-blur-xl z-50 animate-in fade-in zoom-in-95 duration-100">
              <div className="px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wider text-theme-muted">
                选择智能体
              </div>
              {agents.map((agent) => {
                const Icon = AGENT_ICONS[agent.id] ?? Cpu;
                const isSelected = agent.id === currentAgent?.id;
                return (
                  <button
                    key={agent.id}
                    type="button"
                    onClick={() => {
                      onSelectAgent(agent.id);
                      setIsAgentMenuOpen(false);
                    }}
                    className={`flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left text-body-xs transition-colors ${
                      isSelected
                        ? "bg-primary-subtle/30 text-primary-strong font-medium"
                        : "text-theme-text hover:bg-theme-panel/70"
                    }`}
                  >
                    <Icon className="h-4 w-4 text-primary-strong shrink-0" />
                    <div>
                      <div className="font-medium">{agent.displayName}</div>
                      <div className="text-[11px] text-theme-muted line-clamp-1">
                        {agent.description}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Model Dropdown */}
        <div className="relative" ref={modelMenuRef}>
          <button
            type="button"
            onClick={() => setIsModelMenuOpen(!isModelMenuOpen)}
            className="flex items-center gap-1.5 rounded-xl border border-theme-border/40 bg-theme-panel/40 px-2.5 py-1.5 text-body-xs text-theme-text transition-all hover:border-theme-border-hover hover:bg-theme-panel shadow-sm"
          >
            <span className="font-medium">{currentModelDef?.label}</span>
            <ChevronDown
              className={`h-3 w-3 text-theme-muted transition-transform ${
                isModelMenuOpen ? "rotate-180" : ""
              }`}
            />
          </button>

          {isModelMenuOpen && (
            <div className="absolute left-0 mt-1.5 w-64 rounded-2xl border border-theme-border/80 bg-theme-surface/95 p-1.5 shadow-xl backdrop-blur-xl z-50 animate-in fade-in zoom-in-95 duration-100">
              <div className="px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wider text-theme-muted">
                选择模型
              </div>
              {currentAgent?.availableModels.map((m) => {
                const isSelected = m.id === selectedModel;
                return (
                  <button
                    key={m.id}
                    type="button"
                    onClick={() => {
                      onSelectModel(m.id);
                      setIsModelMenuOpen(false);
                    }}
                    className={`flex w-full items-center justify-between rounded-xl px-2.5 py-1.5 text-left text-body-xs transition-colors ${
                      isSelected
                        ? "bg-primary-subtle/30 text-primary-strong font-medium"
                        : "text-theme-text hover:bg-theme-panel/70"
                    }`}
                  >
                    <div>
                      <div className="font-medium">{m.label}</div>
                      {m.description && (
                        <div className="text-[11px] text-theme-muted">
                          {m.description}
                        </div>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* The Star of the Show: AccountPill (Cockpit-style 切号!) */}
        <div className="flex items-center gap-1.5 pl-1 border-l border-theme-border/50">
          <AccountPill
            provider={currentAgent?.provider ?? "codex"}
            currentAccountId={selectedAccountId}
            onSelectAccount={onSelectAccount}
          />
        </div>
      </div>

      {/* Right controls: New Chat, Status */}
      <div className="flex items-center gap-2">
        {isExecuting && (
          <Badge tone="primary" className="animate-pulse">
            正在思考与执行...
          </Badge>
        )}
        <Button
          onClick={onNewChat}
          variant="outline"
          size="sm"
          className="gap-1.5"
          title="开启新会话"
        >
          <Plus className="h-3.5 w-3.5" />
          <span>新会话</span>
        </Button>
      </div>
    </header>
  );
}
