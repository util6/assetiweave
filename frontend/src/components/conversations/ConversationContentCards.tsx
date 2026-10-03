import {
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Copy,
  ExternalLink,
  Eye,
  FileText,
  GitCompareArrows,
  GitFork,
  Languages,
  Layers,
  Search,
  XCircle,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  updateConversationPartTranslation,
  type AiExecutionPhase,
  type ConversationCardTranslationRequest,
  type OpencodeTranslationAvailability,
  type OpencodeTranslationResult,
  type ConversationPartTranslationUpdateRequest,
} from "../../services/cardTranslation";
import { revealPath } from "../../services/catalog";
import {
  DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  type ConversationCardRenderer,
  type ConversationContentNode,
  type ConversationContentType,
  type ConversationContentVisibility,
  type ConversationPart,
  type ConversationPartRole,
  type ConversationRecordKind,
} from "../../types";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
  DEFAULT_FALLBACK_CARD_COLOR,
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  SEMANTIC_FALLBACK_CARD_COLORS,
  normalizeConversationTranslationTargetLanguage,
  type ConversationContentCardColorSettings,
  type ResolvedConversationTranslationSettings,
  type ConversationTranslationTargetLanguage,
} from "../../store/settings/settingsSchema";

import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import { MarkdownContent } from "./ConversationMarkdown";
import {
  ConversationCardKindIcon,
  conversationCardPresentationKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";
import {
  ConversationDiff,
  summarizeConversationDiff,
} from "./ConversationDiff";

export type ConversationContentFormat = "plain" | "markdown";

export { DEFAULT_CONVERSATION_CONTENT_VISIBILITY } from "../../types";
export type {
  ConversationContentType,
  ConversationContentVisibility,
} from "../../types";
import {
  useConversationContentController,
  type ConversationContentController,
  type ConversationTranslationTaskController,
  type TranslationAvailabilityStatus,
} from "./useConversationContentController";

export type {
  ConversationTranslationTaskController,
  TranslationAvailabilityStatus,
} from "./useConversationContentController";

export interface ConversationContentBlock {
  id: string;
  kind?: string;
  partId?: string;
  type: ConversationContentType;
  renderer?: ConversationCardRenderer;
  legacyAnchorIds?: string[];
  role: ConversationPartRole;
  text: string;
  format?: ConversationContentFormat;
  language?: string | null;
  cwd?: string | null;
  status?: string | null;
  exitCode?: number | null;
  translatedText?: string | null;
  commandLabel?: string | null;
  signal?: "ambient" | "focus" | string | null;
  filePath?: string | null;
  toolName?: string | null;
}

export type ConversationContentBlockSeed = {
  node_id: string;
  part_id: string;
  adapter_id?: string;
  kind: string;
  semantic_role?: string | null;
  renderer: ConversationCardRenderer;
  role: ConversationPartRole;
  body: string;
  language?: string | null;
  cwd?: string | null;
  status?: string | null;
  exit_code?: number | null;
  translated_body?: string | null;
  command_label?: string | null;
  source_execution_id?: string | null;
  legacy_anchor_ids: string[];
};

export type ConversationDisplayNode =
  | {
      type: "card";
      turnId: string;
      block: ConversationContentBlock;
    }
  | {
      type: "execution";
      turnId: string;
      sourceExecutionId: string;
      commands: ConversationContentBlock[];
      results: ConversationContentBlock[];
    };

export function conversationCardDomId(blockId: string) {
  return `conversation-card-${blockId}`;
}

const adapterBrowseMarkerPattern =
  /\s*\[AssetIWeave adapter (?:truncated \d+ characters for browsing|compacted low-signal tool output for browsing; original \d+ characters)\.\]/g;

export function buildConversationContentBlocks(
  parts: ConversationPart[],
  projectedSeeds?: ConversationContentBlockSeed[],
): ConversationContentBlock[] {
  if (projectedSeeds?.length) {
    return projectedSeeds.map(conversationContentBlockSeedToBlock);
  }
  return parts.flatMap(createDeclaredContentBlock);
}

export function buildConversationContentBlocksFromNodes(
  nodes: ConversationContentNode[],
): ConversationContentBlock[] {
  return nodes.map(conversationContentNodeToBlock);
}

/**
 * Groups projected nodes by their source execution without reconstructing a Card index.
 * A single source execution may contain multiple projected command nodes.
 */
export function buildConversationDisplayNodesFromNodes(
  nodes: ConversationContentNode[],
): ConversationDisplayNode[] {
  const displayNodes: ConversationDisplayNode[] = [];
  const executions = new Map<
    string,
    Extract<ConversationDisplayNode, { type: "execution" }>
  >();

  for (const node of nodes) {
    const block = conversationContentNodeToBlock(node);
    if (!node.source_execution_id) {
      displayNodes.push({ type: "card", turnId: node.turn_id, block });
      continue;
    }

    const executionKey = `${node.turn_id}:${node.source_execution_id}`;
    let execution = executions.get(executionKey);
    if (!execution) {
      execution = {
        type: "execution",
        turnId: node.turn_id,
        sourceExecutionId: node.source_execution_id,
        commands: [],
        results: [],
      };
      executions.set(executionKey, execution);
      displayNodes.push(execution);
    }

    if (block.type === "result") {
      execution.results.push(block);
    } else {
      execution.commands.push(block);
    }
  }

  return displayNodes;
}

export function buildConversationDisplayNodesFromBlocks(
  blocks: ConversationContentBlock[],
): ConversationDisplayNode[] {
  return blocks.map((block) => ({ type: "card", turnId: "", block }));
}

function conversationContentBlockSeedToBlock(
  card: ConversationContentBlockSeed,
): ConversationContentBlock {
  return {
    id: card.node_id,
    kind: card.kind,
    partId: card.part_id,
    type: conversationCardPresentationKind(card.kind, card.semantic_role),
    renderer: card.renderer,
    legacyAnchorIds: card.legacy_anchor_ids,
    role: card.role,
    text: card.body,
    language: card.language,
    cwd: card.cwd,
    status: card.status,
    exitCode: card.exit_code,
    commandLabel: card.command_label,
    translatedText: card.translated_body,
    format: card.renderer === "markdown" ? "markdown" : "plain",
    signal: card.renderer === "compact_action" || card.semantic_role === "ambient" ? "ambient" : undefined,
  };
}

function conversationContentNodeToBlock(
  node: ConversationContentNode,
): ConversationContentBlock {
  return {
    id: node.node_id,
    kind: node.node_type,
    partId: node.part_id,
    type: conversationCardPresentationKind(node.node_type, node.semantic_role),
    renderer: node.renderer,
    legacyAnchorIds: node.legacy_anchor_ids,
    role: node.role,
    text: node.content,
    language: node.language,
    cwd: node.cwd,
    status: node.status,
    exitCode: node.exit_code,
    commandLabel: node.command_label,
    translatedText: node.translated_content,
    format: node.renderer === "markdown" ? "markdown" : "plain",
    signal: node.renderer === "compact_action" || node.semantic_role === "ambient" ? "ambient" : undefined,
  };
}

interface ParsedCompactAction {
  action: string;
  target: string;
  icon: "search" | "read" | "tool";
  hasDetails: boolean;
  details?: string;
}

function parseCompactActionText(text: string, block: ConversationContentBlock): ParsedCompactAction {
  const trimmed = text.trim();
  const colonMatch = trimmed.match(/^([a-zA-Z0-9_-]+):\s*(.+)$/s);
  if (colonMatch) {
    const act = colonMatch[1];
    const rest = colonMatch[2].trim();
    const isSearch = /grep|search|find/i.test(act);
    const isRead = /view|read|cat|head|tail/i.test(act);
    return {
      action: act,
      target: rest.split("\n", 1)[0],
      icon: isSearch ? "search" : isRead ? "read" : "tool",
      hasDetails: rest.includes("\n"),
      details: rest.includes("\n") ? rest.slice(rest.indexOf("\n") + 1).trim() : undefined,
    };
  }

  if (trimmed.startsWith("Tool: ")) {
    const lines = trimmed.split("\n").filter(Boolean);
    const firstLine = lines[0].replace(/^Tool:\s*/, "").trim();
    const details = lines.slice(1).join("\n").trim();
    const isSearch = /grep|search|find/i.test(firstLine);
    const isRead = /view|read|cat|head|tail/i.test(firstLine);
    const pathMatch = details.match(/(?:AbsolutePath|path|TargetFile):\s*(.+)$/m);
    const target = pathMatch ? pathMatch[1].trim() : (block.filePath || firstLine);
    return {
      action: block.toolName || firstLine,
      target,
      icon: isSearch ? "search" : isRead ? "read" : "tool",
      hasDetails: details.length > 0,
      details: details.length > 0 ? details : undefined,
    };
  }

  const isSearch = /grep|search|find|rg/i.test(trimmed) || /search/i.test(block.kind || "");
  return {
    action: block.commandLabel || (isSearch ? "search" : "read"),
    target: block.filePath || trimmed.split("\n", 1)[0],
    icon: isSearch ? "search" : "read",
    hasDetails: trimmed.includes("\n"),
    details: trimmed.includes("\n") ? trimmed.slice(trimmed.indexOf("\n") + 1).trim() : undefined,
  };
}

function isAmbientBlock(block: ConversationContentBlock): boolean {
  if (block.renderer === "compact_action") return true;
  if (block.signal === "ambient") return true;
  const kind = (block.kind || "").toLowerCase();
  const cmd = (block.text || "").toLowerCase();
  if (kind.includes("read") || kind.includes("search")) return true;
  if (/^(?:view_file|read_file|grep_search|list_directory|read_url_content):/i.test(cmd)) return true;
  return false;
}

function isAmbientNode(node: ConversationDisplayNode): boolean {
  if (node.type === "card") {
    return isAmbientBlock(node.block);
  }
  return (
    node.commands.length > 0 &&
    node.commands.every(isAmbientBlock) &&
    node.results.every((r) => isAmbientBlock(r) || !r.text?.trim())
  );
}

function CompactActionCard({
  block,
  copied,
  onCopy,
  t,
}: {
  block: ConversationContentBlock;
  copied?: boolean;
  onCopy?: () => void;
  t: Translator;
}) {
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  const parsed = parseCompactActionText(block.text || block.commandLabel || "", block);
  const isSuccess = block.status !== "failed" && (!block.exitCode || block.exitCode === 0);

  return (
    <div
      className="group relative flex flex-col rounded-xl border border-theme-card-border/60 bg-theme-card/50 backdrop-blur-xs transition-all hover:border-theme-control-border hover:bg-theme-card/80 shadow-xs"
      data-content-type="compact_action"
      data-conversation-card-id={block.id}
      id={conversationCardDomId(block.id)}
    >
      <div className="flex h-9 items-center justify-between gap-2.5 px-3">
        <div className="flex min-w-0 items-center gap-2">
          {parsed.icon === "search" ? (
            <Search className="h-3.5 w-3.5 shrink-0 text-on-surface-muted group-hover:text-primary transition-colors" />
          ) : (
            <Eye className="h-3.5 w-3.5 shrink-0 text-on-surface-muted group-hover:text-primary transition-colors" />
          )}
          <span className="shrink-0 rounded-full bg-theme-control/60 px-2 py-0.5 text-label-caps font-mono text-on-surface-variant">
            {parsed.action}
          </span>
          <span
            className="truncate font-mono text-code-xs text-on-surface/90"
            title={parsed.target}
          >
            {parsed.target}
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {isSuccess ? (
            <span className="h-1.5 w-1.5 rounded-full bg-status-create" title="Success" />
          ) : (
            <span className="flex items-center gap-1 font-mono text-label-caps text-status-remove">
              <span className="h-1.5 w-1.5 rounded-full bg-status-remove" />
              error
            </span>
          )}
          {onCopy && (
            <button
              className="opacity-0 group-hover:opacity-100 rounded p-1 text-on-surface-muted hover:text-on-surface hover:bg-theme-control/50 transition-all"
              onClick={onCopy}
              title={copied ? t("conversation.content.copied") : t("common.copy")}
              type="button"
            >
              {copied ? <Check className="h-3 w-3 text-status-create" /> : <Copy className="h-3 w-3" />}
            </button>
          )}
          {parsed.hasDetails && (
            <button
              className="rounded p-1 text-on-surface-muted hover:text-on-surface hover:bg-theme-control/50 transition-all"
              onClick={() => setDetailsExpanded((prev) => !prev)}
              type="button"
            >
              {detailsExpanded ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
            </button>
          )}
        </div>
      </div>
      {detailsExpanded && parsed.details ? (
        <div className="border-t border-theme-card-border/40 px-3 py-2 text-code-xs font-mono text-on-surface-variant bg-theme-card/30 whitespace-pre-wrap break-all">
          {parsed.details}
        </div>
      ) : null}
    </div>
  );
}

function AmbientActionGroup({
  nodes,
  renderNode,
  t,
}: {
  nodes: ConversationDisplayNode[];
  renderNode: (node: ConversationDisplayNode) => ReactNode;
  t: Translator;
}) {
  const [expanded, setExpanded] = useState(false);
  const count = nodes.length;

  return (
    <div className="flex flex-col gap-1.5 rounded-2xl border border-theme-card-border/70 bg-theme-card/35 p-2 backdrop-blur-xs transition-all shadow-xs">
      <div
        className="flex h-8 cursor-pointer items-center justify-between rounded-xl px-2.5 transition-colors hover:bg-theme-card/60"
        onClick={() => setExpanded((prev) => !prev)}
      >
        <div className="flex items-center gap-2">
          <Layers className="h-3.5 w-3.5 text-on-surface-muted" />
          <span className="text-body-xs font-medium text-on-surface-variant">
            {t("conversation.content.ambientActionCount", { count })}
          </span>
        </div>
        <button
          className="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-body-xs font-medium text-theme-control-fg hover:bg-theme-control/40 transition-colors"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((prev) => !prev);
          }}
          type="button"
        >
          <span>{expanded ? t("common.collapse") : t("common.expand")}</span>
          {expanded ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
        </button>
      </div>
      {expanded ? (
        <div className="flex flex-col gap-1.5 pt-1">
          {nodes.map((n, idx) => (
            <div key={idx}>{renderNode(n)}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function ConversationContentCards({
  activeBlockId,
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  controller,
  onCopyError,
  onCommandPartsVisible,
  onTranslationError,
  nodes,
  recordKind = "session",
  resultPreviewLineLimit = DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  t,
  translationAvailabilityChecker,
  translationSaver = updateConversationPartTranslation,
  translationSettings = DEFAULT_TRANSLATION_SETTINGS,
  translationTaskController,
  translator,
  visibility,
}: {
  activeBlockId?: string | null;
  colors?: ConversationContentCardColorSettings;
  controller?: ConversationContentController;
  onCopyError?: (message: string) => void;
  onCommandPartsVisible?: (partIds: string[]) => void;
  onTranslationError?: (message: string) => void;
  nodes: ConversationDisplayNode[];
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  t: Translator;
  translationAvailabilityChecker?: () => Promise<OpencodeTranslationAvailability>;
  translationSaver?: (
    request: ConversationPartTranslationUpdateRequest,
  ) => Promise<void>;
  translationSettings?: ResolvedConversationTranslationSettings;
  translationTaskController?: ConversationTranslationTaskController;
  translator?: (
    request: ConversationCardTranslationRequest,
  ) => Promise<OpencodeTranslationResult>;
  visibility: ConversationContentVisibility;
}) {
  const displayNodes = nodes;
  const visibleNodes = displayNodes.flatMap(
    (node): ConversationDisplayNode[] => {
      if (node.type === "card") {
        return (visibility[node.block.type] ?? true) &&
          shouldDisplayContentBlock(node.block)
          ? [node]
          : [];
      }
      const commands = node.commands.filter(
        (block) => visibility[block.type] ?? true,
      );
      const results = node.results.filter(
        (block) =>
          (visibility[block.type] ?? true) && shouldDisplayContentBlock(block),
      );
      if (commands.length === 1 && results.length === 0) {
        return commands.map((command) => ({
          type: "card",
          turnId: node.turnId,
          block: command,
        }));
      }
      return commands.length > 0 || results.length > 0
        ? [{ ...node, commands, results }]
        : [];
    },
  );
  const [nodeRenderLimit, setNodeRenderLimit] = useState(12);
  const nodeLoadMoreRef = useRef<HTMLDivElement>(null);
  const renderedNodes = visibleNodes.slice(0, nodeRenderLimit);
  const hasMoreNodes = renderedNodes.length < visibleNodes.length;
  const visibleBlocks = renderedNodes.flatMap((node) =>
    node.type === "card" ? [node.block] : [...node.commands, ...node.results],
  );

  useEffect(() => {
    if (!onCommandPartsVisible) return;
    const partIds = new Set<string>();
    for (const node of renderedNodes) {
      const commands = node.type === "card" ? [node.block] : node.commands;
      for (const command of commands) {
        if (command.type === "command" && command.partId)
          partIds.add(command.partId);
      }
    }
    if (partIds.size > 0) onCommandPartsVisible([...partIds]);
  }, [onCommandPartsVisible, renderedNodes]);

  useEffect(() => {
    if (!hasMoreNodes || typeof IntersectionObserver === "undefined") return;
    const target = nodeLoadMoreRef.current;
    if (!target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        setNodeRenderLimit((current) =>
          Math.min(current + 12, visibleNodes.length),
        );
      },
      { rootMargin: "480px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [hasMoreNodes, renderedNodes.length, visibleNodes.length]);
  const fallbackController = useConversationContentController({
    blocks: visibleBlocks,
    enabled: controller == null,
    onCopyError,
    onTranslationError,
    recordKind,
    t,
    translationAvailabilityChecker,
    translationSaver,
    translationSettings,
    translationTaskController,
    translator,
  });
  const contentController = controller ?? fallbackController;

  if (visibleBlocks.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">
        {t("conversation.content.hidden")}
      </div>
    );
  }

  const groupedDisplayItems: (
    | { type: "single"; node: ConversationDisplayNode }
    | { type: "ambient_group"; key: string; nodes: ConversationDisplayNode[] }
  )[] = [];

  let currentAmbientGroup: ConversationDisplayNode[] = [];
  for (const node of renderedNodes) {
    if (isAmbientNode(node)) {
      currentAmbientGroup.push(node);
    } else {
      if (currentAmbientGroup.length >= 2) {
        groupedDisplayItems.push({
          type: "ambient_group",
          key: `ambient-group-${groupedDisplayItems.length}`,
          nodes: [...currentAmbientGroup],
        });
      } else if (currentAmbientGroup.length === 1) {
        groupedDisplayItems.push({ type: "single", node: currentAmbientGroup[0] });
      }
      currentAmbientGroup = [];
      groupedDisplayItems.push({ type: "single", node });
    }
  }
  if (currentAmbientGroup.length >= 2) {
    groupedDisplayItems.push({
      type: "ambient_group",
      key: `ambient-group-${groupedDisplayItems.length}`,
      nodes: [...currentAmbientGroup],
    });
  } else if (currentAmbientGroup.length === 1) {
    groupedDisplayItems.push({ type: "single", node: currentAmbientGroup[0] });
  }

  return (
    <div className="grid gap-3">
      {groupedDisplayItems.map((item) => {
        if (item.type === "ambient_group") {
          return (
            <AmbientActionGroup
              key={item.key}
              nodes={item.nodes}
              renderNode={renderDisplayNode}
              t={t}
            />
          );
        }
        return renderDisplayNode(item.node);
      })}
      {hasMoreNodes ? (
        <div className="flex justify-center py-2" ref={nodeLoadMoreRef}>
          <button
            className="rounded-md border border-theme-control-border bg-theme-control/80 px-3 py-1.5 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover"
            onClick={() =>
              setNodeRenderLimit((current) =>
                Math.min(current + 12, visibleNodes.length),
              )
            }
            type="button"
          >
            {t("conversation.content.loadMoreActivities")}
          </button>
        </div>
      ) : null}
    </div>
  );

  function renderDisplayNode(node: ConversationDisplayNode) {
    if (node.type === "card") {
      if (isAmbientBlock(node.block)) {
        return (
          <CompactActionCard
            block={node.block}
            copied={contentController.isCopied(node.block.id)}
            key={node.block.id}
            onCopy={() => void contentController.copyBlock(node.block)}
            t={t}
          />
        );
      }
      return renderContentCard(node.block);
    }
    const executionKey = `${node.turnId}:${node.sourceExecutionId}`;
    if (isAmbientNode(node)) {
      const firstCommand = node.commands[0];
      if (firstCommand) {
        return (
          <CompactActionCard
            block={firstCommand}
            copied={contentController.isCopied(firstCommand.id)}
            key={executionKey}
            onCopy={() => void contentController.copyBlock(firstCommand)}
            t={t}
          />
        );
      }
    }
    return (
      <ConversationExecutionContent
        key={executionKey}
        node={node}
        renderContentCard={renderContentCard}
        t={t}
      />
    );
  }

  function renderContentCard(block: ConversationContentBlock) {
    return (
      <ConversationContentCard
        block={block}
        colors={colors}
        copied={contentController.isCopied(block.id)}
        expanded={contentController.expandedBlockIds.has(block.id)}
        highlighted={
          activeBlockId === block.id ||
          block.legacyAnchorIds?.includes(activeBlockId ?? "") === true
        }
        key={block.id}
        onCopy={() => void contentController.copyBlock(block)}
        onCancelTranslation={() =>
          void contentController.cancelTranslation(block.id)
        }
        onToggleExpanded={() => contentController.toggleExpanded(block.id)}
        onTranslate={() => void contentController.translateBlock(block)}
        resultPreviewLineLimit={resultPreviewLineLimit}
        t={t}
        translatedText={contentController.getTranslatedText(block)}
        translating={contentController.isTranslating(block.id)}
        translationAvailability={contentController.translationAvailability}
        translationPhase={contentController.getTranslationPhase(block.id)}
        translationTargetLanguage={translationSettings.targetLanguage}
      />
    );
  }
}

const COMMAND_RENDER_BATCH_SIZE = 6;
const RESULT_RENDER_BATCH_SIZE = 6;

function ConversationExecutionContent({
  node,
  renderContentCard,
  t,
}: {
  node: Extract<ConversationDisplayNode, { type: "execution" }>;
  renderContentCard: (block: ConversationContentBlock) => ReactNode;
  t: Translator;
}) {
  const [expanded, setExpanded] = useState(false);
  const [renderLimit, setRenderLimit] = useState(COMMAND_RENDER_BATCH_SIZE);
  const [resultRenderLimit, setResultRenderLimit] = useState(
    RESULT_RENDER_BATCH_SIZE,
  );
  const loadMoreRef = useRef<HTMLDivElement>(null);
  const loadMoreResultsRef = useRef<HTMLDivElement>(null);
  const hasFoldedCommands = node.commands.length > 1;
  const visibleCommandCount = expanded
    ? Math.min(renderLimit, node.commands.length)
    : Math.min(1, node.commands.length);
  const hasMore = expanded && visibleCommandCount < node.commands.length;
  const visibleResultCount = Math.min(resultRenderLimit, node.results.length);
  const hasMoreResults = visibleResultCount < node.results.length;

  useEffect(() => {
    if (!hasMore || typeof IntersectionObserver === "undefined") return;
    const target = loadMoreRef.current;
    if (!target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        setRenderLimit((current) =>
          Math.min(current + COMMAND_RENDER_BATCH_SIZE, node.commands.length),
        );
      },
      { rootMargin: "320px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [hasMore, node.commands.length, visibleCommandCount]);

  useEffect(() => {
    if (!hasMoreResults || typeof IntersectionObserver === "undefined") return;
    const target = loadMoreResultsRef.current;
    if (!target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        setResultRenderLimit((current) =>
          Math.min(current + RESULT_RENDER_BATCH_SIZE, node.results.length),
        );
      },
      { rootMargin: "320px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [hasMoreResults, node.results.length, visibleResultCount]);

  return (
    <>
      {node.commands.slice(0, visibleCommandCount).map(renderContentCard)}
      {hasFoldedCommands ? (
        <div className="flex justify-end py-1">
          <button
            aria-expanded={expanded}
            className="inline-flex items-center gap-1.5 rounded-md border border-theme-control-border bg-theme-control/80 px-2.5 py-1 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover"
            onClick={() => {
              setExpanded((current) => !current);
              setRenderLimit(COMMAND_RENDER_BATCH_SIZE);
            }}
            type="button"
          >
            {expanded ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
            {expanded
              ? t("conversation.content.collapseCommands")
              : t("conversation.content.expandCommands", {
                  count: node.commands.length - 1,
                })}
          </button>
        </div>
      ) : null}
      {hasMore ? (
        <div className="flex justify-center py-1" ref={loadMoreRef}>
          <button
            className="rounded-md border border-theme-control-border bg-theme-control/80 px-3 py-1.5 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover"
            onClick={() =>
              setRenderLimit((current) =>
                Math.min(
                  current + COMMAND_RENDER_BATCH_SIZE,
                  node.commands.length,
                ),
              )
            }
            type="button"
          >
            {t("conversation.content.loadMoreCommands")}
          </button>
        </div>
      ) : null}
      {node.results.slice(0, visibleResultCount).map(renderContentCard)}
      {hasMoreResults ? (
        <div className="flex justify-center py-1" ref={loadMoreResultsRef}>
          <button
            className="rounded-md border border-theme-control-border bg-theme-control/80 px-3 py-1.5 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover"
            onClick={() =>
              setResultRenderLimit((current) =>
                Math.min(
                  current + RESULT_RENDER_BATCH_SIZE,
                  node.results.length,
                ),
              )
            }
            type="button"
          >
            {t("conversation.content.loadMoreResults")}
          </button>
        </div>
      ) : null}
    </>
  );
}

const DEFAULT_TRANSLATION_SETTINGS: ResolvedConversationTranslationSettings = {
  agentId: "opencode",
  model: "",
  promptTemplate: DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  provider: "cli",
  targetLanguage: DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
};

function ConversationContentCard({
  block,
  colors,
  copied,
  expanded,
  highlighted,
  onCancelTranslation,
  onCopy,
  onToggleExpanded,
  onTranslate,
  resultPreviewLineLimit,
  t,
  translatedText,
  translating,
  translationAvailability,
  translationPhase,
  translationTargetLanguage,
}: {
  block: ConversationContentBlock;
  colors: ConversationContentCardColorSettings;
  copied: boolean;
  expanded: boolean;
  highlighted: boolean;
  onCancelTranslation: () => void;
  onCopy: () => void;
  onToggleExpanded: () => void;
  onTranslate: () => void;
  resultPreviewLineLimit: number;
  t: Translator;
  translatedText?: string;
  translating: boolean;
  translationAvailability: TranslationAvailabilityStatus;
  translationPhase?: AiExecutionPhase;
  translationTargetLanguage: ConversationTranslationTargetLanguage;
}) {
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  const { definitions } = useConversationCardKindRegistry();
  const definition =
    block.kind && block.kind === block.type
      ? definitions.get(block.kind)
      : undefined;
  const label = definition?.label ?? conversationCardLabel(block.type, t);
  const role = t(`conversation.part.role.${block.role}` as TranslationKey);
  const accentColor = conversationCardColor(block.type, colors, definition?.semantic_role);
  const copyLabel = copied
    ? t("conversation.content.copied")
    : t("conversation.content.copy", { type: label });
  const translationTargetLabel = normalizeConversationTranslationTargetLanguage(
    translationTargetLanguage,
  );
  const translateDisabled =
    renderer === "path" ||
    translationAvailability !== "available" ||
    translating;
  const translateLabel = translationButtonLabel({
    hasTranslation: Boolean(translatedText),
    label,
    status: translationAvailability,
    t,
    targetLanguage: translationTargetLabel,
    translating,
  });
  const resultPresentation =
    block.type === "result" ? describeResultPresentation(block) : undefined;
  const diffSummary =
    renderer === "diff" ? summarizeConversationDiff(block.text) : undefined;
  const preview =
    resultPresentation?.type === "file-change"
      ? {
          hasOverflow: false,
          lines: [],
          visibleLineCount: 0,
          visibleValue: "",
        }
      : buildConversationCardPreview(
          block.text,
          resultPreviewLineLimit,
          expanded,
        );
  const canExpandDiff =
    resultPresentation?.type === "file-change" && block.text.trim().length > 0;
  const canExpandResult =
    canExpandDiff ||
    (resultPresentation?.type !== "success" && preview.hasOverflow);

  return (
    <section
      className={`scroll-mt-32 overflow-hidden rounded-xl border transition-shadow ${
        highlighted
          ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]"
          : ""
      }`}
      data-content-type={block.type}
      data-conversation-card-id={block.id}
      id={conversationCardDomId(block.id)}
      style={{
        backgroundColor: withAlpha(accentColor, "12"),
        borderColor: withAlpha(accentColor, "66"),
      }}
    >
      <header className="conversation-content-header flex flex-wrap items-center justify-between gap-2 px-4 py-2.5">
        <div
          className="flex min-w-0 flex-wrap items-center gap-2 text-label-caps"
          style={{ color: accentColor }}
        >
          {isSuccessfulCommand(block) ? (
            <CheckCircle2 aria-hidden="true" size={15} />
          ) : (
            <ConversationCardKindIcon
              iconHint={definition?.icon_hint}
              kind={block.type}
              renderer={renderer}
            />
          )}
          <span>{label}</span>
          {block.commandLabel ? (
            <span
              className="max-w-48 truncate rounded-sm border border-status-create/55 bg-status-create/10 px-2 py-0.5 text-label-caps text-status-create"
              data-command-label={block.commandLabel}
              title={block.commandLabel}
            >
              {block.commandLabel}
            </span>
          ) : null}
          {block.type === "command" && block.exitCode != null ? (
            <span className="rounded-sm border border-inherit bg-theme-card/45 px-2 py-0.5 font-mono text-code-sm normal-case text-on-surface-variant">
              {t("conversation.content.exitCode", { code: block.exitCode })}
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-1.5 text-label-caps">
          <span
            className="select-text rounded-md border border-inherit bg-theme-card/45 px-1.5 py-0.5 font-mono text-code-sm normal-case text-on-surface-muted"
            title={block.partId ?? block.id}
          >
            {conversationIdFragment(block.partId ?? block.id)}
          </span>
          <span className="text-label-caps text-on-surface-muted">{role}</span>
          <button
            aria-label={copyLabel}
            className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-on-surface-muted transition-colors hover:text-on-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
            onClick={onCopy}
            title={copyLabel}
            type="button"
          >
            {copied ? (
              <Check className="size-[1em]" />
            ) : (
              <Copy className="size-[1em]" />
            )}
          </button>
          <button
            aria-label={translateLabel}
            className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-on-surface-muted transition-colors enabled:hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
            disabled={translateDisabled}
            onClick={onTranslate}
            title={translateLabel}
            type="button"
          >
            <Languages
              className={
                translating ? "size-[1em] animate-pulse" : "size-[1em]"
              }
            />
          </button>
        </div>
      </header>
      <div className="px-4 py-3">
        <ConversationCardBody
          block={block}
          label={label}
          text={
            resultPresentation?.type === "file-change"
              ? block.text
              : preview.visibleValue
          }
          expanded={expanded}
          diffSummary={diffSummary}
          resultPresentation={resultPresentation}
          t={t}
        />
        {canExpandResult ? (
          <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-inherit bg-theme-card/35 px-3 py-2">
            {canExpandDiff ? (
              <span className="text-code-sm text-on-surface-muted">
                {t("conversation.content.diffSummaryFiles", {
                  count: resultPresentation.summary.files.length,
                })}
              </span>
            ) : (
              <span className="text-code-sm text-on-surface-muted">
                {t("conversation.content.resultPreviewLines", {
                  shown: preview.visibleLineCount,
                  total: preview.lines.length,
                })}
              </span>
            )}
            <button
              aria-expanded={expanded}
              className="rounded-xl border border-theme-control-border bg-theme-control/80 px-2.5 py-1 text-body-sm font-semibold text-theme-control-fg transition-[transform,background-color,border-color,color] duration-200 hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0"
              onClick={onToggleExpanded}
              type="button"
            >
              {expanded
                ? t("conversation.content.collapseResult")
                : canExpandDiff
                  ? t("conversation.content.viewDiff")
                  : t("conversation.content.expandResult")}
            </button>
          </div>
        ) : null}
        {translatedText ? (
          <div className="mt-3 rounded-xl border border-inherit bg-theme-card/45 px-3 py-3">
            <div className="mb-2 text-label-caps text-on-surface-muted">
              {t("conversation.content.translation", {
                language: translationTargetLabel,
              })}
            </div>
            <MarkdownContent value={translatedText} />
          </div>
        ) : null}
        {translationPhase ? (
          <div
            className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-inherit bg-theme-card/35 px-3 py-2 text-body-sm text-on-surface-variant"
            data-testid={`translation-progress-${block.id}`}
            role="status"
          >
            <span>{translationPhaseLabel(translationPhase, t)}</span>
            <button
              aria-label={t("conversation.content.translationCancel")}
              className="rounded-md border border-theme-control-border bg-theme-control/80 px-2.5 py-1 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover disabled:cursor-not-allowed disabled:opacity-50"
              disabled={
                translationPhase === "cancelling" ||
                translationPhase === "cleaning_up"
              }
              onClick={onCancelTranslation}
              type="button"
            >
              {translationPhase === "cancelling"
                ? t("conversation.content.translationCancelling")
                : t("common.cancel")}
            </button>
          </div>
        ) : null}
        <BlockMetadata block={block} t={t} />
      </div>
    </section>
  );
}

function ConversationCardBody({
  block,
  diffSummary,
  expanded,
  label,
  resultPresentation,
  text,
  t,
}: {
  block: ConversationContentBlock;
  diffSummary?: ReturnType<typeof summarizeConversationDiff>;
  expanded: boolean;
  label: string;
  resultPresentation?: ConversationResultPresentation;
  text: string;
  t: Translator;
}) {
  if (resultPresentation) {
    return (
      <ConversationResultBody
        block={block}
        expanded={expanded}
        diffSummary={diffSummary}
        presentation={resultPresentation}
        text={text}
        t={t}
      />
    );
  }

  return (
    <ConversationStandardCardBody
      block={block}
      diffSummary={diffSummary}
      expanded={expanded}
      label={label}
      text={text}
      t={t}
    />
  );
}

interface SubagentPayload {
  agent_role?: string;
  task?: string;
  child_session_id?: string;
  status?: string;
  model?: string;
  type_name?: string;
  total_subagents?: number;
}

function parseSubagentPayload(text: string): { payload: SubagentPayload | null; raw: string } {
  try {
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return { payload: parsed as SubagentPayload, raw: text };
    }
  } catch {
    // not JSON
  }
  return { payload: null, raw: text };
}

function SubagentTreeCard({
  block,
  label,
  text,
  t,
}: {
  block: ConversationContentBlock;
  label: string;
  text: string;
  t: Translator;
}) {
  const [open, setOpen] = useState(true);
  const { payload } = parseSubagentPayload(text);
  const roleName = payload?.agent_role || payload?.type_name || label || "Subagent";
  const taskText = payload?.task || (payload ? "" : text);
  const status = payload?.status || block.status || "completed";
  const childSessionId = payload?.child_session_id;

  return (
    <div className="rounded-2xl border border-border/60 bg-theme-card/45 p-3.5 space-y-3 transition-[border-color,background-color] duration-150">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="grid size-7 shrink-0 place-items-center rounded-xl bg-theme-control text-on-surface-variant border border-border/40">
            <GitFork size={14} />
          </span>
          <div className="flex items-center gap-2 min-w-0">
            <span className="truncate text-body-sm font-semibold text-on-surface">
              {roleName}
            </span>
            <span className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-label-caps bg-theme-control/80 text-on-surface-variant border border-border/30">
              <span className="size-1.5 rounded-full bg-status-success" />
              <span>{status}</span>
            </span>
          </div>
        </div>
        <button
          type="button"
          aria-label={open ? "Collapse subagent details" : "Expand subagent details"}
          onClick={() => setOpen((prev) => !prev)}
          className="inline-flex size-7 shrink-0 items-center justify-center rounded-lg text-on-surface-variant hover:bg-theme-control-hover/70 transition-colors duration-150"
        >
          <ChevronDown
            size={15}
            className={`transition-transform duration-200 ${open ? "rotate-180" : ""}`}
          />
        </button>
      </div>

      {taskText ? (
        <div className="rounded-xl border border-border/40 bg-theme-card/30 p-2.5 text-body-sm text-on-surface-variant">
          <div className="text-label-caps text-on-surface-muted mb-1 font-medium">Task</div>
          {block.format === "markdown" ? (
            <MarkdownContent value={taskText} />
          ) : (
            <div className="whitespace-pre-wrap break-words text-body-sm leading-relaxed">{taskText}</div>
          )}
        </div>
      ) : null}

      {childSessionId ? (
        <div className="flex items-center gap-2 text-body-sm">
          <span className="text-on-surface-muted text-label-caps">Child Session:</span>
          <span className="font-mono text-code-sm text-on-surface-variant bg-theme-control px-2 py-0.5 rounded-md border border-border/30">
            {childSessionId}
          </span>
        </div>
      ) : null}

      {open && (!taskText || payload) ? (
        <div className="border-t border-border/30 pt-2.5 text-code-sm text-on-surface-variant">
          <pre className="max-h-[24rem] overflow-auto whitespace-pre-wrap break-words rounded-xl bg-theme-card/25 p-2.5 font-mono leading-5">
            <code>{text}</code>
          </pre>
        </div>
      ) : null}
    </div>
  );
}

function ConversationStandardCardBody({
  block,
  diffSummary,
  expanded,
  label,
  text,
  t,
}: {
  block: ConversationContentBlock;
  diffSummary?: ReturnType<typeof summarizeConversationDiff>;
  expanded: boolean;
  label: string;
  text: string;
  t: Translator;
}) {
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  switch (renderer) {
    case "diff":
      return <ConversationDiff summary={diffSummary} value={text} />;
    case "markdown":
      return <MarkdownContent value={text} />;
    case "terminal_output":
      if (block.format === "markdown") {
        return (
          <div
            className="rounded-xl border border-inherit bg-theme-card/45 px-3 py-3"
            data-result-format="markdown"
          >
            <MarkdownContent value={text} />
          </div>
        );
      }
      return (
        <pre className="max-h-[38rem] overflow-auto whitespace-pre-wrap break-words rounded-xl border border-inherit bg-theme-card/45 p-3 text-code-sm leading-6 text-on-surface">
          <code>{text}</code>
        </pre>
      );
    case "path":
      return <LocalPathCardBody label={label} path={text} t={t} />;
    case "json":
    case "code":
      return (
        <pre className="overflow-auto whitespace-pre-wrap break-words text-code-sm leading-6 text-on-surface">
          <code>{text}</code>
        </pre>
      );
    case "command":
    case "plain":
      return (
        <pre className="overflow-auto whitespace-pre-wrap break-words text-code-sm leading-6 text-on-surface">
          <code>{text}</code>
        </pre>
      );
    case "compact_action":
      return (
        <CompactActionCard
          block={block}
          t={t}
        />
      );
    case "accordion":
      return (
        <details className="group rounded-xl border border-inherit bg-theme-card/35 p-3 transition-colors duration-150 open:bg-theme-card/55">
          <summary className="cursor-pointer list-none flex items-center justify-between text-body-sm font-medium text-on-surface select-none">
            <span>{label}</span>
            <ChevronDown className="size-4 text-on-surface-variant transition-transform duration-200 group-open:rotate-180" />
          </summary>
          <div className="mt-2.5 pt-2 border-t border-border/40 text-body-sm text-on-surface-variant">
            {block.format === "markdown" ? (
              <MarkdownContent value={text} />
            ) : (
              <pre className="overflow-auto whitespace-pre-wrap break-words text-code-sm leading-6 text-on-surface">
                <code>{text}</code>
              </pre>
            )}
          </div>
        </details>
      );
    case "subagent_tree":
      return (
        <SubagentTreeCard
          block={block}
          label={label}
          text={text}
          t={t}
        />
      );
    default: {
      let prettyText = text;
      try {
        prettyText = JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        // keep text as is
      }
      return (
        <div className="rounded-xl border border-border/50 bg-theme-card/30 p-3 space-y-1.5 text-body-sm">
          <div className="flex items-center gap-1.5 text-label-caps text-on-surface-variant font-mono">
            <span>[inspect: {String(renderer)}]</span>
          </div>
          <pre className="max-h-[30rem] overflow-auto whitespace-pre-wrap break-words text-code-sm leading-5 text-on-surface font-mono">
            <code>{prettyText}</code>
          </pre>
        </div>
      );
    }
  }
}

type ConversationResultPresentation =
  | {
      type: "file-change";
      summary: ReturnType<typeof summarizeConversationDiff>;
    }
  | {
      type: "success";
    }
  | {
      type: "failure";
    };

function describeResultPresentation(
  block: ConversationContentBlock,
): ConversationResultPresentation | undefined {
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  if (renderer === "diff") {
    return {
      type: "file-change",
      summary: summarizeConversationDiff(block.text),
    };
  }
  if (isSuccessfulResult(block)) return { type: "success" };
  if (isFailedResult(block)) return { type: "failure" };
  return undefined;
}

function ConversationResultBody({
  block,
  diffSummary,
  expanded,
  presentation,
  text,
  t,
}: {
  block: ConversationContentBlock;
  diffSummary?: ReturnType<typeof summarizeConversationDiff>;
  expanded: boolean;
  presentation: ConversationResultPresentation;
  text: string;
  t: Translator;
}) {
  if (presentation.type === "file-change") {
    return expanded ? (
      <ConversationDiff
        summary={diffSummary ?? presentation.summary}
        value={block.text}
      />
    ) : (
      <FileChangeResultSummary summary={presentation.summary} t={t} />
    );
  }

  if (presentation.type === "success") {
    return (
      <div
        className="flex items-center gap-2 rounded-xl border border-status-create/30 bg-status-create/10 px-3 py-2.5 text-body-sm text-status-create"
        data-result-summary="success"
      >
        <CheckCircle2 aria-hidden="true" size={16} />
        <span>{t("conversation.content.resultSuccess")}</span>
        {block.exitCode != null ? (
          <span>
            · {t("conversation.content.exitCode", { code: block.exitCode })}
          </span>
        ) : null}
      </div>
    );
  }

  return (
    <div className="grid gap-2" data-result-summary="failure">
      <div className="flex items-center gap-2 rounded-xl border border-status-remove/30 bg-status-remove/10 px-3 py-2.5 text-body-sm text-status-remove">
        <XCircle aria-hidden="true" size={16} />
        <span>{t("conversation.content.resultFailed")}</span>
        {block.exitCode != null ? (
          <span>
            · {t("conversation.content.exitCode", { code: block.exitCode })}
          </span>
        ) : null}
      </div>
      {text ? (
        <pre className="max-h-[24rem] overflow-auto whitespace-pre-wrap break-words rounded-xl border border-status-remove/20 bg-status-remove/[0.06] p-3 text-code-sm leading-6 text-on-surface">
          <code>{text}</code>
        </pre>
      ) : null}
    </div>
  );
}

function FileChangeResultSummary({
  summary,
  t,
}: {
  summary: ReturnType<typeof summarizeConversationDiff>;
  t: Translator;
}) {
  return (
    <div className="grid gap-2" data-result-summary="file-change">
      <div className="flex items-center gap-2 text-body-sm font-semibold text-on-surface">
        <GitCompareArrows
          aria-hidden="true"
          size={16}
          className="text-status-update"
        />
        <span>
          {t("conversation.content.changedFiles", {
            count: summary.files.length,
          })}
          <span className="ml-2 font-mono text-code-sm font-normal text-status-create">
            +{summary.additions}
          </span>
          <span className="ml-1 font-mono text-code-sm font-normal text-status-remove">
            -{summary.deletions}
          </span>
        </span>
      </div>
      {summary.files.length > 0 ? (
        <div className="grid gap-1.5 overflow-hidden rounded-xl border border-theme-card-border/55 bg-theme-card/35 p-1.5">
          {summary.files.map((file) => (
            <div
              className="flex items-center gap-3 rounded-md bg-theme-card-header/35 px-3 py-2 font-mono text-code-sm"
              data-diff-summary-file={file.path}
              key={file.path}
            >
              <span className="w-4 shrink-0 text-center text-on-surface-muted">
                {fileStatusMark(file.status, file.binary)}
              </span>
              <span
                className="min-w-0 flex-1 truncate text-on-surface"
                title={file.path}
              >
                {file.path}
              </span>
              <span className="shrink-0 text-status-create">
                +{file.additions}
              </span>
              <span className="shrink-0 text-status-remove">
                -{file.deletions}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-theme-card-border bg-theme-card/45 px-3 py-2 text-code-sm text-on-surface-muted">
          {t("conversation.content.diffSummaryUnavailable")}
        </div>
      )}
    </div>
  );
}

function fileStatusMark(
  status: "added" | "deleted" | "modified" | "renamed",
  binary: boolean,
) {
  if (binary) return "B";
  if (status === "added") return "A";
  if (status === "deleted") return "D";
  if (status === "renamed") return "R";
  return "M";
}

function LocalPathCardBody({
  label,
  path,
  t,
}: {
  label: string;
  path: string;
  t: Translator;
}) {
  const [error, setError] = useState<string | null>(null);
  const revealLabel = t("conversation.content.revealPath", { type: label });

  async function handleReveal() {
    setError(null);
    try {
      await revealPath(path);
    } catch (revealError) {
      setError(errorMessage(revealError));
    }
  }

  return (
    <div className="grid gap-2">
      <button
        aria-label={revealLabel}
        className="flex min-w-0 items-center rounded-xl border border-inherit bg-theme-card/45 px-3 py-2.5 text-left font-mono text-code-sm text-primary transition-[transform,background-color,border-color,color] duration-200 hover:-translate-y-px hover:bg-theme-control hover:text-primary-strong active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
        onClick={() => void handleReveal()}
        title={path}
        type="button"
      >
        <span className="truncate">{abbreviateHomePath(path)}</span>
      </button>
      {error ? (
        <div className="text-body-sm text-status-remove" role="alert">
          {t("conversation.content.revealPathFailed", { message: error })}
        </div>
      ) : null}
    </div>
  );
}

const builtInCardKinds = new Set([
  "answer",
  "tool",
  "command",
  "code",
  "result",
  "ambient",
  "subagent",
]);

export function conversationCardLabel(kind: string, t: Translator) {
  if (builtInCardKinds.has(kind)) {
    return t(`conversation.content.${kind}` as TranslationKey);
  }
  const segments = kind.split(".");
  const leaf = segments[segments.length - 1] ?? kind;
  return (
    leaf
      .split(/[-_]+/)
      .filter(Boolean)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(" ") || kind
  );
}

export function conversationCardColor(
  kind: string,
  colors: ConversationContentCardColorSettings,
  semanticRole?: string | null,
) {
  if (colors[kind]) return colors[kind];
  if (semanticRole && colors[semanticRole]) return colors[semanticRole];

  const leaf = kind.includes(".") ? kind.slice(kind.lastIndexOf(".") + 1) : kind;
  if (colors[leaf]) return colors[leaf];

  if (semanticRole && SEMANTIC_FALLBACK_CARD_COLORS[semanticRole]) {
    return SEMANTIC_FALLBACK_CARD_COLORS[semanticRole];
  }
  if (SEMANTIC_FALLBACK_CARD_COLORS[leaf]) {
    return SEMANTIC_FALLBACK_CARD_COLORS[leaf];
  }
  if (SEMANTIC_FALLBACK_CARD_COLORS[kind]) {
    return SEMANTIC_FALLBACK_CARD_COLORS[kind];
  }

  return DEFAULT_FALLBACK_CARD_COLOR;
}

function translationButtonLabel({
  hasTranslation,
  label,
  status,
  t,
  targetLanguage,
  translating,
}: {
  hasTranslation: boolean;
  label: string;
  status: TranslationAvailabilityStatus;
  t: Translator;
  targetLanguage: string;
  translating: boolean;
}) {
  if (translating) {
    return t("conversation.content.translating");
  }
  if (status === "checking" || status === "idle") {
    return t("conversation.content.translationChecking");
  }
  if (status === "unavailable") {
    return t("conversation.content.translationUnavailable");
  }
  return hasTranslation
    ? t("conversation.content.retranslate", {
        language: targetLanguage,
        type: label,
      })
    : t("conversation.content.translate", {
        language: targetLanguage,
        type: label,
      });
}

function translationPhaseLabel(phase: AiExecutionPhase, t: Translator) {
  return t(`conversation.content.translationPhase.${phase}` as TranslationKey);
}

function removeRecordKey<T>(record: Record<string, T>, key: string) {
  if (!(key in record)) return record;
  const next = { ...record };
  delete next[key];
  return next;
}

function normalizeResultPreviewText(value: string) {
  return value.replace(/\r\n?/g, "\n").trimEnd();
}

function buildConversationCardPreview(
  value: string,
  lineLimit: number,
  expanded: boolean,
) {
  const safeLineLimit = Number.isFinite(lineLimit)
    ? Math.max(1, Math.round(lineLimit))
    : DEFAULT_RESULT_PREVIEW_LINE_LIMIT;
  const formattedValue = normalizeResultPreviewText(value);
  const lines = formattedValue.split("\n");
  const hasOverflow = lines.length > safeLineLimit;

  return {
    hasOverflow,
    lines,
    visibleLineCount: hasOverflow && !expanded ? safeLineLimit : lines.length,
    visibleValue:
      hasOverflow && !expanded
        ? lines.slice(0, safeLineLimit).join("\n")
        : formattedValue,
  };
}

function clearCopiedResetTimer(timerRef: { current: number | null }) {
  if (timerRef.current === null) return;
  window.clearTimeout(timerRef.current);
  timerRef.current = null;
}

async function writeClipboardText(value: string) {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
    throw new Error("Clipboard API is unavailable");
  }
  await navigator.clipboard.writeText(value);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function withAlpha(hexColor: string, alpha: string) {
  return `${hexColor}${alpha}`;
}

function BlockMetadata({
  block,
  t,
}: {
  block: ConversationContentBlock;
  t: Translator;
}) {
  const details = [
    block.language,
    block.cwd ? abbreviateHomePath(block.cwd) : null,
    block.type === "command" ? null : block.status,
    block.type === "command" || block.exitCode == null
      ? null
      : t("conversation.content.exitCode", { code: block.exitCode }),
  ].filter(Boolean);

  if (details.length === 0) return null;

  return (
    <div className="conversation-meta-row mt-3 flex flex-wrap gap-2 pt-3">
      {details.map((detail) => (
        <span
          className="rounded-full border border-inherit bg-theme-card/45 px-2 py-1 font-mono text-code-sm text-on-surface-variant"
          key={String(detail)}
        >
          {detail}
        </span>
      ))}
    </div>
  );
}

function isSuccessfulCommand(block: ConversationContentBlock) {
  if (block.type !== "command") return false;
  if (block.exitCode === 0) return true;
  return [
    "success",
    "succeeded",
    "completed",
    "complete",
    "done",
    "ok",
  ].includes(block.status?.toLowerCase() ?? "");
}

function isSuccessfulResult(block: ConversationContentBlock) {
  if (block.type !== "result") return false;
  if (block.exitCode === 0) return true;
  return [
    "success",
    "succeeded",
    "completed",
    "complete",
    "done",
    "ok",
  ].includes(block.status?.toLowerCase() ?? "");
}

function isFailedResult(block: ConversationContentBlock) {
  if (block.type !== "result") return false;
  if (block.exitCode != null && block.exitCode !== 0) return true;
  return [
    "error",
    "failed",
    "failure",
    "cancelled",
    "canceled",
    "interrupted",
    "timeout",
    "timed_out",
  ].includes(block.status?.toLowerCase() ?? "");
}

function shouldDisplayContentBlock(block: ConversationContentBlock) {
  if (block.type !== "result") return true;
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  const text = block.text.trim();
  if (renderer === "diff") return text.length > 0;
  if (isSuccessfulResult(block)) return false;
  if (/^(?:\{\}|\[\]|null|undefined)$/i.test(text)) return false;
  return text.length > 0 || isFailedResult(block);
}

function createBlock(
  part: ConversationPart,
  type: ConversationContentType,
  value?: string | null,
  suffix: string = type,
  metadataMode: "all" | "command" | "result" = "all",
  overrides: Partial<ConversationContentBlock> = {},
): ConversationContentBlock[] {
  const text = visibleCardText(value) ?? "";
  const hasOverride = (key: keyof ConversationContentBlock) =>
    Object.prototype.hasOwnProperty.call(overrides, key) &&
    overrides[key] !== undefined;
  const status = hasOverride("status") ? overrides.status : part.status;
  const exitCode = hasOverride("exitCode")
    ? overrides.exitCode
    : part.exit_code;
  const renderer = overrides.renderer ?? legacyRenderer(type, overrides.format);
  const statusOnlyResult =
    type === "result" &&
    (status != null || exitCode != null || renderer === "diff");
  if (!text && !statusOnlyResult) return [];

  let signal = overrides.signal;
  let filePath = overrides.filePath;
  let toolName = overrides.toolName;
  if (part.metadata_json) {
    try {
      const meta = JSON.parse(part.metadata_json);
      if (typeof meta === "object" && meta !== null) {
        if (!signal && typeof meta.signal === "string") signal = meta.signal;
        if (!filePath && typeof meta.file_path === "string") filePath = meta.file_path;
        if (!toolName && typeof meta.tool_name === "string") toolName = meta.tool_name;
      }
    } catch {}
  }

  return [
    {
      id: `${part.id}-${suffix}`,
      partId: part.id,
      type,
      renderer,
      role: part.role,
      text,
      commandLabel: part.command_label,
      translatedText: part.translated_text,
      format: overrides.format,
      signal,
      filePath,
      toolName,
      language: hasOverride("language")
        ? overrides.language
        : metadataMode === "result"
          ? null
          : part.language,
      cwd: hasOverride("cwd")
        ? overrides.cwd
        : metadataMode === "result"
          ? null
          : part.cwd,
      status: hasOverride("status")
        ? overrides.status
        : metadataMode === "command"
          ? null
          : part.status,
      exitCode: hasOverride("exitCode")
        ? overrides.exitCode
        : metadataMode === "command"
          ? null
          : part.exit_code,
    },
  ];
}

function createDeclaredContentBlock(
  part: ConversationPart,
): ConversationContentBlock[] {
  if (part.content_card) {
    const renderer = part.content_card.renderer ?? "plain";
    const declaredType = contentTypeValue(part.content_card.kind);
    if (!declaredType) return [];
    const type = conversationCardPresentationKind(declaredType);
    return createBlock(
      part,
      type,
      defaultContentCardText(part, type),
      type,
      "all",
      {
        renderer,
        format: renderer === "markdown" ? "markdown" : "plain",
      },
    );
  }
  const card = contentCardMetadata(part.metadata_json);
  if (!card) return [];

  const declaredType = contentTypeValue(card.type);
  if (!declaredType) return [];
  const type = conversationCardPresentationKind(declaredType);

  const format = contentFormatValue(card.format);
  const renderer =
    rendererValue(card.renderer) ??
    rendererValue(
      isRecord(card.presentation) ? card.presentation.renderer : undefined,
    ) ??
    legacyRenderer(type, format);
  const text = stringValue(card.text) ?? defaultContentCardText(part, type);
  const suffix = stringValue(card.suffix) ?? type;

  return createBlock(part, type, text, suffix, "all", {
    format,
    renderer,
    language: stringValue(card.language),
    cwd: stringValue(card.cwd),
    status: stringValue(card.status),
    exitCode: numberValue(card.exit_code) ?? numberValue(card.exitCode),
  });
}

function contentCardMetadata(value?: string | null) {
  const metadata = parseMetadataRecord(value);
  const card = metadata?.content_card ?? metadata?.contentCard;
  return isRecord(card) ? card : null;
}

function parseMetadataRecord(value?: string | null) {
  if (!value?.trim()) return null;
  try {
    const parsed = JSON.parse(value) as unknown;
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function contentTypeValue(value: unknown): ConversationContentType | null {
  return typeof value === "string" && /^[a-z0-9][a-z0-9._-]{0,127}$/.test(value)
    ? value
    : null;
}

function rendererValue(value: unknown): ConversationCardRenderer | undefined {
  return value === "markdown" ||
    value === "plain" ||
    value === "path" ||
    value === "json" ||
    value === "code" ||
    value === "command" ||
    value === "terminal_output" ||
    value === "diff" ||
    value === "compact_action" ||
    value === "subagent_tree" ||
    value === "accordion"
    ? value
    : undefined;
}

function legacyRenderer(
  type: ConversationContentType,
  format?: ConversationContentFormat,
): ConversationCardRenderer {
  if (type === "command") return "command";
  if (type === "code") return "code";
  if (type === "result") return "terminal_output";
  if (format === "plain") return "plain";
  return "markdown";
}

function contentFormatValue(
  value: unknown,
): ConversationContentFormat | undefined {
  return value === "markdown" || value === "plain" ? value : undefined;
}

function defaultContentCardText(
  part: ConversationPart,
  type: ConversationContentType,
) {
  if (type === "command") {
    return part.command?.trim() || part.text;
  }
  return part.text ?? part.command;
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function visibleCardText(value?: string | null) {
  const text = value?.replace(adapterBrowseMarkerPattern, "").trim();
  return text || undefined;
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
