import { Crown, User, Sparkles, Cpu, Bot } from "lucide-react";
import { AgentSessionWorkspace } from "../../agent-session/AgentSessionWorkspace";
import { AccountPill } from "../../common/AccountPill";
import { Badge } from "../../foundation/Badge";
import { SUPPORTED_AGENTS } from "../../../services/agentTeamService";
import type {
  TeamMemberConfig,
  TeamLaneSession,
  AgentProvider,
} from "../../../types/agentWorkspace";
import { DEFAULT_INTERACTIVE_CAPABILITIES } from "../../../types/agentSession";

export interface TeamParallelLanesProps {
  members: TeamMemberConfig[];
  laneSessions: Record<string, TeamLaneSession>;
  onSendToLane: (memberId: string, prompt: string) => void;
  onStopLane: (memberId: string) => void;
  onLaneDraftChange: (memberId: string, draft: string) => void;
  onUpdateMemberAccount: (memberId: string, accountId: string) => void;
}

const AGENT_ICONS: Record<string, typeof Cpu> = {
  codex: Cpu,
  antigravity: Sparkles,
  claude: Bot,
};

export function TeamParallelLanes({
  members,
  laneSessions,
  onSendToLane,
  onStopLane,
  onLaneDraftChange,
  onUpdateMemberAccount,
}: TeamParallelLanesProps) {
  return (
    <div className="flex min-h-0 flex-1 overflow-x-auto divide-x divide-theme-border/60 bg-theme-background">
      {members.map((member) => {
        const session = laneSessions[member.memberId] ?? {
          memberId: member.memberId,
          sessionRef: `lane_${member.memberId}`,
          items: [],
          isExecuting: false,
          draft: "",
        };

        const agentDef =
          SUPPORTED_AGENTS.find((a) => a.id === member.agentId) ??
          SUPPORTED_AGENTS[0];
        const AgentIcon = AGENT_ICONS[member.agentId] ?? Cpu;

        return (
          <div
            key={member.memberId}
            className="flex min-w-[340px] max-w-[480px] flex-1 flex-col bg-theme-panel/15"
          >
            {/* Lane Header */}
            <div className="flex h-11 items-center justify-between border-b border-theme-border/50 bg-theme-panel/40 px-3 py-1.5 backdrop-blur-xs">
              <div className="flex items-center gap-2 truncate">
                {member.role === "leader" ? (
                  <span
                    className="flex h-5 items-center gap-1 rounded-md bg-status-warning/15 px-1.5 text-[11px] font-semibold text-status-warning"
                    title="团队组长 (Leader)"
                  >
                    <Crown className="h-3 w-3" />
                    <span>组长</span>
                  </span>
                ) : (
                  <span
                    className="flex h-5 items-center gap-1 rounded-md bg-theme-panel/80 px-1.5 text-[11px] font-medium text-theme-muted"
                    title="协同成员 (Teammate)"
                  >
                    <User className="h-3 w-3" />
                  </span>
                )}

                <span className="font-semibold text-body-xs text-theme-text truncate">
                  {member.name}
                </span>

                <span className="text-[11px] text-theme-muted truncate">
                  ({agentDef.displayName.split(" ")[0]} • {member.model})
                </span>
              </div>

              {/* Lane Account Pill (Cockpit 切号!) */}
              <div className="shrink-0">
                <AccountPill
                  size="sm"
                  provider={agentDef.provider as AgentProvider}
                  currentAccountId={member.accountId}
                  onSelectAccount={(accId) =>
                    onUpdateMemberAccount(member.memberId, accId)
                  }
                />
              </div>
            </div>

            {/* Lane Interactive Session */}
            <div className="relative flex min-h-0 flex-1 flex-col">
              <AgentSessionWorkspace
                items={session.items}
                draft={session.draft}
                onDraftChange={(draft) =>
                  onLaneDraftChange(member.memberId, draft)
                }
                onSend={() => onSendToLane(member.memberId, session.draft)}
                onStop={() => onStopLane(member.memberId)}
                isExecuting={session.isExecuting}
                canSend={session.draft.trim().length > 0}
                capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
                emptyTitle={member.name}
                emptyDescription={`角色: ${member.role === "leader" ? "组长" : "组员"} • ${agentDef.displayName.split(" ")[0]}`}
                placeholder={`指令将仅发给 [${member.name}]...`}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
