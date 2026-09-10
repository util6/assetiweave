import {
  Activity,
  Bot,
  CheckCircle2,
  CircleAlert,
  Clock3,
  LoaderCircle,
  MessageSquare,
  Settings2,
  Shield,
  Sparkles,
  XCircle,
} from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { TeamPlanCard } from "./TeamPlanCard";
import { TeamTaskCard, teamTaskAnchor } from "./TeamTaskCard";
import { Button } from "../ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import { useTeamSession } from "../../app/backgroundTasks/TeamSessionProvider";
import { AgentSessionWorkspace } from "../agent-session";
import { adaptTeamSessionToWorkspaceProps } from "./teamSessionAdapter";
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
  const members = useMemo(
    () =>
      [...team.members].sort(
        (left, right) => left.sort_order - right.sort_order,
      ),
    [team.members],
  );
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
  const memberButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const activeMember =
    members.find((member) => member.id === selectedMemberId) ?? leader;
  const activeSession = activeMember
    ? session.getMember(activeMember.id)
    : null;
  const activeStatus = getMemberStatus(activeSession, t);
  const activeDraft = activeMember ? (drafts[activeMember.id] ?? "") : "";
  const activeMessages = activeMember
    ? optimisticMessages.filter(
        (message) => message.memberId === activeMember.id,
      )
    : [];
  const activeMemberBusy = isActiveTask(activeSession?.task);
  const activeMemberSending = activeMessages.some(
    (message) => message.state === "sending",
  );
  const canSend = Boolean(
    activeMember &&
    activeDraft.trim() &&
    !activeMemberBusy &&
    !activeMemberSending,
  );
  const isLeader = activeMember?.role === "leader";
  const activeRun =
    isLeader && runSnapshot?.run.team_id === team.id ? runSnapshot : null;
  const activeProjectedTasks = useMemo(() => {
    if (
      !activeMember ||
      activeMember.role === "leader" ||
      runSnapshot?.run.team_id !== team.id ||
      !["executing", "terminal"].includes(runSnapshot.run.state)
    )
      return [];
    return runSnapshot.tasks
      .filter(
        (task) =>
          task.owner_member_id === activeMember.id && task.state !== "draft",
      )
      .sort(
        (left, right) =>
          left.sort_order - right.sort_order || left.id.localeCompare(right.id),
      );
  }, [activeMember, runSnapshot, team.id]);
  const taskModeBusy =
    workflowBusy ||
    ["drafting", "awaiting_review", "executing"].includes(
      activeRun?.run.state ?? "",
    );
  const activeTimelineItems = useMemo(
    () =>
      composeTimelineItems(
        activeSession?.stream.items ?? [],
        activeMessages,
        activeSession?.execution_id,
      ),
    [activeMessages, activeSession?.execution_id, activeSession?.stream.items],
  );
  const timelineKey = useMemo(
    () =>
      createTimelineKey(
        activeTimelineItems,
        activeProjectedTasks,
        activeRun,
        activeSession?.restore_state,
      ),
    [
      activeProjectedTasks,
      activeRun,
      activeSession?.restore_state,
      activeTimelineItems,
    ],
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
  }, [activeMember?.id, activeProjectedTasks, pendingTaskNavigation]);

  const navigateToTask = (taskId: string, ownerMemberId: string | null) => {
    const owner = members.find(
      (member) => member.id === ownerMemberId && member.role === "teammate",
    );
    if (!owner) return;
    setPendingTaskNavigation(taskId);
    onActiveMemberChange(owner.id);
    session.markSeen(owner.id);
  };

  const sendMessage = async () => {
    if (!activeMember || !canSend) return;
    const message = activeDraft.trim();
    const clientId = `optimistic-${activeMember.id}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const optimisticMessage: OptimisticUserMessage = {
      clientId,
      memberId: activeMember.id,
      message,
      baselineExecutionId: activeSession?.execution_id ?? null,
      executionId: null,
      state: "sending",
      errorCode: null,
    };
    setOptimisticMessages((current) => [...current, optimisticMessage]);
    setDrafts((current) => ({ ...current, [activeMember.id]: "" }));

    try {
      const snapshot = await session.startTurn(activeMember.id, message);
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

  const submitComposer = async () => {
    if (composerMode === "task" && isLeader && activeMember) {
      if (!activeDraft.trim() || taskModeBusy) return;
      const message = activeDraft.trim();
      setDrafts((current) => ({ ...current, [activeMember.id]: "" }));
      onStartTeamDraft(message);
      return;
    }
    await sendMessage();
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

  return (
    <Panel
      className="min-h-0 flex-1 overflow-hidden"
      data-testid="team-chat-shell"
      padding="none"
    >
      <header className="flex shrink-0 flex-wrap items-start justify-between gap-3 border-b border-theme-card-border/65 bg-theme-card-header/55 px-4 py-3 sm:px-5">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-theme-nav-active-border/50 bg-theme-nav-active/20 text-primary">
            <MessageSquare size={19} />
          </span>
          <div className="min-w-0">
            <p className="text-label-caps uppercase text-status-update">
              {t("team.chat.eyebrow")}
            </p>
            <h2 className="truncate text-title-lg font-bold text-on-surface">
              {team.name}
            </h2>
            <p className="mt-0.5 truncate text-body-sm text-on-surface-variant">
              {team.description || t("team.chat.description")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button
            onClick={onOpenDetails}
            size="sm"
            type="button"
            variant="ghost"
          >
            <Settings2 size={14} />
            {t("team.chat.details")}
          </Button>
          <Button onClick={onEdit} size="sm" type="button" variant="outline">
            {t("team.action.edit")}
          </Button>
          <Button
            aria-label={t("team.action.delete")}
            onClick={onDelete}
            size="icon-sm"
            type="button"
            variant="destructive"
          >
            <XCircle size={15} />
          </Button>
        </div>
      </header>

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
          <div
            aria-label={t("team.chat.memberNavigation")}
            className="flex min-w-0 gap-2 overflow-x-auto pb-1"
            role="tablist"
          >
            {members.map((member) => {
              const projection = session.getMember(member.id);
              const status = getMemberStatus(projection, t);
              const selected = member.id === activeMember?.id;
              return (
                <button
                  aria-controls="team-session-timeline"
                  aria-label={`${roleLabel(member, t)} · ${member.agent_id}`}
                  aria-selected={selected}
                  className={`group flex min-w-44 shrink-0 items-center gap-2 rounded-xl border px-2.5 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 ${selected ? "border-theme-nav-active-border bg-theme-nav-active/20" : "border-theme-card-border/65 bg-theme-card/40 hover:border-theme-nav-active-border/55 hover:bg-theme-control-hover/60"}`}
                  data-testid={`team-member-${member.id}`}
                  id={`team-member-tab-${member.id}`}
                  key={member.id}
                  onClick={() => {
                    setPendingTaskNavigation(null);
                    onActiveMemberChange(member.id);
                    session.markSeen(member.id);
                  }}
                  onKeyDown={(event) => {
                    const nextIndex = memberNavigationIndex(
                      event.key,
                      members.length,
                      members.indexOf(member),
                    );
                    if (nextIndex === null) return;
                    event.preventDefault();
                    const nextMember = members[nextIndex];
                    onActiveMemberChange(nextMember.id);
                    session.markSeen(nextMember.id);
                    memberButtonRefs.current[nextMember.id]?.focus();
                  }}
                  ref={(element) => {
                    memberButtonRefs.current[member.id] = element;
                  }}
                  role="tab"
                  tabIndex={selected ? 0 : -1}
                  type="button"
                >
                  <span
                    className={`grid size-9 shrink-0 place-items-center rounded-full border text-caption font-bold ${selected ? "border-primary/50 bg-theme-nav-active text-theme-nav-active-fg" : "border-theme-control-border bg-theme-control text-on-surface-variant"}`}
                  >
                    {memberInitials(member)}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className="truncate text-body-sm font-semibold text-on-surface">
                        {roleLabel(member, t)}
                      </span>
                      {member.role === "leader" ? (
                        <Shield
                          aria-label={t("team.chat.leaderBadge")}
                          className="shrink-0 text-primary"
                          size={13}
                        />
                      ) : null}
                    </span>
                    <span className="block truncate text-caption text-on-surface-variant">
                      {member.agent_id} ·{" "}
                      {member.model || t("team.detail.defaultModel")}
                    </span>
                    <span
                      className={`mt-0.5 flex items-center gap-1 text-caption ${status.className}`}
                      data-testid={`team-member-${member.id}-status`}
                    >
                      {status.icon}
                      {status.label}
                      {projection?.unread ? (
                        <span className="font-semibold">
                          · {t("team.chat.unread")}
                        </span>
                      ) : null}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </section>

        {activeMember ? (
          <AgentSessionWorkspace
            {...adaptTeamSessionToWorkspaceProps({
              activeMember,
              activeSession,
              activeTimelineItems,
              activityDependencyKey: timelineKey,
              canSend,
              composerExtra: isLeader ? (
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
              ) : null,
              disabled:
                !activeMember ||
                activeMemberBusy ||
                activeMemberSending ||
                (composerMode === "task" && taskModeBusy),
              draft: activeDraft,
              isLeader,
              onDraftChange: (value) => {
                if (!activeMember) return;
                setDrafts((current) => ({
                  ...current,
                  [activeMember.id]: value,
                }));
              },
              onSend: submitComposer,
              placeholder:
                composerMode === "task"
                  ? t("team.chat.taskPlaceholder")
                  : activeMember
                    ? t("team.chat.composerPlaceholder", {
                        name: roleLabel(activeMember, t),
                      })
                    : t("team.chat.composerPlaceholderFallback"),
              submitLabel:
                composerMode === "task"
                  ? t("team.workflow.draft")
                  : t("team.chat.send"),
              restoreStatus:
                activeSession?.restore_state &&
                activeSession.restore_state !== "ready" &&
                activeSession.restore_state !== "not-started"
                  ? {
                      errorCode: activeSession.restore_error_code,
                      icon: activeStatus.icon,
                      label: activeStatus.label,
                      state: activeSession.restore_state,
                      className:
                        activeStatus.className === "text-status-remove"
                          ? "border-status-remove/35 bg-status-remove/10 text-status-remove"
                          : "border-theme-nav-active-border/40 bg-theme-nav-active/10 text-on-surface-variant",
                    }
                  : null,
              roleLabelText: activeMember
                ? roleLabel(activeMember, t)
                : t("team.chat.noRecipient"),
              sessionResetKey: activeMember?.id,
              status: activeStatus,
              testIdPrefix: "team",
              timelineExtra:
                activeProjectedTasks.length || activeRun ? (
                  <>
                    {activeProjectedTasks.map((task) => (
                      <TeamTaskCard
                        key={task.id}
                        owner={members.find(
                          (member) => member.id === task.owner_member_id,
                        )}
                        task={task}
                      />
                    ))}
                    {activeRun ? (
                      <TeamPlanCard
                        busy={workflowBusy}
                        error={workflowError}
                        onCancel={onCancel}
                        onConfirm={onConfirm}
                        onMoveTask={onMoveTask}
                        onReview={onReview}
                        onTaskChange={onTaskChange}
                        onTaskNavigate={navigateToTask}
                        snapshot={activeRun}
                        team={team}
                      />
                    ) : null}
                  </>
                ) : undefined,
            })}
          />
        ) : null}
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

export function isNearTimelineBottom(
  element: Pick<HTMLElement, "clientHeight" | "scrollHeight" | "scrollTop">,
  threshold = 64,
): boolean {
  return (
    element.scrollHeight - element.scrollTop - element.clientHeight <= threshold
  );
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

function memberNavigationIndex(
  key: string,
  length: number,
  currentIndex: number,
): number | null {
  if (length === 0) return null;
  if (
    ![
      "ArrowDown",
      "ArrowLeft",
      "ArrowRight",
      "ArrowUp",
      "End",
      "Home",
    ].includes(key)
  )
    return null;
  if (key === "Home") return 0;
  if (key === "End") return length - 1;
  const direction = key === "ArrowLeft" || key === "ArrowUp" ? -1 : 1;
  return (currentIndex + direction + length) % length;
}

function optimisticUserItem(
  message: OptimisticUserMessage,
  executionId: string | null,
): SessionItemSnapshot {
  const itemExecutionId = executionId ?? `optimistic:${message.clientId}`;
  return {
    identity: {
      session_id: `optimistic:${message.memberId}`,
      member_id: message.memberId,
      execution_id: itemExecutionId,
      turn_id: itemExecutionId,
      item_id: `user:${message.clientId}`,
    },
    kind: "user_message",
    sequence: 0,
    delivery: "live",
    state:
      message.state === "sending"
        ? "pending"
        : message.state === "failed"
          ? "failed"
          : "completed",
    text: message.message,
    status: null,
    code: message.errorCode,
  };
}

function sessionItemKey(item: SessionItemSnapshot): string {
  const { identity } = item;
  return [
    identity.session_id,
    identity.member_id,
    identity.execution_id,
    identity.turn_id,
    identity.item_id,
  ].join("\u0000");
}

function isActiveTask(
  task: TeamMemberSessionProjection["task"] | undefined | null,
): boolean {
  return (
    task?.state === "Pending" ||
    task?.state === "Running" ||
    task?.state === "Cancelling"
  );
}

function errorCode(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  return "member_turn_failed";
}

function getMemberStatus(
  projection: TeamMemberSessionProjection | null,
  t: ReturnType<typeof useI18n>["t"],
) {
  if (!projection) {
    return {
      className: "text-on-surface-variant",
      icon: <CircleAlert size={12} />,
      label: t("team.chat.status.notStarted"),
    };
  }
  if (
    projection.restore_state === "restoring" ||
    ["Pending", "Running", "Cancelling"].includes(projection.task?.state ?? "")
  ) {
    return {
      className: "text-status-update",
      icon: <LoaderCircle className="animate-spin" size={12} />,
      label: t("team.chat.status.working"),
    };
  }
  if (
    projection.restore_state === "unavailable" ||
    projection.task?.state === "Failed"
  ) {
    return {
      className: "text-status-remove",
      icon: <CircleAlert size={12} />,
      label: t("team.chat.status.unavailable"),
    };
  }
  if (projection.restore_state === "partial") {
    return {
      className: "text-status-conflict",
      icon: <Clock3 size={12} />,
      label: t("team.chat.status.partial"),
    };
  }
  if (projection.unread) {
    return {
      className: "text-status-create",
      icon: <Activity size={12} />,
      label: t("team.chat.status.unread"),
    };
  }
  return {
    className: "text-status-create",
    icon: <CheckCircle2 size={12} />,
    label: t("team.chat.status.ready"),
  };
}

function roleLabel(member: TeamMember, t: ReturnType<typeof useI18n>["t"]) {
  return member.role === "leader"
    ? t("team.detail.leader")
    : t("team.detail.teammate");
}

function memberInitials(member: TeamMember) {
  const source = member.agent_id.trim() || member.role;
  return source.slice(0, 2).toUpperCase();
}
