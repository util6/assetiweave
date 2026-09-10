import {
  Activity,
  Bot,
  CheckCircle2,
  CircleAlert,
  Clock3,
  LoaderCircle,
  MessageSquare,
  Sparkles,
} from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { Button } from "../ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import { useTeamSession } from "../../app/backgroundTasks/TeamSessionProvider";
import { teamTaskAnchor } from "./TeamTaskCard";
import { TeamMemberLane } from "./TeamMemberLane";
import { TeamMemberTabs } from "./TeamMemberTabs";
import { TeamWorkspaceHeader } from "./TeamWorkspaceHeader";
import { useTeamViewMode } from "./useTeamViewMode";
import type {
  SessionItemSnapshot,
  TeamDetail,
  TeamMember,
  TeamMemberRestoreState,
  TeamMemberSessionProjection,
  TeamRunSnapshot,
  TeamTask,
} from "../../types/team";

export interface TeamWorkspaceShellProps {
  team: TeamDetail;
  activeMemberId: string | null;
  onActiveMemberChange: (memberId: string) => void;
  onOpenDetails: () => void;
  onEdit: () => void;
  onDelete: () => void;
  runSnapshot: TeamRunSnapshot | null;
  workflowBusy: boolean;
  workflowError: string | null;
  onStartTeamDraft: (message: string) => void;
  onTaskChange: (
    taskId: string,
    patch: { title?: string; description?: string; owner_member_id?: string },
  ) => void;
  onMoveTask: (taskId: string, direction: -1 | 1) => void;
  onReview: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

interface OptimisticUserMessage {
  clientId: string;
  memberId: string;
  message: string;
  baselineExecutionId: string | null;
  executionId: string | null;
  state: "sending" | "accepted" | "failed";
  errorCode: string | null;
}

export function TeamWorkspaceShell({
  activeMemberId: selectedMemberId,
  onDelete,
  onEdit,
  onOpenDetails,
  onActiveMemberChange,
  onCancel,
  onConfirm,
  onMoveTask,
  onReview,
  onStartTeamDraft,
  onTaskChange,
  runSnapshot,
  team,
  workflowBusy,
  workflowError,
}: TeamWorkspaceShellProps) {
  const { t } = useI18n();
  const session = useTeamSession();

  // Roster ordering: Leader first, then remaining teammates sorted by sort_order
  const members = useMemo(() => {
    const leader = team.members.find((member) => member.role === "leader");
    const teammates = team.members
      .filter((member) => member.id !== leader?.id)
      .sort(
        (left, right) =>
          left.sort_order - right.sort_order || left.id.localeCompare(right.id),
      );
    return leader ? [leader, ...teammates] : teammates;
  }, [team.members]);

  const leader =
    members.find((member) => member.role === "leader") ?? members[0] ?? null;

  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [optimisticMessages, setOptimisticMessages] = useState<
    OptimisticUserMessage[]
  >([]);
  const [composerMode, setComposerMode] = useState<"normal" | "task">("normal");
  const [pendingTaskNavigation, setPendingTaskNavigation] = useState<
    string | null
  >(null);

  const laneRefs = useRef<Record<string, HTMLDivElement | null>>({});

  const { effectiveMode, setMode } = useTeamViewMode(team.id);

  const activeMember =
    members.find((member) => member.id === selectedMemberId) ?? leader;

  const isLeader = activeMember?.role === "leader";
  const activeRun =
    isLeader && runSnapshot?.run.team_id === team.id ? runSnapshot : null;

  const taskModeBusy =
    workflowBusy ||
    ["drafting", "awaiting_review", "executing"].includes(
      activeRun?.run.state ?? "",
    );

  useEffect(() => {
    if (!isLeader && composerMode === "task") setComposerMode("normal");
  }, [composerMode, isLeader]);

  useEffect(() => {
    if (!pendingTaskNavigation) return;
    const anchor = document.getElementById(
      teamTaskAnchor(pendingTaskNavigation),
    );
    if (!anchor) return;
    anchor.scrollIntoView?.({ block: "center", behavior: "smooth" });
    anchor.focus({ preventScroll: true });
    setPendingTaskNavigation(null);
  }, [activeMember?.id, pendingTaskNavigation]);

  const handleSelectMember = (memberId: string) => {
    onActiveMemberChange(memberId);
    session.markSeen(memberId);
    if (effectiveMode === "parallel") {
      const laneEl = laneRefs.current[memberId];
      if (laneEl) {
        laneEl.scrollIntoView?.({ behavior: "smooth", inline: "start" });
      }
    }
  };

  const handleFocusSingle = (memberId: string) => {
    onActiveMemberChange(memberId);
    session.markSeen(memberId);
    setMode("single");
  };

  const sendMessageForMember = async (targetMember: TeamMember) => {
    const draft = (drafts[targetMember.id] ?? "").trim();
    if (!draft) return;
    const targetSession = session.getMember(targetMember.id);
    const clientId = `optimistic-${targetMember.id}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const optimisticMessage: OptimisticUserMessage = {
      clientId,
      memberId: targetMember.id,
      message: draft,
      baselineExecutionId: targetSession?.execution_id ?? null,
      executionId: null,
      state: "sending",
      errorCode: null,
    };
    setOptimisticMessages((current) => [...current, optimisticMessage]);
    setDrafts((current) => ({ ...current, [targetMember.id]: "" }));

    try {
      const snapshot = await session.startTurn(targetMember.id, draft);
      setOptimisticMessages((current) =>
        current.map((item) =>
          item.clientId === clientId
            ? {
                ...item,
                executionId: snapshot.execution_id,
                state: snapshot.task.state === "Failed" ? "failed" : "accepted",
                errorCode: snapshot.task.error?.code ?? null,
              }
            : item,
        ),
      );
    } catch (error) {
      setOptimisticMessages((current) =>
        current.map((item) =>
          item.clientId === clientId
            ? { ...item, state: "failed", errorCode: errorCode(error) }
            : item,
        ),
      );
    }
  };

  const submitComposerForMember = async (targetMember: TeamMember) => {
    if (composerMode === "task" && targetMember.role === "leader") {
      const draft = (drafts[targetMember.id] ?? "").trim();
      if (!draft || taskModeBusy) return;
      setDrafts((current) => ({ ...current, [targetMember.id]: "" }));
      onStartTeamDraft(draft);
      return;
    }
    await sendMessageForMember(targetMember);
  };

  if (members.length === 0) {
    return (
      <Panel
        className="min-h-0 flex-1"
        data-testid="team-chat-shell"
        padding="none"
        variant="muted"
      >
        <EmptyState
          className="h-full border-0 bg-transparent shadow-none"
          description={t("team.chat.noEligibleDescription")}
          icon={<Bot size={22} />}
          title={t("team.chat.noEligibleTitle")}
        />
      </Panel>
    );
  }

  const navigateToTask = (taskId: string, ownerMemberId: string | null) => {
    if (ownerMemberId && ownerMemberId !== activeMember?.id) {
      onActiveMemberChange(ownerMemberId);
    }
    const taskElement = document.getElementById(`team-task-${taskId}`);
    taskElement?.scrollIntoView({ behavior: "smooth", block: "center" });
  };

  const renderLaneForMember = (member: TeamMember) => {
    const memberSession = session.getMember(member.id);
    const memberStatus = getMemberStatus(memberSession, t);
    const memberDraft = drafts[member.id] ?? "";
    const memberMessages = optimisticMessages.filter(
      (message) => message.memberId === member.id,
    );
    const memberBusy = isActiveTask(memberSession?.task);
    const memberSending = memberMessages.some((m) => m.state === "sending");
    const isMemberLeader = member.role === "leader";
    const memberDisabled =
      memberBusy ||
      memberSending ||
      (isMemberLeader && composerMode === "task" && taskModeBusy);
    const memberCanSend = Boolean(
      memberDraft.trim() &&
        !memberBusy &&
        !memberSending &&
        !(isMemberLeader && composerMode === "task" && taskModeBusy),
    );

    const memberRun =
      isMemberLeader && runSnapshot?.run.team_id === team.id ? runSnapshot : null;

    const projectedTasks: TeamTask[] =
      !isMemberLeader &&
      runSnapshot?.run.team_id === team.id &&
      ["executing", "terminal"].includes(runSnapshot.run.state)
        ? runSnapshot.tasks
            .filter(
              (task) =>
                task.owner_member_id === member.id && task.state !== "draft",
            )
            .sort(
              (left, right) =>
                left.sort_order - right.sort_order ||
                left.id.localeCompare(right.id),
            )
        : [];

    const timelineItems = composeTimelineItems(
      memberSession?.stream.items ?? [],
      memberMessages,
      memberSession?.execution_id,
    );

    const timelineKey = createTimelineKey(
      timelineItems,
      projectedTasks,
      memberRun,
      memberSession?.restore_state,
    );

    const composerExtra = isMemberLeader ? (
      <div
        aria-label={t("team.chat.composerMode")}
        className="mb-2 flex items-center gap-1 rounded-lg border border-theme-control-border/60 bg-theme-control/30 p-1"
        role="group"
      >
        <Button
          aria-pressed={composerMode === "normal"}
          onClick={() => setComposerMode("normal")}
          size="sm"
          type="button"
          variant={composerMode === "normal" ? "secondary" : "ghost"}
        >
          <MessageSquare size={14} />
          {t("team.chat.mode.normal")}
        </Button>
        <Button
          aria-pressed={composerMode === "task"}
          disabled={taskModeBusy}
          onClick={() => setComposerMode("task")}
          size="sm"
          type="button"
          variant={composerMode === "task" ? "secondary" : "ghost"}
        >
          <Sparkles size={14} />
          {t("team.chat.mode.task")}
        </Button>
      </div>
    ) : null;

    const placeholder =
      isMemberLeader && composerMode === "task"
        ? t("team.chat.taskPlaceholder")
        : t("team.chat.composerPlaceholder", {
            name: roleLabel(member, t),
          });

    const submitLabel =
      isMemberLeader && composerMode === "task"
        ? t("team.workflow.draft")
        : t("team.chat.send");

    return (
      <div
        key={member.id}
        ref={(el) => {
          laneRefs.current[member.id] = el;
        }}
        className={
          effectiveMode === "single"
            ? "flex h-full min-h-0 flex-1 overflow-hidden"
            : undefined
        }
      >
        <TeamMemberLane
          activeRun={memberRun}
          allMembers={members}
          canSend={memberCanSend}
          composerExtra={composerExtra}
          disabled={memberDisabled}
          draft={memberDraft}
          isActive={member.id === activeMember?.id}
          isExecuting={isActiveTask(memberSession?.task)}
          isLeader={isMemberLeader}
          member={member}
          onCancelRun={onCancel}
          onConfirmRun={onConfirm}
          onDraftChange={(value) =>
            setDrafts((current) => ({
              ...current,
              [member.id]: value,
            }))
          }
          onFocusSingle={handleFocusSingle}
          onMoveTask={onMoveTask}
          onReview={onReview}
          onSend={() => submitComposerForMember(member)}
          onStop={
            memberSession?.execution_id
              ? async () => {
                  const executionId = memberSession.execution_id;
                  if (!executionId) return;
                  await session.cancelTurn(member.id, executionId);
                }
              : undefined
          }
          onTaskChange={onTaskChange}
          onTaskNavigate={navigateToTask}
          placeholder={placeholder}
          projectedTasks={projectedTasks}
          roleLabelText={roleLabel(member, t)}
          sending={memberSending}
          sessionProjection={memberSession}
          status={memberStatus}
          submitLabel={submitLabel}
          team={team}
          timelineItems={timelineItems}
          timelineKey={timelineKey}
          totalMembers={members.length}
          viewMode={effectiveMode}
          workflowBusy={workflowBusy}
          workflowError={workflowError}
        />
      </div>
    );
  };

  return (
    <Panel
      className="min-h-0 flex-1 overflow-hidden"
      data-testid="team-chat-shell"
      padding="none"
    >
      <TeamWorkspaceHeader
        onDelete={onDelete}
        onEdit={onEdit}
        onOpenDetails={onOpenDetails}
        onViewModeChange={setMode}
        team={team}
        viewMode={effectiveMode}
      />

      <div className="flex min-h-0 flex-1 flex-col">
        <section
          aria-label={t("team.chat.members")}
          className="shrink-0 border-b border-theme-card-border/55 bg-theme-control/20 px-3 py-3 sm:px-5"
        >
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="text-label-caps uppercase text-on-surface-variant">
              {t("team.chat.members")}
            </h3>
            <span className="text-caption text-on-surface-variant">
              {t("team.list.count", { count: members.length })}
            </span>
          </div>
          <TeamMemberTabs
            activeMemberId={activeMember?.id ?? null}
            getMemberProjection={(id) => session.getMember(id)}
            getMemberStatus={getMemberStatus}
            members={members}
            onSelectMember={handleSelectMember}
            roleLabel={roleLabel}
          />
        </section>

        <div className="min-h-0 flex-1 overflow-hidden">
          {effectiveMode === "parallel" ? (
            <div
              className="flex h-full min-h-0 flex-1 overflow-x-auto snap-x snap-mandatory"
              data-testid="team-parallel-lanes"
            >
              {members.map((member) => renderLaneForMember(member))}
            </div>
          ) : (
            <div
              className="flex h-full min-h-0 flex-1 overflow-hidden"
              data-testid="team-single-lane"
            >
              {activeMember ? renderLaneForMember(activeMember) : null}
            </div>
          )}
        </div>
      </div>
    </Panel>
  );
}

function composeTimelineItems(
  streamItems: SessionItemSnapshot[],
  localMessages: OptimisticUserMessage[],
  currentExecutionId: string | null | undefined,
): SessionItemSnapshot[] {
  const items = new Map(
    streamItems.map((item) => [sessionItemKey(item), item]),
  );
  const claimedServerItems = new Set<string>();

  for (const message of localMessages) {
    const executionId =
      message.executionId ??
      (currentExecutionId && currentExecutionId !== message.baselineExecutionId
        ? currentExecutionId
        : null);
    const serverItem = executionId
      ? streamItems.find(
          (item) =>
            item.kind === "user_message" &&
            item.identity.execution_id === executionId &&
            !claimedServerItems.has(sessionItemKey(item)),
        )
      : undefined;

    if (serverItem) {
      const key = sessionItemKey(serverItem);
      claimedServerItems.add(key);
      items.set(key, {
        ...serverItem,
        text: message.message,
        code: message.errorCode ?? serverItem.code,
        state: message.state === "failed" ? "failed" : serverItem.state,
      });
      continue;
    }

    const optimisticItem = optimisticUserItem(message, executionId);
    items.set(sessionItemKey(optimisticItem), optimisticItem);
  }

  return [...items.values()].sort(
    (left, right) =>
      left.sequence - right.sequence ||
      sessionItemKey(left).localeCompare(sessionItemKey(right)),
  );
}

function optimisticUserItem(
  message: OptimisticUserMessage,
  executionId: string | null,
): SessionItemSnapshot {
  return {
    identity: {
      session_id: `optimistic-${message.memberId}`,
      member_id: message.memberId,
      execution_id: executionId ?? "optimistic-exec",
      turn_id: `optimistic-turn-${message.clientId}`,
      item_id: message.clientId,
    },
    kind: "user_message",
    sequence: Number.MAX_SAFE_INTEGER - 1,
    delivery: "live",
    state: message.state === "failed" ? "failed" : "completed",
    text: message.message,
    status: null,
    code: message.errorCode,
  };
}

function sessionItemKey(item: SessionItemSnapshot): string {
  return `${item.identity.execution_id ?? "exec"}:${item.identity.item_id}`;
}

function isActiveTask(task: TeamMemberSessionProjection["task"] | undefined): boolean {
  return task?.state === "Running" || task?.state === "Pending" || task?.state === "Cancelling";
}

function getMemberStatus(
  projection: TeamMemberSessionProjection | null,
  t: ReturnType<typeof useI18n>["t"],
): { label: string; icon: React.ReactNode; className: string } {
  if (!projection) {
    return {
      label: t("team.chat.status.notStarted"),
      icon: <Clock3 size={13} />,
      className: "text-on-surface-variant",
    };
  }

  if (
    projection.restore_state === "unavailable" ||
    projection.task?.state === "Failed"
  ) {
    return {
      label: t("team.chat.status.unavailable"),
      icon: <CircleAlert size={13} />,
      className: "text-status-remove",
    };
  }

  if (projection.restore_state === "partial") {
    return {
      label: t("team.chat.status.partial"),
      icon: <CircleAlert size={13} />,
      className: "text-status-conflict",
    };
  }

  if (
    projection.restore_state === "restoring" ||
    isActiveTask(projection.task)
  ) {
    return {
      label: t("team.chat.status.working"),
      icon: <LoaderCircle className="animate-spin" size={13} />,
      className: "text-primary",
    };
  }

  if (projection.unread) {
    return {
      label: t("team.chat.status.unread"),
      icon: <Activity size={13} />,
      className: "text-primary",
    };
  }

  return {
    label: t("team.chat.status.ready"),
    icon: <CheckCircle2 size={13} />,
    className: "text-status-create",
  };
}

function roleLabel(
  member: TeamMember,
  t: ReturnType<typeof useI18n>["t"],
): string {
  return member.role === "leader"
    ? t("team.detail.leader")
    : t("team.detail.teammate");
}

function createTimelineKey(
  items: SessionItemSnapshot[],
  projectedTasks: TeamTask[],
  run: TeamRunSnapshot | null,
  restoreState: TeamMemberRestoreState | undefined,
): string {
  return JSON.stringify({
    items: items.map((item) => [
      sessionItemKey(item),
      item.sequence,
      item.state,
      item.text,
      item.code,
    ]),
    projectedTasks: projectedTasks.map((task) => [
      task.id,
      task.state,
      task.result,
      task.error_code,
    ]),
    run: run
      ? [
          run.run.id,
          run.run.revision,
          run.run.state,
          ...run.tasks.map((task) => [
            task.id,
            task.state,
            task.result,
            task.error_code,
          ]),
        ]
      : null,
    restoreState,
  });
}

function errorCode(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof (error as { code: unknown }).code === "string"
  ) {
    return (error as { code: string }).code;
  }
  if (error instanceof Error && error.message.trim()) return error.message;
  return "UNKNOWN";
}
