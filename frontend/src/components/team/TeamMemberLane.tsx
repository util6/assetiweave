import { Maximize2, Shield } from "lucide-react";
import React from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  SessionItemSnapshot,
  TeamDetail,
  TeamMember,
  TeamMemberSessionProjection,
  TeamRunSnapshot,
  TeamTask,
} from "../../types/team";
import { AgentSessionWorkspace } from "../agent-session";
import { TeamPlanCard } from "./TeamPlanCard";
import { TeamTaskCard } from "./TeamTaskCard";
import { adaptTeamSessionToWorkspaceProps } from "./teamSessionAdapter";
import type { TeamViewMode } from "./TeamViewToggle";

export interface TeamMemberLaneProps {
  member: TeamMember;
  sessionProjection: TeamMemberSessionProjection | null;
  timelineItems: SessionItemSnapshot[];
  draft: string;
  onDraftChange: (value: string) => void;
  onSend: (message: string) => void | Promise<void>;
  onStop?: () => void | Promise<void>;
  onInterrupt?: () => void | Promise<void>;
  onQueue?: (message: string) => void | Promise<void>;
  isLeader: boolean;
  canSend: boolean;
  disabled?: boolean;
  isExecuting: boolean;
  sending: boolean;
  viewMode: TeamViewMode;
  totalMembers: number;
  isActive: boolean;
  onFocusSingle: (memberId: string) => void;
  roleLabelText: string;
  status: {
    label: string;
    icon: React.ReactNode;
    className: string;
  };
  composerExtra?: React.ReactNode;
  placeholder?: string;
  submitLabel?: string;
  timelineKey?: string;
  projectedTasks?: TeamTask[];
  activeRun?: TeamRunSnapshot | null;
  workflowBusy?: boolean;
  workflowError?: string | null;
  onCancelRun?: () => void;
  onConfirmRun?: () => void;
  allMembers?: TeamMember[];
  team?: TeamDetail;
  onTaskChange?: (
    taskId: string,
    patch: { title?: string; description?: string; owner_member_id?: string },
  ) => void;
  onMoveTask?: (taskId: string, direction: -1 | 1) => void;
  onTaskNavigate?: (taskId: string, ownerMemberId: string | null) => void;
  onReview?: () => void;
}

export function TeamMemberLane({
  activeRun,
  allMembers = [],
  canSend,
  composerExtra,
  disabled = false,
  draft,
  isActive,
  isExecuting,
  isLeader,
  member,
  onCancelRun,
  onConfirmRun,
  onDraftChange,
  onFocusSingle,
  onInterrupt,
  onMoveTask,
  onQueue,
  onReview,
  onSend,
  onStop,
  onTaskChange,
  onTaskNavigate,
  placeholder,
  projectedTasks = [],
  roleLabelText,
  sending,
  sessionProjection,
  status,
  submitLabel,
  team,
  timelineItems,
  timelineKey,
  totalMembers,
  viewMode,
  workflowBusy = false,
  workflowError = null,
}: TeamMemberLaneProps) {
  const { t } = useI18n();

  const timelineExtra =
    projectedTasks.length > 0 || (activeRun && team) ? (
      <>
        {projectedTasks.map((task) => (
          <TeamTaskCard
            key={task.id}
            owner={allMembers.find((m) => m.id === task.owner_member_id)}
            task={task}
          />
        ))}
        {activeRun && team ? (
          <TeamPlanCard
            busy={workflowBusy}
            error={workflowError}
            onCancel={onCancelRun ?? (() => {})}
            onConfirm={onConfirmRun ?? (() => {})}
            onMoveTask={onMoveTask ?? (() => {})}
            onReview={onReview ?? (() => {})}
            onTaskChange={onTaskChange ?? (() => {})}
            onTaskNavigate={onTaskNavigate ?? (() => {})}
            snapshot={activeRun}
            team={team}
          />
        ) : null}
      </>
    ) : undefined;

  const testIdPrefix =
    isActive || viewMode === "single" ? "team" : `team-${member.id}`;

  return (
    <div
      className={`flex h-full min-h-0 flex-col ${
        viewMode === "single"
          ? "w-full flex-1 min-w-0"
          : totalMembers === 2
            ? "min-w-[240px] flex-1 shrink-0 border-r border-theme-card-border/60 last:border-r-0"
            : "min-w-[400px] flex-1 shrink-0 snap-start border-r border-theme-card-border/60 last:border-r-0"
      }`}
      data-testid={`team-member-lane-${member.id}`}
      id={`team-member-lane-${member.id}`}
    >
      {/* Parallel mode lane sub-header */}
      {viewMode === "parallel" ? (
        <div className="flex shrink-0 items-center justify-between border-b border-theme-card-border/50 bg-theme-control/25 px-3 py-1.5">
          <div className="flex min-w-0 items-center gap-1.5 text-caption font-semibold text-on-surface">
            <span className="truncate">{roleLabelText}</span>
            {member.role === "leader" ? (
              <Shield
                aria-label={t("team.chat.leaderBadge") || "Team owner"}
                className="shrink-0 text-primary"
                size={12}
              />
            ) : null}
            <span className="truncate font-mono text-outline font-normal">
              ({member.agent_id})
            </span>
          </div>
          <button
            type="button"
            onClick={() => onFocusSingle(member.id)}
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-caption text-outline transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            title={t("team.view.single") || "Single"}
            aria-label={`Focus ${roleLabelText}`}
          >
            <Maximize2 size={12} />
          </button>
        </div>
      ) : null}

      <AgentSessionWorkspace
        {...adaptTeamSessionToWorkspaceProps({
          activeMember: member,
          activeSession: sessionProjection,
          activeTimelineItems: timelineItems,
          activityDependencyKey: timelineKey,
          canSend,
          composerExtra,
          disabled,
          draft,
          isExecuting,
          isLeader,
          model: member.agent_id,
          onDraftChange,
          onInterrupt,
          onQueue,
          onSend,
          onStop,
          placeholder,
          restoreStatus:
            sessionProjection?.restore_state &&
            sessionProjection.restore_state !== "ready" &&
            sessionProjection.restore_state !== "not-started"
              ? {
                  errorCode: sessionProjection.restore_error_code,
                  icon: status.icon,
                  label: status.label,
                  state: sessionProjection.restore_state,
                  className:
                    status.className === "text-status-remove"
                      ? "border-status-remove/35 bg-status-remove/10 text-status-remove"
                      : "border-theme-nav-active-border/40 bg-theme-nav-active/10 text-on-surface-variant",
                }
              : null,
          roleLabelText,
          sessionResetKey: member.id,
          status,
          submitLabel,
          testIdPrefix,
          timelineExtra,
        })}
        className="min-h-0 flex-1"
      />
    </div>
  );
}
