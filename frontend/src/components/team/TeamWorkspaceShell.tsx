import {
  Activity,
  Bot,
  CheckCircle2,
  CircleAlert,
  Clock3,
  FileText,
  LoaderCircle,
  MessageSquare,
  MoreHorizontal,
  Send,
  Settings2,
  Shield,
  Sparkles,
  Wrench,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { Button } from "../ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import { useTeamSession } from "../../app/backgroundTasks/TeamSessionProvider";
import type {
  SessionItemKind,
  SessionItemSnapshot,
  TeamDetail,
  TeamMember,
  TeamMemberRestoreState,
  TeamMemberSessionProjection,
} from "../../types/team";

export interface TeamWorkspaceShellProps {
  team: TeamDetail;
  onOpenDetails: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onOpenWorkflow: () => void;
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
  onDelete,
  onEdit,
  onOpenDetails,
  onOpenWorkflow,
  team,
}: TeamWorkspaceShellProps) {
  const { t } = useI18n();
  const session = useTeamSession();
  const members = useMemo(
    () => [...team.members].sort((left, right) => left.sort_order - right.sort_order),
    [team.members],
  );
  const leader = members.find((member) => member.role === "leader") ?? members[0] ?? null;
  const [activeMemberId, setActiveMemberId] = useState<string | null>(leader?.id ?? null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [optimisticMessages, setOptimisticMessages] = useState<OptimisticUserMessage[]>([]);

  useEffect(() => {
    setActiveMemberId((current) => (
      current && members.some((member) => member.id === current)
        ? current
        : leader?.id ?? members[0]?.id ?? null
    ));
  }, [leader?.id, members, team.id]);

  const activeMember = members.find((member) => member.id === activeMemberId) ?? leader;
  const activeSession = activeMember ? session.getMember(activeMember.id) : null;
  const activeStatus = getMemberStatus(activeSession, t);
  const activeDraft = activeMember ? drafts[activeMember.id] ?? "" : "";
  const activeMessages = activeMember
    ? optimisticMessages.filter((message) => message.memberId === activeMember.id)
    : [];
  const activeMemberBusy = isActiveTask(activeSession?.task);
  const activeMemberSending = activeMessages.some((message) => message.state === "sending");
  const canSend = Boolean(activeMember && activeDraft.trim() && !activeMemberBusy && !activeMemberSending);

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
      setOptimisticMessages((current) => current.map((item) => (
        item.clientId === clientId
          ? {
            ...item,
            executionId: snapshot.execution_id,
            state: snapshot.task.state === "Failed" ? "failed" : "accepted",
            errorCode: snapshot.task.error?.code ?? null,
          }
          : item
      )));
    } catch (error) {
      setOptimisticMessages((current) => current.map((item) => (
        item.clientId === clientId
          ? { ...item, state: "failed", errorCode: errorCode(error) }
          : item
      )));
    }
  };

  if (members.length === 0) {
    return (
      <Panel className="min-h-0 flex-1" data-testid="team-chat-shell" padding="none" variant="muted">
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
    <Panel className="min-h-0 flex-1 overflow-hidden" data-testid="team-chat-shell" padding="none">
      <header className="flex shrink-0 flex-wrap items-start justify-between gap-3 border-b border-theme-card-border/65 bg-theme-card-header/55 px-4 py-3 sm:px-5">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-theme-nav-active-border/50 bg-theme-nav-active/20 text-primary">
            <MessageSquare size={19} />
          </span>
          <div className="min-w-0">
            <p className="text-label-caps uppercase text-status-update">{t("team.chat.eyebrow")}</p>
            <h2 className="truncate text-title-lg font-bold text-on-surface">{team.name}</h2>
            <p className="mt-0.5 truncate text-body-sm text-on-surface-variant">
              {team.description || t("team.chat.description")}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button onClick={onOpenDetails} size="sm" type="button" variant="ghost">
            <Settings2 size={14} />
            {t("team.chat.details")}
          </Button>
          <Button onClick={onOpenWorkflow} size="sm" type="button" variant="outline">
            <Sparkles size={14} />
            {t("team.chat.workflow")}
          </Button>
          <Button onClick={onEdit} size="sm" type="button" variant="outline">
            {t("team.action.edit")}
          </Button>
          <Button aria-label={t("team.action.delete")} onClick={onDelete} size="icon-sm" type="button" variant="destructive">
            <XCircle size={15} />
          </Button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col">
        <section aria-label={t("team.chat.members")} className="shrink-0 border-b border-theme-card-border/55 bg-theme-control/20 px-3 py-3 sm:px-5">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="text-label-caps uppercase text-on-surface-variant">{t("team.chat.members")}</h3>
            <span className="text-caption text-on-surface-variant">{t("team.list.count", { count: members.length })}</span>
          </div>
          <div aria-label={t("team.chat.memberNavigation")} className="flex min-w-0 gap-2 overflow-x-auto pb-1" role="tablist">
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
                  key={member.id}
                  onClick={() => {
                    setActiveMemberId(member.id);
                    session.markSeen(member.id);
                  }}
                  role="tab"
                  type="button"
                >
                  <span className={`grid size-9 shrink-0 place-items-center rounded-full border text-caption font-bold ${selected ? "border-primary/50 bg-theme-nav-active text-theme-nav-active-fg" : "border-theme-control-border bg-theme-control text-on-surface-variant"}`}>
                    {memberInitials(member)}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className="truncate text-body-sm font-semibold text-on-surface">{roleLabel(member, t)}</span>
                      {member.role === "leader" ? <Shield aria-label={t("team.chat.leaderBadge")} className="shrink-0 text-primary" size={13} /> : null}
                    </span>
                    <span className="block truncate text-caption text-on-surface-variant">{member.agent_id} · {member.model || t("team.detail.defaultModel")}</span>
                    <span className={`mt-0.5 flex items-center gap-1 text-caption ${status.className}`} data-testid={`team-member-${member.id}-status`}>
                      {status.icon}
                      {status.label}
                      {projection?.unread ? <span className="font-semibold">· {t("team.chat.unread")}</span> : null}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </section>

        <section className="flex min-h-0 flex-1 flex-col bg-theme-panel/25" aria-label={t("team.chat.sessionArea")}>
          <div className="flex shrink-0 items-center justify-between gap-3 border-b border-theme-card-border/45 px-4 py-2.5 sm:px-5">
            <div className="min-w-0">
              <p className="text-label-caps uppercase text-on-surface-variant">{t("team.chat.activeSession")}</p>
              <h3 className="truncate text-title-sm font-bold text-on-surface" data-testid="team-active-recipient">
                {activeMember ? roleLabel(activeMember, t) : t("team.chat.noRecipient")}
              </h3>
            </div>
            <div className={`flex shrink-0 items-center gap-1.5 text-caption ${activeStatus.className}`}>
              {activeStatus.icon}
              <span>{activeStatus.label}</span>
            </div>
          </div>

          <div
            aria-label={activeMember ? t("team.chat.timelineLabel", { name: roleLabel(activeMember, t) }) : t("team.chat.sessionArea")}
            className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-5"
            data-testid="team-timeline"
            id="team-session-timeline"
            role="log"
            tabIndex={0}
          >
            {activeSession?.stream.items.length || activeMessages.length ? (
              <ol className="mx-auto grid w-full max-w-3xl gap-3">
                {composeTimelineItems(activeSession?.stream.items ?? [], activeMessages, activeSession?.execution_id).map((item) => (
                  <SessionItem item={item} key={`${item.identity.execution_id}:${item.identity.item_id}`} />
                ))}
              </ol>
            ) : (
              <EmptyState
                className="min-h-56 border-0 bg-transparent shadow-none"
                description={t("team.chat.emptyDescription", { name: activeMember ? roleLabel(activeMember, t) : "" })}
                icon={<MessageSquare size={21} />}
                title={t("team.chat.emptyTitle")}
              />
            )}
          </div>

          <section aria-label={t("team.chat.composerLabel")} className="sticky bottom-0 shrink-0 border-t border-theme-card-border/65 bg-theme-card-header/90 px-4 py-3 shadow-[0_-10px_24px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur sm:px-5" data-testid="team-composer">
            <div className="mb-2 flex flex-wrap items-center justify-between gap-2 text-caption">
              <span className="text-on-surface-variant">{t("team.chat.recipient")}</span>
              <span className="font-semibold text-primary">{activeMember ? roleLabel(activeMember, t) : t("team.chat.noRecipient")}</span>
              <span className="ml-auto text-on-surface-variant">
                {activeMemberSending || activeMemberBusy ? t("team.chat.status.working") : t("team.chat.composerPending")}
              </span>
            </div>
            <form
              className="flex items-end gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                void sendMessage();
              }}
            >
              <textarea
                aria-label={t("team.chat.composerInput")}
                className="min-h-16 min-w-0 flex-1 resize-none rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 py-2.5 text-body-sm text-on-surface shadow-[var(--theme-shadow-control-inset)] outline-none placeholder:text-outline focus:border-primary-strong/65 focus:ring-2 focus:ring-primary-strong/25 disabled:cursor-not-allowed disabled:opacity-70"
                disabled={!activeMember || activeMemberBusy || activeMemberSending}
                placeholder={activeMember ? t("team.chat.composerPlaceholder", { name: roleLabel(activeMember, t) }) : t("team.chat.composerPlaceholderFallback")}
                rows={2}
                value={activeDraft}
                onChange={(event) => {
                  if (!activeMember) return;
                  setDrafts((current) => ({ ...current, [activeMember.id]: event.target.value }));
                }}
              />
              <Button aria-label={t("team.chat.send")} disabled={!canSend} size="sm" type="submit">
                <Send size={14} />
                {t("team.chat.send")}
              </Button>
            </form>
          </section>
        </section>
      </div>
    </Panel>
  );
}

function composeTimelineItems(
  streamItems: SessionItemSnapshot[],
  localMessages: OptimisticUserMessage[],
  currentExecutionId: string | null | undefined,
): SessionItemSnapshot[] {
  const items = new Map(streamItems.map((item) => [sessionItemKey(item), item]));
  const claimedServerItems = new Set<string>();

  for (const message of localMessages) {
    const executionId = message.executionId
      ?? (currentExecutionId && currentExecutionId !== message.baselineExecutionId
        ? currentExecutionId
        : null);
    const serverItem = executionId
      ? streamItems.find((item) => (
        item.kind === "user_message"
        && item.identity.execution_id === executionId
        && !claimedServerItems.has(sessionItemKey(item))
      ))
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

  return [...items.values()].sort((left, right) => (
    left.sequence - right.sequence || sessionItemKey(left).localeCompare(sessionItemKey(right))
  ));
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
    state: message.state === "sending"
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

function isActiveTask(task: TeamMemberSessionProjection["task"] | undefined | null): boolean {
  return task?.state === "Pending" || task?.state === "Running" || task?.state === "Cancelling";
}

function errorCode(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  return "member_turn_failed";
}

function SessionItem({ item }: { item: SessionItemSnapshot }) {
  const { t } = useI18n();
  const isUser = item.kind === "user_message";
  const label = itemLabel(item.kind, t);
  const icon = itemIcon(item.kind);
  const detail = item.text || itemStatus(item, t);
  const tone = item.kind === "error" || item.state === "failed"
    ? "border-status-remove/35 bg-status-remove/10"
    : isUser
      ? "border-theme-nav-active-border/35 bg-theme-nav-active/10"
      : "border-theme-card-border/65 bg-theme-card/55";
  return (
    <li className={`rounded-xl border px-3.5 py-3 ${tone}`}>
      <div className="flex items-start gap-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border/70 bg-theme-control/70 text-primary">{icon}</span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-label-caps uppercase text-on-surface-variant">{label}</span>
            <span className="text-caption text-outline">{item.delivery === "replay" ? t("team.chat.replay") : t("team.chat.live")}</span>
            <span className="ml-auto text-caption text-outline">{item.state}</span>
          </div>
          <p className="mt-1 whitespace-pre-wrap break-words text-body-sm text-on-surface">{detail}</p>
          {item.code && item.text ? <p className="mt-1 text-caption text-status-remove">{item.code}</p> : null}
        </div>
      </div>
    </li>
  );
}

function getMemberStatus(
  projection: TeamMemberSessionProjection | null,
  t: ReturnType<typeof useI18n>["t"],
) {
  if (!projection) {
    return { className: "text-on-surface-variant", icon: <CircleAlert size={12} />, label: t("team.chat.status.notStarted") };
  }
  if (projection.restore_state === "restoring" || ["Pending", "Running", "Cancelling"].includes(projection.task?.state ?? "")) {
    return { className: "text-status-update", icon: <LoaderCircle className="animate-spin" size={12} />, label: t("team.chat.status.working") };
  }
  if (projection.restore_state === "unavailable" || projection.task?.state === "Failed") {
    return { className: "text-status-remove", icon: <CircleAlert size={12} />, label: t("team.chat.status.unavailable") };
  }
  if (projection.restore_state === "partial") {
    return { className: "text-status-conflict", icon: <Clock3 size={12} />, label: t("team.chat.status.partial") };
  }
  if (projection.unread) {
    return { className: "text-status-create", icon: <Activity size={12} />, label: t("team.chat.status.unread") };
  }
  return { className: "text-status-create", icon: <CheckCircle2 size={12} />, label: t("team.chat.status.ready") };
}

function roleLabel(member: TeamMember, t: ReturnType<typeof useI18n>["t"]) {
  return member.role === "leader" ? t("team.detail.leader") : t("team.detail.teammate");
}

function memberInitials(member: TeamMember) {
  const source = member.agent_id.trim() || member.role;
  return source.slice(0, 2).toUpperCase();
}

function itemLabel(kind: SessionItemKind, t: ReturnType<typeof useI18n>["t"]) {
  switch (kind) {
    case "user_message": return t("team.chat.item.user");
    case "assistant_text": return t("team.chat.item.assistant");
    case "processing": return t("team.chat.item.processing");
    case "thinking": return t("team.chat.item.thinking");
    case "tool": return t("team.chat.item.tool");
    case "task": return t("team.chat.item.task");
    case "notice": return t("team.chat.item.notice");
    case "final_result": return t("team.chat.item.result");
    case "cancelled": return t("team.chat.item.cancelled");
    case "error": return t("team.chat.item.error");
  }
}

function itemStatus(item: SessionItemSnapshot, t: ReturnType<typeof useI18n>["t"]) {
  if (item.kind === "processing") return t("team.chat.item.processingActive");
  if (item.kind === "tool") return t("team.chat.item.toolActivity");
  if (item.kind === "task") return t("team.chat.item.taskActivity");
  if (item.kind === "error") return item.code || t("team.chat.item.error");
  return t("team.chat.item.noText");
}

function itemIcon(kind: SessionItemKind) {
  switch (kind) {
    case "user_message": return <MessageSquare size={15} />;
    case "assistant_text": return <Sparkles size={15} />;
    case "processing": return <LoaderCircle size={15} />;
    case "thinking": return <MoreHorizontal size={15} />;
    case "tool": return <Wrench size={15} />;
    case "task": return <FileText size={15} />;
    case "notice": return <Activity size={15} />;
    case "final_result": return <CheckCircle2 size={15} />;
    case "cancelled": return <XCircle size={15} />;
    case "error": return <CircleAlert size={15} />;
    default: return <Bot size={15} />;
  }
}
