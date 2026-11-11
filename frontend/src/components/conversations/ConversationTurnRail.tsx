import { ChevronRight } from "lucide-react";
import type {
  ConversationQuestionDetail,
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import type { Translator } from "../../i18n/I18nProvider";
import { Button } from "../ui/button";
import {
  groupSubagentPortalsByTurn,
  mainConversationTurns,
  type SubagentDispatchPortal,
} from "./conversationSubagents";

/**
 * 左栏问题卡片里的 Turn 轨：一问一轮。
 *
 * ADR-0022 之后子代理不再拆成物理轮次，而是父轮次里的委派锚点：
 * 轨道只在对应轮次下展示 `N 个子代理` 胶囊，展开后给出可点击的子代理芯片。
 */
export interface QuestionTurnEntry {
  turn: ConversationTurnRecord;
  /** 该轮在问题内的序号（从 1 开始）。 */
  ordinal: number;
  /** 该轮派发的子代理锚点。 */
  subagents: SubagentDispatchPortal[];
}

export function buildQuestionTurnRail(
  question: ConversationQuestionDetail,
): QuestionTurnEntry[] {
  const portalsByTurn = groupSubagentPortalsByTurn(question);
  return mainConversationTurns(question).map((turn, index) => ({
    ordinal: index + 1,
    subagents: portalsByTurn.get(turn.id) ?? [],
    turn,
  }));
}

export function QuestionTurnRail({
  entries,
  expandedRootIds,
  onToggleRoot,
  onSelectTurn,
  onOpenSubagent,
  selectedTurnId,
  t,
}: {
  entries: QuestionTurnEntry[];
  expandedRootIds: ReadonlySet<string>;
  onToggleRoot: (turnId: string) => void;
  onSelectTurn: (turnId: string) => void;
  onOpenSubagent: (
    portal: SubagentDispatchPortal,
    turn: ConversationTurnRecord,
  ) => void;
  selectedTurnId: string | null;
  t: Translator;
}) {
  return (
    <ul className="conversation-turn-rail mt-2 flex flex-col gap-1 border-l-2 border-primary/15 pl-2">
      {entries.map((entry) => {
        const expanded = expandedRootIds.has(entry.turn.id);
        return (
          <li key={entry.turn.id}>
            <div
              className={`flex min-w-0 items-center gap-2 rounded-xl px-2 py-1.5 transition-colors hover:bg-primary/[0.06] ${
                entry.turn.id === selectedTurnId ? "bg-primary/[0.09]" : ""
              }`}
            >
              <button
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
                data-turn-rail-id={entry.turn.id}
                onClick={() => onSelectTurn(entry.turn.id)}
                type="button"
              >
                <span className="shrink-0 font-mono text-code-sm text-on-surface-muted">
                  {entry.ordinal}
                </span>
                <span className="min-w-0 flex-1 truncate text-body-sm text-on-surface-variant">
                  {turnRailTitle(entry.turn)}
                </span>
              </button>
              {entry.subagents.length > 0 ? (
                <Button
                  aria-expanded={expanded}
                  aria-label={t("conversation.turn.railToggleSubagents", {
                    count: entry.subagents.length,
                  })}
                  className="h-6 shrink-0 gap-1 rounded-full border border-primary/25 bg-theme-control/70 px-2 text-code-sm text-primary hover:bg-primary/10"
                  onClick={() => onToggleRoot(entry.turn.id)}
                  size="sm"
                  title={t("conversation.turn.childCount", {
                    count: entry.subagents.length,
                  })}
                  type="button"
                  variant="ghost"
                >
                  {t("conversation.turn.childCount", {
                    count: entry.subagents.length,
                  })}
                  <ChevronRight
                    className={`size-3 transition-transform ${
                      expanded ? "rotate-90" : ""
                    }`}
                  />
                </Button>
              ) : null}
            </div>
            {entry.subagents.length > 0 && expanded ? (
              <SubagentChipList
                onOpen={(portal) => onOpenSubagent(portal, entry.turn)}
                portals={entry.subagents}
                t={t}
              />
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

/** 子代理芯片列表：左栏轮次行与单轮问题卡片直接复用同一套呈现。 */
export function SubagentChipList({
  className = "mt-0.5 pl-6",
  onOpen,
  portals,
  t,
}: {
  className?: string;
  onOpen: (portal: SubagentDispatchPortal) => void;
  portals: readonly SubagentDispatchPortal[];
  t: Translator;
}) {
  return (
    <ul className={`flex flex-wrap gap-1.5 ${className}`}>
      {portals.map((portal) => (
        <li key={portal.partId}>
          <SubagentChip onOpen={() => onOpen(portal)} portal={portal} t={t} />
        </li>
      ))}
    </ul>
  );
}

function SubagentChip({
  onOpen,
  portal,
  t,
}: {
  onOpen: () => void;
  portal: SubagentDispatchPortal;
  t: Translator;
}) {
  const role =
    portal.agentRole?.trim() || t("conversation.subagent.roleFallback");
  const title = portal.title.trim() || t("conversation.subagent.untitled");
  const unresolved = portal.unresolved === true;
  return (
    <Button
      aria-label={
        unresolved
          ? t("conversation.subagent.unresolvedHint")
          : t("conversation.subagent.openChildSession", {
              title: `${role} ${title}`,
            })
      }
      className="h-7 max-w-[16rem] gap-1.5 rounded-full border border-primary/25 bg-theme-control/70 px-2.5 text-code-sm text-on-surface-variant hover:bg-primary/10 hover:text-on-surface"
      data-subagent-chip={portal.partId}
      data-subagent-unresolved={unresolved ? "true" : undefined}
      disabled={unresolved}
      onClick={unresolved ? undefined : onOpen}
      size="sm"
      title={
        unresolved
          ? `${role} · ${title} · ${t("conversation.subagent.unresolved")}`
          : `${role} · ${title}`
      }
      type="button"
      variant="outline"
    >
      <span
        className={`size-1.5 shrink-0 rounded-full ${
          portal.status === "running"
            ? "bg-status-update"
            : portal.status === "failed"
              ? "bg-status-remove"
              : "bg-status-create"
        }`}
      />
      <span className="shrink-0 font-medium text-primary">{role}</span>
      <span className="min-w-0 truncate">{title}</span>
    </Button>
  );
}

function turnRailTitle(turn: ConversationTurnRecord): string {
  const line = (turn.prompt ?? "")
    .split(/\r?\n/)
    .map((value) => value.trim())
    .find((value) => value.length > 0);
  return line ?? turn.title ?? "";
}
