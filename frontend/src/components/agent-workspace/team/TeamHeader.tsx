import { useState, useRef, useEffect } from "react";
import { Users2, Plus, ChevronDown, Crown, User, KeyRound } from "lucide-react";
import { PillTabs, type PillTabItem } from "../../common/PillTabs";
import { TeamViewToggle } from "./TeamViewToggle";
import { Button } from "../../ui/button";
import { AccountManagerModal } from "../../common/AccountManagerModal";
import type {
  TeamDefinition,
  TeamViewMode,
} from "../../../types/agentWorkspace";

export interface TeamHeaderProps {
  teams: TeamDefinition[];
  currentTeam: TeamDefinition;
  activeMemberId: string;
  viewMode: TeamViewMode;
  onSelectTeam: (teamId: string) => void;
  onSelectMember: (memberId: string) => void;
  onChangeViewMode: (mode: TeamViewMode) => void;
  onOpenCreateModal: () => void;
}

export function TeamHeader({
  teams,
  currentTeam,
  activeMemberId,
  viewMode,
  onSelectTeam,
  onSelectMember,
  onChangeViewMode,
  onOpenCreateModal,
}: TeamHeaderProps) {
  const [isTeamMenuOpen, setIsTeamMenuOpen] = useState(false);
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);
  const teamMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (
        teamMenuRef.current &&
        !teamMenuRef.current.contains(event.target as Node)
      ) {
        setIsTeamMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, []);

  const memberTabs: PillTabItem[] = currentTeam.members.map((m) => ({
    id: m.memberId,
    label: m.name,
    icon:
      m.role === "leader" ? (
        <Crown className="h-3 w-3 text-status-warning shrink-0" />
      ) : (
        <User className="h-3 w-3 text-theme-muted shrink-0" />
      ),
  }));

  return (
    <>
      <header className="flex h-13 items-center justify-between border-b border-theme-border/60 bg-theme-panel/40 px-4 backdrop-blur-md">
        {/* Left: Team Selector & Create Button */}
        <div className="flex items-center gap-2">
          <div className="relative" ref={teamMenuRef}>
            <button
              type="button"
              onClick={() => setIsTeamMenuOpen(!isTeamMenuOpen)}
              className="flex items-center gap-2 rounded-xl border border-theme-border/60 bg-theme-panel/60 px-3 py-1.5 text-body-xs font-semibold text-theme-text transition-all hover:border-theme-border-hover hover:bg-theme-panel shadow-sm"
            >
              <Users2 className="h-4 w-4 text-primary-strong" />
              <span>{currentTeam.name}</span>
              <ChevronDown
                className={`h-3 w-3 text-theme-muted transition-transform ${
                  isTeamMenuOpen ? "rotate-180" : ""
                }`}
              />
            </button>

            {isTeamMenuOpen && (
              <div className="absolute left-0 mt-1.5 w-64 rounded-2xl border border-theme-border/80 bg-theme-surface/95 p-1.5 shadow-xl backdrop-blur-xl z-50 animate-in fade-in zoom-in-95 duration-100">
                <div className="px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wider text-theme-muted">
                  我的协同团队
                </div>
                {teams.map((t) => {
                  const isSelected = t.id === currentTeam.id;
                  return (
                    <button
                      key={t.id}
                      type="button"
                      onClick={() => {
                        onSelectTeam(t.id);
                        setIsTeamMenuOpen(false);
                      }}
                      className={`flex w-full items-center justify-between rounded-xl px-2.5 py-2 text-left text-body-xs transition-colors ${
                        isSelected
                          ? "bg-primary-subtle/30 text-primary-strong font-medium"
                          : "text-theme-text hover:bg-theme-panel/70"
                      }`}
                    >
                      <div className="truncate">
                        <div className="font-medium truncate">{t.name}</div>
                        <div className="text-[11px] text-theme-muted">
                          {t.members.length} 名智能体成员
                        </div>
                      </div>
                    </button>
                  );
                })}

                <div className="mt-1 border-t border-theme-border/40 pt-1">
                  <button
                    type="button"
                    onClick={() => {
                      setIsTeamMenuOpen(false);
                      onOpenCreateModal();
                    }}
                    className="flex w-full items-center gap-1.5 rounded-xl px-2.5 py-1.5 text-body-xs font-medium text-primary-strong hover:bg-primary-subtle/20 transition-colors"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    <span>创建新团队...</span>
                  </button>
                </div>
              </div>
            )}
          </div>

          <Button
            onClick={onOpenCreateModal}
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            title="创建新团队"
          >
            <Plus className="h-4 w-4" />
          </Button>
        </div>

        {/* Center: Member Tabs */}
        <div className="flex items-center">
          <PillTabs
            activeId={activeMemberId}
            items={memberTabs}
            onSelect={(id) => onSelectMember(id)}
            size="sm"
          />
        </div>

        {/* Right: View Mode Toggle & Account Manager */}
        <div className="flex items-center gap-2.5">
          <TeamViewToggle mode={viewMode} onChange={onChangeViewMode} />

          <Button
            onClick={() => setIsAccountModalOpen(true)}
            variant="outline"
            size="sm"
            className="gap-1.5"
            title="管理智能体账号凭据"
          >
            <KeyRound className="h-3.5 w-3.5 text-theme-muted" />
            <span>账号库</span>
          </Button>
        </div>
      </header>

      <AccountManagerModal
        isOpen={isAccountModalOpen}
        onClose={() => setIsAccountModalOpen(false)}
      />
    </>
  );
}
