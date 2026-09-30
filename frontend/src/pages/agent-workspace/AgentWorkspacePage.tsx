import { useState } from "react";
import { MessageSquare, Users2, KeyRound, Sparkles } from "lucide-react";
import { PillTabs, type PillTabItem } from "../../components/common/PillTabs";
import { SingleChatWorkspace } from "../../components/agent-workspace/single/SingleChatWorkspace";
import { TeamWorkspace } from "../../components/agent-workspace/team/TeamWorkspace";
import { AccountManagerModal } from "../../components/common/AccountManagerModal";
import { Button } from "../../components/ui/button";

export type WorkspaceTabId = "single" | "team";

export function AgentWorkspacePage() {
  const [activeTab, setActiveTab] = useState<WorkspaceTabId>("single");
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);

  const tabs: PillTabItem<WorkspaceTabId>[] = [
    {
      id: "single",
      label: "智能体单聊 (Single ACP)",
      icon: <MessageSquare className="h-3.5 w-3.5" />,
    },
    {
      id: "team",
      label: "团队协作 (Team Workspace)",
      icon: <Users2 className="h-3.5 w-3.5" />,
    },
  ];

  return (
    <div className="flex h-full w-full flex-col bg-theme-background">
      {/* Top Universal Mode Switcher Bar */}
      <div className="flex h-12 items-center justify-between border-b border-theme-border/60 bg-theme-panel/70 px-4 backdrop-blur-md">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 text-primary-strong">
            <Sparkles className="h-4 w-4" />
            <span className="text-body-xs font-semibold uppercase tracking-wider text-theme-text">
              智能体工作台
            </span>
          </div>

          <div className="h-4 w-[1px] bg-theme-border/60" />

          {/* Mode PillTabs */}
          <PillTabs<WorkspaceTabId>
            activeId={activeTab}
            items={tabs}
            onSelect={(id) => setActiveTab(id)}
            size="sm"
          />
        </div>

        <div className="flex items-center gap-2">
          <Button
            onClick={() => setIsAccountModalOpen(true)}
            variant="outline"
            size="sm"
            className="gap-1.5"
            title="查看并管理所有平台的智能体账号与凭据"
          >
            <KeyRound className="h-3.5 w-3.5 text-primary-strong" />
            <span>智能体账号库</span>
          </Button>
        </div>
      </div>

      {/* Main Workspace Body */}
      <div className="min-h-0 flex-1">
        {activeTab === "single" ? <SingleChatWorkspace /> : <TeamWorkspace />}
      </div>

      {/* Global Account Manager Modal */}
      <AccountManagerModal
        isOpen={isAccountModalOpen}
        onClose={() => setIsAccountModalOpen(false)}
      />
    </div>
  );
}

export default AgentWorkspacePage;
