import { useState, useRef, useEffect } from "react";
import { Sparkles, Search, Zap, TestTube2, FileCode2 } from "lucide-react";
import { SingleChatHeader } from "./SingleChatHeader";
import { AgentSessionWorkspace } from "../../agent-session/AgentSessionWorkspace";
import {
  SUPPORTED_AGENTS,
  simulateAgentStream,
} from "../../../services/agentTeamService";
import {
  loadStoredAccounts,
  subscribeAgentAccounts,
} from "../../../services/agentAccountService";
import type { AgentSessionItemView } from "../../../types/agentSession";
import { DEFAULT_INTERACTIVE_CAPABILITIES } from "../../../types/agentSession";

export function SingleChatWorkspace() {
  const [agents] = useState(SUPPORTED_AGENTS);
  const [selectedAgentId, setSelectedAgentId] = useState(
    SUPPORTED_AGENTS[0].id,
  );
  const currentAgent =
    agents.find((a) => a.id === selectedAgentId) ?? agents[0];

  const [selectedModel, setSelectedModel] = useState(currentAgent.defaultModel);
  const [accounts, setAccounts] = useState(loadStoredAccounts);

  const providerAccounts = accounts.filter(
    (a) => a.provider === currentAgent.provider,
  );
  const [selectedAccountId, setSelectedAccountId] = useState<string>(
    providerAccounts[0]?.id ?? "",
  );

  const [items, setItems] = useState<AgentSessionItemView[]>([]);
  const [draft, setDraft] = useState("");
  const [isExecuting, setIsExecuting] = useState(false);
  const cancelStreamRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return subscribeAgentAccounts((updated) => {
      setAccounts(updated);
    });
  }, []);

  // When agent changes, update model & account
  const handleSelectAgent = (agentId: string) => {
    setSelectedAgentId(agentId);
    const agent = agents.find((a) => a.id === agentId) ?? agents[0];
    setSelectedModel(agent.defaultModel);
    const accs = accounts.filter((a) => a.provider === agent.provider);
    if (accs.length > 0) {
      setSelectedAccountId(accs[0].id);
    }
  };

  const currentAccount = accounts.find((a) => a.id === selectedAccountId);

  const handleSend = (textToSend?: string) => {
    const text = (textToSend ?? draft).trim();
    if (!text || isExecuting) return;

    setDraft("");
    setIsExecuting(true);

    const cancel = simulateAgentStream(
      text,
      currentAgent.displayName,
      currentAccount?.displayName ?? "默认账号",
      (updatedItems, isFinished) => {
        setItems(updatedItems);
        if (isFinished) {
          setIsExecuting(false);
        }
      },
    );

    cancelStreamRef.current = cancel;
  };

  const handleStop = () => {
    if (cancelStreamRef.current) {
      cancelStreamRef.current();
      cancelStreamRef.current = null;
    }
    setIsExecuting(false);
    // Mark last item as cancelled
    setItems((prev) =>
      prev.map((item, idx) =>
        idx === prev.length - 1 && item.state === "running"
          ? { ...item, state: "cancelled" }
          : item,
      ),
    );
  };

  const handleNewChat = () => {
    handleStop();
    setItems([]);
    setDraft("");
  };

  const QUICK_PROMPTS = [
    {
      icon: Search,
      title: "诊断与检查",
      prompt: "检查项目中现存的依赖与配置，并报告潜在的架构冲突与代码异味。",
    },
    {
      icon: Zap,
      title: "免代理切号测试",
      prompt:
        "模拟调用 Codex 与 Antigravity，验证基于环境变量注入的账号沙箱隔离特性。",
    },
    {
      icon: TestTube2,
      title: "运行验证套件",
      prompt: "执行前端全部单元测试与类型检查，输出详细的通过矩阵与建议。",
    },
    {
      icon: FileCode2,
      title: "架构设计与重构",
      prompt:
        "为当前业务模块设计一份职责明确、符合高内聚低耦合原则的重构方案。",
    },
  ];

  return (
    <div className="flex h-full flex-col bg-theme-background">
      <SingleChatHeader
        agents={agents}
        selectedAgentId={selectedAgentId}
        selectedModel={selectedModel}
        selectedAccountId={selectedAccountId}
        isExecuting={isExecuting}
        onSelectAgent={handleSelectAgent}
        onSelectModel={setSelectedModel}
        onSelectAccount={setSelectedAccountId}
        onNewChat={handleNewChat}
      />

      <div className="relative flex min-h-0 flex-1 flex-col">
        <AgentSessionWorkspace
          items={items}
          draft={draft}
          onDraftChange={setDraft}
          onSend={() => handleSend(draft)}
          onStop={handleStop}
          isExecuting={isExecuting}
          canSend={draft.trim().length > 0}
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          emptyTitle={`你好，我是 ${currentAgent.displayName.split(" ")[0]}`}
          emptyDescription={`当前已就绪，已关联账号 [${currentAccount?.displayName ?? "未选择"}]。请直接输入提示词或选择预设指令开始。`}
          placeholder={`以 [${currentAccount?.displayName ?? "未选账号"}] 的身份向 ${currentAgent.displayName.split(" ")[0]} 发送指令...`}
          timelineExtra={
            items.length === 0 ? (
              <div className="mx-auto max-w-2xl px-4 py-8">
                <div className="mb-4 text-center">
                  <div className="inline-flex h-12 w-12 items-center justify-center rounded-2xl bg-primary-subtle/30 text-primary-strong shadow-sm mb-3">
                    <Sparkles className="h-6 w-6" />
                  </div>
                  <h3 className="text-body font-semibold text-theme-text">
                    快速开始会话
                  </h3>
                  <p className="mt-1 text-body-xs text-theme-muted">
                    点击下方推荐提示词，即可立即唤起智能体执行：
                  </p>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  {QUICK_PROMPTS.map((qp, idx) => {
                    const Icon = qp.icon;
                    return (
                      <button
                        key={idx}
                        type="button"
                        onClick={() => handleSend(qp.prompt)}
                        className="group flex flex-col items-start rounded-2xl border border-theme-border/50 bg-theme-panel/40 p-3.5 text-left transition-all hover:border-primary-strong/40 hover:bg-theme-panel hover:shadow-md"
                      >
                        <div className="mb-2 flex h-8 w-8 items-center justify-center rounded-xl bg-theme-panel text-primary-strong transition-transform group-hover:scale-105">
                          <Icon className="h-4 w-4" />
                        </div>
                        <div className="text-body-xs font-semibold text-theme-text group-hover:text-primary-strong">
                          {qp.title}
                        </div>
                        <div className="mt-1 text-[11px] text-theme-muted line-clamp-2">
                          {qp.prompt}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
            ) : null
          }
        />
      </div>
    </div>
  );
}
