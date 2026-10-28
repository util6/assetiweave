import { Check, ChevronRight, Copy, GitFork } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  ConversationQuestionDetail,
  ConversationRecordKind,
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import type { Translator } from "../../i18n/I18nProvider";
import { Badge } from "../foundation/Badge";
import type {
  ConversationContentBlock,
  ConversationContentVisibility,
  ConversationDisplayNode,
} from "./ConversationContentCards";
import {
  buildConversationDisplayNodesFromNodes,
  conversationCardDomId,
  ConversationContentCards,
} from "./ConversationContentCards";
import type { ConversationContentController } from "./useConversationContentController";
import type {
  ConversationContentCardColorSettings,
  ResolvedConversationTranslationSettings,
} from "../../store/settings/settingsSchema";

import { conversationIdFragment } from "../../utils/conversationIds";
import { MarkdownContent } from "./ConversationMarkdown";
import type { ConversationCommandProjection } from "../../services/conversationCommandProjection";

export interface ConversationTurnPresentation {
  blocks: ConversationContentBlock[];
  childCount: number;
  depth: number;
  displayNodes?: ConversationDisplayNode[];
  hasContent: boolean;
  isSubagent: boolean;
  parentTurnId: string | null;
  promptBlockId: string;
  turn: ConversationTurnRecord;
}

export function buildConversationTurnPresentations(
  question: ConversationQuestionDetail,
  commandProjections: readonly ConversationCommandProjection[] = [],
): ConversationTurnPresentation[] {
  const projectionByPartId = new Map(
    commandProjections.map((projection) => [projection.part_id, projection]),
  );
  const presentations = question.turns.map((turn) => {
    const displayNodes = applyConversationCommandProjections(
      buildConversationDisplayNodesFromNodes(
        question.projected_content_nodes.filter(
          (node) => node.turn_id === turn.id,
        ),
      ),
      projectionByPartId,
    );

    return {
      blocks: [],
      childCount: 0,
      depth: 0,
      displayNodes,
      hasContent: displayNodes.length > 0,
      isSubagent: turn.turn_kind === "subagent" || turn.turn_kind === "agent",
      parentTurnId: null,
      promptBlockId: `${turn.id}-question`,
      turn,
    } satisfies ConversationTurnPresentation;
  });

  linkTurnParents(presentations);
  return orderTurnTree(presentations);
}

function linkTurnParents(presentations: ConversationTurnPresentation[]) {
  const byId = new Map(
    presentations.map((presentation) => [presentation.turn.id, presentation]),
  );
  for (const presentation of presentations) {
    const parentId = presentation.turn.parent_turn_id ?? null;
    presentation.parentTurnId =
      parentId && parentId !== presentation.turn.id && byId.has(parentId)
        ? parentId
        : null;
  }
  detachTurnCycles(presentations, byId);
}

function detachTurnCycles(
  presentations: ConversationTurnPresentation[],
  byId: ReadonlyMap<string, ConversationTurnPresentation>,
) {
  for (const presentation of presentations) {
    const path: ConversationTurnPresentation[] = [];
    const indexById = new Map<string, number>();
    let current: ConversationTurnPresentation | undefined = presentation;
    while (current) {
      const cycleStart = indexById.get(current.turn.id);
      if (cycleStart !== undefined) {
        for (let index = cycleStart; index < path.length; index += 1)
          path[index]!.parentTurnId = null;
        break;
      }
      indexById.set(current.turn.id, path.length);
      path.push(current);
      current = current.parentTurnId
        ? byId.get(current.parentTurnId)
        : undefined;
    }
  }
}

function orderTurnTree(
  presentations: ConversationTurnPresentation[],
): ConversationTurnPresentation[] {
  const childrenByParent = new Map<string, ConversationTurnPresentation[]>();
  const roots: ConversationTurnPresentation[] = [];
  for (const presentation of presentations) {
    if (!presentation.parentTurnId) {
      roots.push(presentation);
      continue;
    }
    const siblings = childrenByParent.get(presentation.parentTurnId) ?? [];
    siblings.push(presentation);
    childrenByParent.set(presentation.parentTurnId, siblings);
  }

  const ordered: ConversationTurnPresentation[] = [];
  const visit = (presentation: ConversationTurnPresentation, depth: number) => {
    presentation.depth = depth;
    presentation.childCount =
      childrenByParent.get(presentation.turn.id)?.length ?? 0;
    ordered.push(presentation);
    for (const child of childrenByParent.get(presentation.turn.id) ?? [])
      visit(child, depth + 1);
  };
  for (const root of roots) visit(root, 0);
  return ordered;
}

/**
 * Flattens the tree into the virtual list order. Only collapsed subagent
 * branches hide their children, so main turns keep their delegated work one
 * level deep and the virtualized collection never lays out hidden subtrees.
 */
export function visibleConversationTurnPresentations(
  models: readonly ConversationTurnPresentation[],
  expandedTurnIds: ReadonlySet<string>,
): ConversationTurnPresentation[] {
  const byId = new Map(models.map((model) => [model.turn.id, model]));
  const visibleIds = new Set<string>();
  const visible: ConversationTurnPresentation[] = [];
  for (const model of models) {
    if (model.parentTurnId) {
      if (!visibleIds.has(model.parentTurnId)) continue;
      const parent = byId.get(model.parentTurnId);
      if (parent?.isSubagent && !expandedTurnIds.has(model.parentTurnId))
        continue;
    }
    visibleIds.add(model.turn.id);
    visible.push(model);
  }
  return visible;
}

export function conversationTurnAncestorIds(
  models: readonly ConversationTurnPresentation[],
  turnId: string,
): string[] {
  const byId = new Map(models.map((model) => [model.turn.id, model]));
  const seen = new Set<string>([turnId]);
  const ancestors: string[] = [];
  let current = byId.get(turnId)?.parentTurnId ?? null;
  while (current && !seen.has(current)) {
    ancestors.push(current);
    seen.add(current);
    current = byId.get(current)?.parentTurnId ?? null;
  }
  return ancestors;
}

function applyConversationCommandProjections(
  nodes: readonly ConversationDisplayNode[],
  projectionByPartId: ReadonlyMap<string, ConversationCommandProjection>,
): ConversationDisplayNode[] {
  return nodes.flatMap((node): ConversationDisplayNode[] => {
    if (node.type === "card") {
      if (node.block.type !== "command") return [node];
      const projection = node.block.partId
        ? projectionByPartId.get(node.block.partId)
        : undefined;
      return projection
        ? projectCommandBlock(node.block, projection).map((block) => ({
            ...node,
            block,
          }))
        : [node];
    }

    const commands = node.commands.flatMap((command) => {
      const projection = command.partId
        ? projectionByPartId.get(command.partId)
        : undefined;
      if (!projection) return [command];
      return projectCommandBlock(command, projection);
    });
    return [
      {
        ...node,
        commands,
      },
    ];
  });
}

function projectCommandBlock(
  rawBlock: ConversationContentBlock,
  projection: ConversationCommandProjection,
): ConversationContentBlock[] {
  return projection.nodes.map((node, index) => ({
    ...rawBlock,
    id: `${rawBlock.partId ?? rawBlock.id}::display:${node.display_order}`,
    commandLabel: node.command_label || rawBlock.commandLabel || null,
    legacyAnchorIds:
      index === 0
        ? Array.from(
            new Set(
              [
                rawBlock.id,
                rawBlock.partId,
                ...(rawBlock.legacyAnchorIds ?? []),
              ].filter(Boolean) as string[],
            ),
          )
        : [],
    text: node.command,
  }));
}

export function collectConversationTurnBlocks(
  models: readonly ConversationTurnPresentation[],
): ConversationContentBlock[] {
  return models.flatMap((model) =>
    model.displayNodes
      ? model.displayNodes.flatMap((node) =>
          node.type === "card"
            ? [node.block]
            : [...node.commands, ...node.results],
        )
      : model.blocks,
  );
}

export function buildConversationBlockTurnIndex(
  models: readonly ConversationTurnPresentation[],
): Map<string, string> {
  const index = new Map<string, string>();
  for (const model of models) {
    index.set(model.promptBlockId, model.turn.id);
    for (const block of model.blocks) {
      index.set(block.id, model.turn.id);
      for (const legacyAnchorId of block.legacyAnchorIds ?? [])
        index.set(legacyAnchorId, model.turn.id);
    }
    for (const node of model.displayNodes ?? []) {
      if (node.type === "card") {
        index.set(node.block.id, model.turn.id);
        for (const legacyAnchorId of node.block.legacyAnchorIds ?? [])
          index.set(legacyAnchorId, model.turn.id);
        continue;
      }
      index.set(node.sourceExecutionId, model.turn.id);
      for (const block of [...node.commands, ...node.results]) {
        index.set(block.id, model.turn.id);
        for (const legacyAnchorId of block.legacyAnchorIds ?? [])
          index.set(legacyAnchorId, model.turn.id);
      }
    }
  }
  return index;
}

export const ConversationTurn = memo(function ConversationTurn({
  activeBlockId,
  colors,
  controller,
  expanded: expandedProp = false,
  index,
  onCopyError,
  onCommandPartsVisible,
  onToggleExpanded,
  recordKind,
  resultPreviewLineLimit,
  t,
  translationSettings,
  visibility,
  model,
}: {
  activeBlockId?: string | null;
  colors?: ConversationContentCardColorSettings;
  controller: ConversationContentController;
  expanded?: boolean;
  index: number;
  model: ConversationTurnPresentation;
  onCopyError?: (message: string) => void;
  onCommandPartsVisible?: (partIds: string[]) => void;
  onToggleExpanded?: (turnId: string) => void;
  recordKind: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility: ConversationContentVisibility;
}) {
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<number | null>(null);
  const promptHighlighted = activeBlockId === model.promptBlockId;
  const isSubagent = model.isSubagent;
  const isContinuation = model.turn.turn_kind === "continuation";
  const expanded = isSubagent ? expandedProp : true;
  const promptSummary = firstPromptLine(model.turn.prompt);
  const toggleLabel = expanded
    ? t("conversation.turn.collapseSubagent")
    : t("conversation.turn.expandSubagent");

  useEffect(
    () => () => {
      if (copiedTimerRef.current !== null)
        window.clearTimeout(copiedTimerRef.current);
    },
    [],
  );

  async function copyPrompt() {
    try {
      if (!navigator.clipboard?.writeText)
        throw new Error("Clipboard API is unavailable");
      await navigator.clipboard.writeText(model.turn.prompt);
      if (copiedTimerRef.current !== null)
        window.clearTimeout(copiedTimerRef.current);
      setCopied(true);
      copiedTimerRef.current = window.setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 1400);
    } catch (error) {
      onCopyError?.(
        t("conversation.content.copyFailed", { message: errorMessage(error) }),
      );
    }
  }

  return (
    <section
      className={`conversation-turn mb-6 ${
        isSubagent ? "border-l-2 border-primary/20 pl-3" : ""
      }`}
      data-conversation-turn-depth={model.depth}
      data-conversation-turn-expanded={isSubagent ? expanded : undefined}
      data-conversation-turn-id={model.turn.id}
      style={
        isSubagent && model.depth > 0
          ? { marginLeft: `${Math.min(model.depth, 5) * 0.75}rem` }
          : undefined
      }
    >
      <div
        className={`conversation-prompt-block scroll-mt-32 rounded-xl border px-4 py-3 transition-shadow ${
          isSubagent
            ? "border-primary/20 bg-primary/[0.035]"
            : "border-primary/30 bg-primary/[0.055]"
        } ${
          promptHighlighted
            ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]"
            : ""
        }`}
        data-conversation-card-id={model.promptBlockId}
        id={conversationCardDomId(model.promptBlockId)}
      >
        <div className="mb-2 flex items-center justify-between gap-3">
          <div
            className={`flex items-center gap-2.5 min-w-0 ${
              isSubagent ? "flex-wrap" : ""
            }`}
          >
            {isSubagent ? (
              <button
                aria-expanded={expanded}
                aria-label={toggleLabel}
                className="grid size-6 shrink-0 place-items-center rounded-full border border-primary/30 bg-theme-card/60 text-primary transition-colors hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
                onClick={() => onToggleExpanded?.(model.turn.id)}
                title={toggleLabel}
                type="button"
              >
                <ChevronRight
                  className={`size-3.5 transition-transform ${
                    expanded ? "rotate-90" : ""
                  }`}
                />
              </button>
            ) : null}
            {isSubagent ? (
              <h3 className="flex items-center gap-2 text-label-caps text-primary shrink-0">
                <GitFork size={13} className="text-primary/90" />
                {t("conversation.question.subagentTask")}
              </h3>
            ) : isContinuation ? (
              <h3 className="flex items-center gap-2 text-label-caps text-on-surface-variant shrink-0">
                <span className="size-2 rounded-full bg-status-info" />
                {t("conversation.question.continuationPrompt")}
              </h3>
            ) : (
              <h3 className="flex items-center gap-2 text-label-caps text-primary shrink-0">
                <span className="size-2 rounded-full bg-primary" />
                {t("conversation.question.userPrompt")}
              </h3>
            )}
            {model.turn.agent_role ? (
              <Badge
                tone="neutral"
                className="rounded-full font-medium text-xs px-2.5 py-0.5 truncate max-w-xs"
                title={model.turn.agent_role}
              >
                {model.turn.agent_role}
              </Badge>
            ) : null}
            {isSubagent && model.turn.title ? (
              <span
                className="truncate text-body-sm text-on-surface-variant"
                title={model.turn.title}
              >
                {model.turn.title}
              </span>
            ) : null}
            {model.turn.model ? (
              <Badge
                tone="neutral"
                className="rounded-full font-mono text-xs normal-case tracking-normal px-2.5 py-0.5 truncate max-w-xs"
                title={model.turn.model}
              >
                {model.turn.model}
              </Badge>
            ) : null}
            {isSubagent && model.childCount > 0 ? (
              <Badge
                tone="neutral"
                className="shrink-0 rounded-full text-xs normal-case tracking-normal px-2.5 py-0.5"
              >
                {t("conversation.turn.childCount", {
                  count: model.childCount,
                })}
              </Badge>
            ) : null}
            {isSubagent ? <SubagentStatus turn={model.turn} t={t} /> : null}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <span
              className="select-text rounded-md border border-primary/25 bg-theme-card/45 px-1.5 py-0.5 font-mono text-code-sm normal-case text-on-surface-muted"
              title={model.promptBlockId}
            >
              {conversationIdFragment(model.promptBlockId)}
            </span>
            <PromptCopyButton
              copied={copied}
              isSubagent={isSubagent}
              onClick={() => void copyPrompt()}
              t={t}
            />
          </div>
        </div>
        {isSubagent && !expanded ? (
          <p
            className="truncate text-body-sm text-on-surface-variant"
            title={model.turn.prompt}
          >
            {promptSummary}
          </p>
        ) : (
          <MarkdownContent value={model.turn.prompt} />
        )}
      </div>
      {expanded ? (
        <div className="mt-3 pl-3">
          <h3 className="mb-3 text-label-caps text-on-surface-muted">
            {t("conversation.question.parts")}
          </h3>
          {!model.hasContent ? (
            isSubagent ? (
              <p className="px-2 py-4 text-body-sm text-on-surface-variant">
                {t("conversation.turn.subagentNoRecords")}
              </p>
            ) : (
              <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
            )
          ) : (
            <ConversationContentCards
              activeBlockId={activeBlockId}
              colors={colors}
              controller={controller}
              nodes={model.displayNodes ?? []}
              onCopyError={onCopyError}
              onCommandPartsVisible={onCommandPartsVisible}
              recordKind={recordKind}
              resultPreviewLineLimit={resultPreviewLineLimit}
              t={t}
              translationSettings={translationSettings}
              visibility={visibility}
            />
          )}
        </div>
      ) : null}
    </section>
  );
});

function SubagentStatus({
  turn,
  t,
}: {
  turn: ConversationTurnRecord;
  t: Translator;
}) {
  const running = Boolean(turn.started_at) && !turn.ended_at;
  return (
    <Badge
      tone={running ? "update" : "neutral"}
      className="shrink-0 gap-1.5 rounded-full text-xs normal-case tracking-normal px-2.5 py-0.5"
    >
      <span
        className={`size-1.5 rounded-full ${
          running ? "bg-status-update" : "bg-status-create"
        }`}
      />
      {running
        ? t("conversation.turn.status.running")
        : t("conversation.turn.status.completed")}
    </Badge>
  );
}

function firstPromptLine(prompt: string) {
  return (
    prompt
      .split(/\r?\n/)
      .find((line) => line.trim())
      ?.trim() ?? ""
  );
}

function PromptCopyButton({
  copied,
  isSubagent,
  onClick,
  t,
}: {
  copied: boolean;
  isSubagent?: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const label = copied
    ? t("conversation.content.copied")
    : isSubagent
      ? t("conversation.question.copySubagentPrompt")
      : t("conversation.question.copyPrompt");
  return (
    <button
      aria-label={label}
      className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-label-caps text-primary/80 transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
      onClick={onClick}
      title={label}
      type="button"
    >
      {copied ? (
        <Check className="size-[1em]" />
      ) : (
        <Copy className="size-[1em]" />
      )}
    </button>
  );
}

function ToolbarTextButton({
  icon,
  label,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className="inline-flex items-center gap-1 rounded-md border border-theme-control-border bg-theme-control/80 px-2 py-1 text-code-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover"
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

function EmptyPanel({ children }: { children: ReactNode }) {
  return (
    <div className="conversation-empty-state m-2 rounded-2xl p-6 text-center text-body-sm text-on-surface-variant">
      {children}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
