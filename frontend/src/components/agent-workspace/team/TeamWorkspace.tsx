import { useState, useRef, useEffect } from "react";
import { TeamHeader } from "./TeamHeader";
import { TeamParallelLanes } from "./TeamParallelLanes";
import { TeamBroadcastBar } from "./TeamBroadcastBar";
import { TeamCreateModal } from "./TeamCreateModal";
import { AgentSessionWorkspace } from "../../agent-session/AgentSessionWorkspace";
import { AccountPill } from "../../common/AccountPill";
import {
  loadStoredTeams,
  saveStoredTeams,
  SUPPORTED_AGENTS,
  simulateAgentStream,
} from "../../../services/agentTeamService";
import { loadStoredAccounts } from "../../../services/agentAccountService";
import type {
  TeamDefinition,
  TeamViewMode,
  TeamLaneSession,
  AgentProvider,
} from "../../../types/agentWorkspace";
import { DEFAULT_INTERACTIVE_CAPABILITIES } from "../../../types/agentSession";

export function TeamWorkspace() {
  const [teams, setTeams] = useState<TeamDefinition[]>(loadStoredTeams);
  const [currentTeamId, setCurrentTeamId] = useState<string>(
    teams[0]?.id ?? "",
  );
  const currentTeam = teams.find((t) => t.id === currentTeamId) ?? teams[0];

  const [activeMemberId, setActiveMemberId] = useState<string>(
    currentTeam?.leaderMemberId ?? currentTeam?.members[0]?.memberId ?? "",
  );
  const [viewMode, setViewMode] = useState<TeamViewMode>("parallel");
  const [isCreateModalOpen, setIsCreateModalOpen] = useState(false);

  // Per-member session state
  const [laneSessions, setLaneSessions] = useState<
    Record<string, TeamLaneSession>
  >({});

  const cancelMapRef = useRef<Record<string, () => void>>({});

  // Ensure activeMemberId is valid when currentTeam changes
  useEffect(() => {
    if (
      currentTeam &&
      !currentTeam.members.some((m) => m.memberId === activeMemberId)
    ) {
      setActiveMemberId(
        currentTeam.leaderMemberId || currentTeam.members[0]?.memberId || "",
      );
    }
  }, [currentTeamId, currentTeam]);

  // Clean up streaming on unmount
  useEffect(() => {
    return () => {
      Object.values(cancelMapRef.current).forEach((cancel) => cancel());
    };
  }, []);

  if (!currentTeam) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="text-center">
          <p className="text-body-sm text-theme-muted mb-3">暂无协同团队</p>
          <button
            type="button"
            onClick={() => setIsCreateModalOpen(true)}
            className="rounded-xl bg-primary-strong px-4 py-2 text-body-xs font-semibold text-on-primary shadow-sm"
          >
            立即创建团队
          </button>
        </div>
        <TeamCreateModal
          isOpen={isCreateModalOpen}
          onClose={() => setIsCreateModalOpen(false)}
          onTeamCreated={(newTeam) => {
            setTeams(loadStoredTeams());
            setCurrentTeamId(newTeam.id);
            setActiveMemberId(newTeam.leaderMemberId);
          }}
        />
      </div>
    );
  }

  const activeMember =
    currentTeam.members.find((m) => m.memberId === activeMemberId) ??
    currentTeam.members[0];

  const handleUpdateMemberAccount = (
    memberId: string,
    newAccountId: string,
  ) => {
    const updatedMembers = currentTeam.members.map((m) =>
      m.memberId === memberId ? { ...m, accountId: newAccountId } : m,
    );
    const updatedTeam = { ...currentTeam, members: updatedMembers };
    const updatedTeams = teams.map((t) =>
      t.id === currentTeam.id ? updatedTeam : t,
    );
    setTeams(updatedTeams);
    saveStoredTeams(updatedTeams);
  };

  const handleSendToLane = (memberId: string, promptText?: string) => {
    const session = laneSessions[memberId] ?? {
      memberId,
      sessionRef: `lane_${memberId}`,
      items: [],
      isExecuting: false,
      draft: "",
    };

    const text = (promptText ?? session.draft).trim();
    if (!text || session.isExecuting) return;

    // Reset draft and set executing
    setLaneSessions((prev) => ({
      ...prev,
      [memberId]: {
        ...session,
        draft: "",
        isExecuting: true,
      },
    }));

    const member = currentTeam.members.find((m) => m.memberId === memberId);
    const agentDef =
      SUPPORTED_AGENTS.find((a) => a.id === member?.agentId) ??
      SUPPORTED_AGENTS[0];
    const accounts = loadStoredAccounts();
    const account = accounts.find((a) => a.id === member?.accountId);

    const cancel = simulateAgentStream(
      text,
      `${member?.name ?? "成员"} (${agentDef.displayName.split(" ")[0]})`,
      account?.displayName ?? "绑定账号",
      (updatedItems, isFinished) => {
        setLaneSessions((prev) => ({
          ...prev,
          [memberId]: {
            ...prev[memberId],
            items: updatedItems,
            isExecuting: !isFinished,
          },
        }));
      },
    );

    cancelMapRef.current[memberId] = cancel;
  };

  const handleStopLane = (memberId: string) => {
    if (cancelMapRef.current[memberId]) {
      cancelMapRef.current[memberId]();
      delete cancelMapRef.current[memberId];
    }
    setLaneSessions((prev) => {
      const session = prev[memberId];
      if (!session) return prev;
      return {
        ...prev,
        [memberId]: {
          ...session,
          isExecuting: false,
          items: session.items.map((it, idx) =>
            idx === session.items.length - 1 && it.state === "running"
              ? { ...it, state: "cancelled" }
              : it,
          ),
        },
      };
    });
  };

  const handleLaneDraftChange = (memberId: string, draft: string) => {
    setLaneSessions((prev) => ({
      ...prev,
      [memberId]: {
        ...(prev[memberId] ?? {
          memberId,
          sessionRef: `lane_${memberId}`,
          items: [],
          isExecuting: false,
        }),
        draft,
      },
    }));
  };

  const handleBroadcast = (prompt: string, mode: "all" | "leader") => {
    if (mode === "leader") {
      // Send only to the leader
      handleSendToLane(currentTeam.leaderMemberId, prompt);
      setActiveMemberId(currentTeam.leaderMemberId);
    } else {
      // Send concurrently to all team members!
      currentTeam.members.forEach((m) => {
        handleSendToLane(m.memberId, prompt);
      });
    }
  };

  const activeLaneSession = laneSessions[activeMember.memberId] ?? {
    memberId: activeMember.memberId,
    sessionRef: `lane_${activeMember.memberId}`,
    items: [],
    isExecuting: false,
    draft: "",
  };

  const activeAgentDef =
    SUPPORTED_AGENTS.find((a) => a.id === activeMember.agentId) ??
    SUPPORTED_AGENTS[0];

  return (
    <div className="flex h-full flex-col bg-theme-background">
      {/* Top Team Header */}
      <TeamHeader
        teams={teams}
        currentTeam={currentTeam}
        activeMemberId={activeMemberId}
        viewMode={viewMode}
        onSelectTeam={setCurrentTeamId}
        onSelectMember={setActiveMemberId}
        onChangeViewMode={setViewMode}
        onOpenCreateModal={() => setIsCreateModalOpen(true)}
      />

      {/* Main Workspace Body */}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {viewMode === "parallel" ? (
          /* Parallel Lanes View */
          <TeamParallelLanes
            members={currentTeam.members}
            laneSessions={laneSessions}
            onSendToLane={handleSendToLane}
            onStopLane={handleStopLane}
            onLaneDraftChange={handleLaneDraftChange}
            onUpdateMemberAccount={handleUpdateMemberAccount}
          />
        ) : (
          /* Single Focused Member Lane View */
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex h-11 items-center justify-between border-b border-theme-border/50 bg-theme-panel/40 px-4 backdrop-blur-xs">
              <div className="flex items-center gap-2">
                <span className="font-semibold text-body-xs text-theme-text">
                  聚焦成员: {activeMember.name}
                </span>
                <span className="text-[11px] text-theme-muted">
                  ({activeAgentDef.displayName.split(" ")[0]} •{" "}
                  {activeMember.model})
                </span>
              </div>
              <AccountPill
                size="sm"
                provider={activeAgentDef.provider as AgentProvider}
                currentAccountId={activeMember.accountId}
                onSelectAccount={(accId) =>
                  handleUpdateMemberAccount(activeMember.memberId, accId)
                }
              />
            </div>

            <div className="relative flex min-h-0 flex-1 flex-col">
              <AgentSessionWorkspace
                items={activeLaneSession.items}
                draft={activeLaneSession.draft}
                onDraftChange={(draft) =>
                  handleLaneDraftChange(activeMember.memberId, draft)
                }
                onSend={() =>
                  handleSendToLane(
                    activeMember.memberId,
                    activeLaneSession.draft,
                  )
                }
                onStop={() => handleStopLane(activeMember.memberId)}
                isExecuting={activeLaneSession.isExecuting}
                canSend={activeLaneSession.draft.trim().length > 0}
                capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
                emptyTitle={activeMember.name}
                emptyDescription={`角色: ${activeMember.role === "leader" ? "组长" : "组员"} • ${activeAgentDef.displayName.split(" ")[0]}`}
                placeholder={`向聚焦成员 [${activeMember.name}] 发送具体指令...`}
              />
            </div>
          </div>
        )}

        {/* Global Team Broadcast Composer at bottom */}
        <TeamBroadcastBar
          onBroadcast={handleBroadcast}
          memberCount={currentTeam.members.length}
        />
      </div>

      {/* Team Create Modal */}
      <TeamCreateModal
        isOpen={isCreateModalOpen}
        onClose={() => setIsCreateModalOpen(false)}
        onTeamCreated={(newTeam) => {
          setTeams(loadStoredTeams());
          setCurrentTeamId(newTeam.id);
          setActiveMemberId(newTeam.leaderMemberId);
        }}
      />
    </div>
  );
}
