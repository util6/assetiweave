import { ChevronRight, CornerDownRight, GitFork } from "lucide-react";
import type {
  ConversationQuestionDetail,
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import type { Translator } from "../../i18n/I18nProvider";

/**
 * 左栏问题卡片里的 Turn 轨：一个问题展开后逐行列出它的 Turn，
 * 主 Turn 行下折叠该 Turn 派发的子代理，子代理行只带归属标签，不再塞进正文。
 */
export interface QuestionTurnEntry {
  turn: ConversationTurnRecord;
  isSubagent: boolean;
  /** 0 = 主 Turn；1..n = 子代理层级（Tier）。 */
  tier: number;
  parentTurnId: string | null;
  /** 该主 Turn 直接派发的子代理。 */
  subagents: QuestionTurnEntry[];
  /** 主 Turn 在问题内的序号（从 1 开始），子代理行用它标注归属。 */
  ordinal: number;
  /** 子代理所属主 Turn 的序号。 */
  parentOrdinal: number | null;
}

function turnSortKey(turn: ConversationTurnRecord): [number, string] {
  return [turn.turn_index ?? 0, turn.id];
}

/**
 * 把扁平 Turn 列表整理成「主 Turn + 其子代理」两级结构。
 * 子代理的父指针可能指向另一个子代理（多层派发），这里统一挂到最近的根主 Turn 下，
 * 层级仍按 tier 保留，保证折叠一次就能看到该主 Turn 的全部委派。
 */
export function buildQuestionTurnRail(
  question: ConversationQuestionDetail,
): QuestionTurnEntry[] {
  const turns = [...question.turns].sort((left, right) => {
    const [leftIndex, leftId] = turnSortKey(left);
    const [rightIndex, rightId] = turnSortKey(right);
    return leftIndex - rightIndex || leftId.localeCompare(rightId);
  });
  const byId = new Map(turns.map((turn) => [turn.id, turn]));
  const isSubagent = (turn: ConversationTurnRecord) =>
    turn.turn_kind === "subagent" || turn.turn_kind === "agent";

  // 解析每个 Turn 的根主 Turn（跳过中间层子代理）。
  const rootOf = new Map<string, string | null>();
  const tierOf = new Map<string, number>();
  for (const turn of turns) {
    let current = turn;
    let tier = isSubagent(turn) ? 1 : 0;
    const seen = new Set<string>([current.id]);
    while (current.parent_turn_id) {
      const parent = byId.get(current.parent_turn_id);
      if (!parent || seen.has(parent.id)) {
        current = parent ?? current;
        if (!parent) break;
      }
      seen.add(parent.id);
      tier += 1;
      if (!isSubagent(parent)) {
        rootOf.set(turn.id, parent.id);
        tierOf.set(turn.id, tier - 1);
        break;
      }
      current = parent;
    }
    if (!rootOf.has(turn.id)) {
      rootOf.set(turn.id, null);
      tierOf.set(turn.id, isSubagent(turn) ? 1 : 0);
    }
  }

  const roots = turns.filter((turn) => !isSubagent(turn));
  const entries: QuestionTurnEntry[] = [];
  let ordinal = 0;
  for (const root of roots) {
    ordinal += 1;
    const children = turns.filter(
      (turn) => isSubagent(turn) && rootOf.get(turn.id) === root.id,
    );
    entries.push({
      turn: root,
      isSubagent: false,
      tier: 0,
      parentTurnId: null,
      subagents: children.map((child) => ({
        turn: child,
        isSubagent: true,
        tier: tierOf.get(child.id) ?? 1,
        parentTurnId: child.parent_turn_id ?? null,
        subagents: [],
        ordinal: 0,
        parentOrdinal: ordinal,
      })),
      ordinal,
      parentOrdinal: null,
    });
  }

  // 父 Turn 不在本问题内的孤儿子代理，单独成组，避免凭空消失。
  const orphans = turns.filter(
    (turn) => isSubagent(turn) && !rootOf.get(turn.id),
  );
  for (const orphan of orphans) {
    entries.push({
      turn: orphan,
      isSubagent: true,
      tier: tierOf.get(orphan.id) ?? 1,
      parentTurnId: orphan.parent_turn_id ?? null,
      subagents: [],
      ordinal: 0,
      parentOrdinal: null,
    });
  }
  return entries;
}

export function QuestionTurnRail({
  entries,
  expandedRootIds,
  onToggleRoot,
  onSelectTurn,
  selectedTurnId,
  t,
}: {
  entries: QuestionTurnEntry[];
  expandedRootIds: ReadonlySet<string>;
  onToggleRoot: (turnId: string) => void;
  onSelectTurn: (turnId: string) => void;
  selectedTurnId: string | null;
  t: Translator;
}) {
  return (
    <ul className="conversation-turn-rail mt-2 flex flex-col gap-1 border-l-2 border-primary/15 pl-2">
      {entries.map((entry) => (
        <li key={entry.turn.id}>
          <button
            className={`flex w-full min-w-0 items-start gap-2 rounded-xl px-2 py-1.5 text-left transition-colors hover:bg-primary/[0.06] ${
              entry.turn.id === selectedTurnId ? "bg-primary/[0.09]" : ""
            }`}
            data-turn-rail-id={entry.turn.id}
            onClick={() => onSelectTurn(entry.turn.id)}
            type="button"
          >
            <span className="mt-0.5 shrink-0 font-mono text-code-sm text-on-surface-muted">
              {entry.isSubagent ? "↳" : `${entry.ordinal}`}
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex min-w-0 items-center gap-1.5">
                {entry.isSubagent ? (
                  <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full border border-primary/25 bg-primary/10 px-1.5 text-code-sm text-primary">
                    <GitFork size={10} />
                    {t("conversation.question.subagentTask")}
                  </span>
                ) : null}
                {entry.turn.agent_role ? (
                  <span className="truncate text-code-sm font-medium text-on-surface">
                    {entry.turn.agent_role}
                  </span>
                ) : null}
                {entry.isSubagent ? (
                  <span className="shrink-0 rounded-full bg-theme-control px-1.5 text-code-sm text-on-surface-muted">
                    {t("conversation.turn.tier", { tier: entry.tier })}
                  </span>
                ) : null}
                {entry.isSubagent && entry.parentOrdinal ? (
                  <span className="shrink-0 text-code-sm text-on-surface-muted">
                    {t("conversation.turn.railParent", {
                      ordinal: entry.parentOrdinal,
                    })}
                  </span>
                ) : null}
              </span>
              <span className="mt-0.5 block truncate text-body-sm text-on-surface-variant">
                {turnRailTitle(entry.turn)}
              </span>
            </span>
            {entry.subagents.length > 0 ? (
              <span
                aria-expanded={expandedRootIds.has(entry.turn.id)}
                aria-label={t("conversation.turn.railToggleSubagents", {
                  count: entry.subagents.length,
                })}
                className="mt-0.5 inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/25 bg-theme-control/70 px-2 py-0.5 text-code-sm text-primary"
                onClick={(event) => {
                  event.stopPropagation();
                  onToggleRoot(entry.turn.id);
                }}
                role="button"
                tabIndex={0}
              >
                {t("conversation.turn.childCount", {
                  count: entry.subagents.length,
                })}
                <ChevronRight
                  className={`size-3 transition-transform ${
                    expandedRootIds.has(entry.turn.id) ? "rotate-90" : ""
                  }`}
                />
              </span>
            ) : null}
          </button>
          {entry.subagents.length > 0 && expandedRootIds.has(entry.turn.id) ? (
            <ul className="mt-0.5 flex flex-col gap-0.5 pl-3">
              {entry.subagents.map((child) => (
                <li key={child.turn.id}>
                  <button
                    className={`flex w-full min-w-0 items-start gap-1.5 rounded-xl px-2 py-1 text-left transition-colors hover:bg-primary/[0.06] ${
                      child.turn.id === selectedTurnId
                        ? "bg-primary/[0.09]"
                        : ""
                    }`}
                    data-turn-rail-id={child.turn.id}
                    onClick={() => onSelectTurn(child.turn.id)}
                    type="button"
                  >
                    <CornerDownRight
                      className="mt-0.5 size-3 shrink-0 text-primary/60"
                      size={12}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex min-w-0 items-center gap-1.5">
                        <span className="truncate text-code-sm font-medium text-on-surface">
                          {child.turn.agent_role ??
                            t("conversation.question.subagentTask")}
                        </span>
                        <span className="shrink-0 rounded-full bg-theme-control px-1.5 text-code-sm text-on-surface-muted">
                          {t("conversation.turn.tier", { tier: child.tier })}
                        </span>
                        {child.parentOrdinal ? (
                          <span className="shrink-0 text-code-sm text-on-surface-muted">
                            {t("conversation.turn.railParent", {
                              ordinal: child.parentOrdinal,
                            })}
                          </span>
                        ) : null}
                      </span>
                      <span className="mt-0.5 block truncate text-body-sm text-on-surface-variant">
                        {turnRailTitle(child.turn)}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

function turnRailTitle(turn: ConversationTurnRecord): string {
  const line = (turn.prompt ?? "")
    .split(/\r?\n/)
    .map((value) => value.trim())
    .find((value) => value.length > 0);
  return line ?? turn.title ?? "";
}
