import { Check, Copy } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  ConversationQuestionDetail,
  ConversationRecordKind,
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import type { Translator } from "../../i18n/I18nProvider";
import { Badge } from "../foundation/Badge";
import { TruncatedText } from "../foundation/TruncatedText";
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
import {
  collectSubagentPortals,
  mainConversationTurns,
  SUBAGENT_PORTAL_RENDERER,
  type SubagentDispatchPortal,
  type SubagentDispatchPortalViewModel,
} from "./conversationSubagents";
export interface ConversationTurnPresentation {
  blocks: ConversationContentBlock[];
  displayNodes?: ConversationDisplayNode[];
  hasContent: boolean;
  promptBlockId: string;
  turn: ConversationTurnRecord;
}

/**
 * 一问一轮：只把真实人类提问轮次整理成正文呈现。
 *
 * ADR-0022 之后子代理是独立从属会话，父轮次里只留轻量委派锚点；
 * 锚点在正文原位改写为传送门卡片，不再伪装成独立 Turn 平铺进来。
 *
 * 注意锚点 Part 在后端**不挂 `content_card`**（见 `merge_anchor_attrs`），
 * 因此它不会产出投影节点；这里必须从 Part 侧补一张合成卡片，
 * 否则真实数据下正文里根本不会出现传送门。
 */
export function buildConversationTurnPresentations(
  question: ConversationQuestionDetail,
  commandProjections: readonly ConversationCommandProjection[] = [],
): ConversationTurnPresentation[] {
  const projectionByPartId = new Map(
    commandProjections.map((projection) => [projection.part_id, projection]),
  );
  const portals = collectSubagentPortals(question);
  const portalByPartId = new Map(
    portals.map((portal) => [portal.partId, portal]),
  );
  const partOrderById = new Map(
    (question.parts ?? []).map((part) => [part.id, part.part_index]),
  );

  return mainConversationTurns(question).map((turn) => {
    const projectedNodes = applyConversationCommandProjections(
      buildConversationDisplayNodesFromNodes(
        question.projected_content_nodes.filter(
          (node) => node.turn_id === turn.id,
        ),
      ),
      projectionByPartId,
    ).flatMap((node) => applySubagentPortal(node, portalByPartId));
    const displayNodes = mergeAnchoredPortals({
      nodes: projectedNodes,
      partOrderById,
      portals: portals.filter((portal) => portal.turnId === turn.id),
      turnId: turn.id,
    });

    return {
      blocks: [],
      displayNodes,
      hasContent: displayNodes.length > 0,
      promptBlockId: `${turn.id}-question`,
      turn,
    } satisfies ConversationTurnPresentation;
  });
}

/**
 * 把父轮次里的子代理锚点原地改写为传送门卡片节点。
 *
 * 定位不到子会话的锚点不再丢弃：`collectSubagentPortals` 会为它产出
 * `unresolved` 卡片，保证"这一轮派过子代理"始终可见，而不是凭空消失。
 */
function applySubagentPortal(
  node: ConversationDisplayNode,
  portalByPartId: ReadonlyMap<string, SubagentDispatchPortal>,
): ConversationDisplayNode[] {
  if (node.type !== "card") return [node];
  const partId = node.block.partId ?? node.block.id;
  const portal = portalByPartId.get(partId);
  if (!portal) return [node];
  return [
    {
      ...node,
      block: {
        ...node.block,
        format: "plain",
        portal,
        renderer: SUBAGENT_PORTAL_RENDERER,
        text: "",
      },
    },
  ];
}

/**
 * 把没有投影节点的委派锚点补成传送门卡片，并按 Part 顺序插回正文。
 *
 * 后端锚点不带 `content_card`，历史投影数据也不一定有锚点节点；
 * 缺少合成这一步就会出现「左栏有胶囊、正文没有卡片」的断层。
 */
function mergeAnchoredPortals({
  nodes,
  partOrderById,
  portals,
  turnId,
}: {
  nodes: ConversationDisplayNode[];
  partOrderById: ReadonlyMap<string, number>;
  portals: readonly SubagentDispatchPortal[];
  turnId: string;
}): ConversationDisplayNode[] {
  if (portals.length === 0) return nodes;
  const representedPartIds = new Set<string>();
  for (const node of nodes)
    for (const block of displayNodeBlocks(node))
      representedPartIds.add(block.partId ?? block.id);

  const synthesized = portals
    .filter((portal) => !representedPartIds.has(portal.partId))
    .map((portal, offset) => ({
      index: nodes.length + offset,
      node: synthesizePortalNode(portal, turnId),
      order: partOrderById.get(portal.partId) ?? Number.MAX_SAFE_INTEGER,
    }));
  if (synthesized.length === 0) return nodes;

  const ordered = nodes.map((node, index) => ({
    index,
    node,
    order: displayNodePartOrder(node, partOrderById),
  }));
  return [...ordered, ...synthesized]
    .sort((left, right) => left.order - right.order || left.index - right.index)
    .map((item) => item.node);
}

function synthesizePortalNode(
  portal: SubagentDispatchPortal,
  turnId: string,
): ConversationDisplayNode {
  return {
    block: {
      format: "plain",
      id: portal.partId,
      kind: "subagent",
      partId: portal.partId,
      portal,
      renderer: SUBAGENT_PORTAL_RENDERER,
      role: "assistant",
      text: "",
      type: "subagent",
    },
    turnId,
    type: "card",
  };
}

function displayNodeBlocks(
  node: ConversationDisplayNode,
): ConversationContentBlock[] {
  return node.type === "card"
    ? [node.block]
    : [...node.commands, ...node.results];
}

function displayNodePartOrder(
  node: ConversationDisplayNode,
  partOrderById: ReadonlyMap<string, number>,
): number {
  let order = Number.MAX_SAFE_INTEGER;
  for (const block of displayNodeBlocks(node)) {
    const candidate = partOrderById.get(block.partId ?? block.id);
    if (candidate !== undefined && candidate < order) order = candidate;
  }
  return order;
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
  index,
  onCopyError,
  onCommandPartsVisible,
  onOpenSubagent,
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
  index: number;
  model: ConversationTurnPresentation;
  onCopyError?: (message: string) => void;
  onCommandPartsVisible?: (partIds: string[]) => void;
  onOpenSubagent?: (
    portal: SubagentDispatchPortalViewModel,
    turn: ConversationTurnRecord,
  ) => void;
  recordKind: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility: ConversationContentVisibility;
}) {
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<number | null>(null);
  const promptHighlighted = activeBlockId === model.promptBlockId;
  const openSubagent = onOpenSubagent
    ? (portal: SubagentDispatchPortalViewModel) =>
        onOpenSubagent(portal, model.turn)
    : undefined;

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
      className="conversation-turn mb-6"
      data-conversation-turn-id={model.turn.id}
      data-conversation-turn-index={index}
    >
      <div
        className={`conversation-prompt-block scroll-mt-32 rounded-xl border border-primary/30 bg-primary/[0.055] px-4 py-3 transition-shadow ${
          promptHighlighted
            ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]"
            : ""
        }`}
        data-conversation-card-id={model.promptBlockId}
        id={conversationCardDomId(model.promptBlockId)}
      >
        <div className="mb-2 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2.5">
            <h3 className="flex shrink-0 items-center gap-2 text-label-caps text-primary">
              <span className="size-2 rounded-full bg-primary" />
              {t("conversation.question.userPrompt")}
            </h3>
            {model.turn.agent_role ? (
              <Badge
                className="max-w-xs rounded-full px-2.5 py-0.5 text-xs font-medium"
                tone="neutral"
              >
                <TruncatedText>{model.turn.agent_role}</TruncatedText>
              </Badge>
            ) : null}
            {model.turn.model ? (
              <Badge
                className="max-w-xs rounded-full px-2.5 py-0.5 font-mono text-xs normal-case tracking-normal"
                tone="neutral"
              >
                <TruncatedText>{model.turn.model}</TruncatedText>
              </Badge>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <span
              className="select-text rounded-xl border border-primary/25 bg-theme-card/45 px-1.5 py-0.5 font-mono text-code-sm normal-case text-on-surface-muted"
              title={model.promptBlockId}
            >
              {conversationIdFragment(model.promptBlockId)}
            </span>
            <PromptCopyButton
              copied={copied}
              onClick={() => void copyPrompt()}
              t={t}
            />
          </div>
        </div>
        <MarkdownContent value={model.turn.prompt} />
      </div>
      <div className="mt-3 pl-3">
        <h3 className="mb-3 text-label-caps text-on-surface-muted">
          {t("conversation.question.parts")}
        </h3>
        {!model.hasContent ? (
          <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
        ) : (
          <ConversationContentCards
            activeBlockId={activeBlockId}
            colors={colors}
            controller={controller}
            nodes={model.displayNodes ?? []}
            onCopyError={onCopyError}
            onCommandPartsVisible={onCommandPartsVisible}
            onOpenSubagent={openSubagent}
            recordKind={recordKind}
            resultPreviewLineLimit={resultPreviewLineLimit}
            t={t}
            translationSettings={translationSettings}
            visibility={visibility}
          />
        )}
      </div>
    </section>
  );
});

function PromptCopyButton({
  copied,
  onClick,
  t,
}: {
  copied: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const label = copied
    ? t("conversation.content.copied")
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
