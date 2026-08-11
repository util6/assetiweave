import { Check, CheckCircle2, Copy, GitCompareArrows, Languages, XCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  checkConversationTranslationAvailability,
  translateConversationCardContent,
  updateConversationPartTranslation,
  type ConversationCardTranslationRequest,
  type OpencodeTranslationAvailability,
  type OpencodeTranslationResult,
  type ConversationPartTranslationUpdateRequest,
} from "../../services/cardTranslation";
import { revealPath } from "../../services/catalog";
import type {
  ConversationCard,
  ConversationCardRenderer,
  ConversationContentNode,
  ConversationPart,
  ConversationPartRole,
  ConversationRecordKind,
} from "../../types";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  normalizeConversationTranslationTargetLanguage,
  type ConversationContentCardColorSettings,
  type ResolvedConversationTranslationSettings,
  type ConversationTranslationTargetLanguage,
} from "../../store/settings/AppSettingsProvider";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import { MarkdownContent } from "./ConversationMarkdown";
import {
  ConversationCardKindIcon,
  conversationCardPresentationKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";
import { ConversationDiff, summarizeConversationDiff } from "./ConversationDiff";

export type ConversationContentType = string;

export type ConversationContentVisibility = Record<ConversationContentType, boolean>;
export type ConversationContentFormat = "plain" | "markdown";
type TranslationAvailabilityStatus = "idle" | "checking" | "available" | "unavailable";

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
}

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
      command?: ConversationContentBlock;
      results: ConversationContentBlock[];
    };

export const DEFAULT_CONVERSATION_CONTENT_VISIBILITY: ConversationContentVisibility = {
  answer: true,
  tool: true,
  command: true,
  code: true,
  result: true,
};

export function conversationCardDomId(blockId: string) {
  return `conversation-card-${blockId}`;
}

const adapterBrowseMarkerPattern =
  /\s*\[AssetIWeave adapter (?:truncated \d+ characters for browsing|compacted low-signal tool output for browsing; original \d+ characters)\.\]/g;

export function buildConversationContentBlocks(
  parts: ConversationPart[],
  cards?: ConversationCard[],
): ConversationContentBlock[] {
  if (cards?.length) {
    return cards.map(conversationCardToBlock);
  }
  return parts.flatMap(createDeclaredContentBlock);
}

export function buildConversationDisplayNodes(
  cards: ConversationCard[],
  nodes: ConversationContentNode[],
): ConversationDisplayNode[] {
  return nodes.flatMap((node): ConversationDisplayNode[] => {
    if (node.type === "card") {
      const card = cards[node.card_index];
      return card
        ? [{ type: "card", turnId: node.turn_id, block: conversationCardToBlock(card) }]
        : [];
    }

    const commandCard = node.command_card_index == null
      ? undefined
      : cards[node.command_card_index];
    const results = node.result_card_indices.flatMap((cardIndex) => {
      const card = cards[cardIndex];
      return card ? [conversationCardToBlock(card)] : [];
    });
    if (!commandCard && results.length === 0) return [];
    if (commandCard && results.length === 0) {
      return [{
        type: "card",
        turnId: node.turn_id,
        block: conversationCardToBlock(commandCard),
      }];
    }

    return [{
      type: "execution",
      turnId: node.turn_id,
      sourceExecutionId: node.source_execution_id,
      command: commandCard ? conversationCardToBlock(commandCard) : undefined,
      results,
    }];
  });
}

function conversationCardToBlock(card: ConversationCard): ConversationContentBlock {
  return {
    id: card.card_id,
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
  };
}

export function ConversationContentCards({
  activeBlockId,
  blocks,
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  onCopyError,
  onTranslationError,
  nodes,
  recordKind = "session",
  resultPreviewLineLimit = DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  t,
  translationAvailabilityChecker,
  translationSaver = updateConversationPartTranslation,
  translationSettings = DEFAULT_TRANSLATION_SETTINGS,
  translator = translateConversationCardContent,
  visibility,
}: {
  activeBlockId?: string | null;
  blocks: ConversationContentBlock[];
  colors?: ConversationContentCardColorSettings;
  onCopyError?: (message: string) => void;
  onTranslationError?: (message: string) => void;
  nodes?: ConversationDisplayNode[];
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  t: Translator;
  translationAvailabilityChecker?: () => Promise<OpencodeTranslationAvailability>;
  translationSaver?: (request: ConversationPartTranslationUpdateRequest) => Promise<void>;
  translationSettings?: ResolvedConversationTranslationSettings;
  translator?: (request: ConversationCardTranslationRequest) => Promise<OpencodeTranslationResult>;
  visibility: ConversationContentVisibility;
}) {
  const displayNodes: ConversationDisplayNode[] = nodes
    ?? blocks.map((block) => ({ type: "card", turnId: "", block }));
  const visibleNodes = displayNodes.flatMap((node): ConversationDisplayNode[] => {
    if (node.type === "card") {
      return visibility[node.block.type] ?? true ? [node] : [];
    }
    const command = node.command && (visibility[node.command.type] ?? true)
      ? node.command
      : undefined;
    const results = node.results.filter((block) => (
      (visibility[block.type] ?? true) && shouldDisplayContentBlock(block)
    ));
    if (command && results.length === 0) {
      return [{ type: "card", turnId: node.turnId, block: command }];
    }
    return command || results.length > 0 ? [{ ...node, command, results }] : [];
  });
  const visibleBlocks = visibleNodes.flatMap((node) => (
    node.type === "card"
      ? [node.block]
      : [...(node.command ? [node.command] : []), ...node.results]
  ));
  const [copiedBlockId, setCopiedBlockId] = useState<string | null>(null);
  const [translatedBlocks, setTranslatedBlocks] = useState<Record<string, string>>({});
  const [translationErrors, setTranslationErrors] = useState<Record<string, string>>({});
  const [translatingBlockIds, setTranslatingBlockIds] = useState<Set<string>>(new Set());
  const [translationAvailability, setTranslationAvailability] =
    useState<TranslationAvailabilityStatus>("idle");
  const copiedResetTimerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      clearCopiedResetTimer(copiedResetTimerRef);
    },
    [],
  );

  useEffect(() => {
    if (visibleBlocks.length === 0) return;

    let cancelled = false;
    setTranslationAvailability("checking");
    const checkAvailability = translationAvailabilityChecker ?? (() =>
      checkConversationTranslationAvailability({
        cli: translationSettings.cli,
        model: translationSettings.model,
        provider: translationSettings.provider,
      }));
    checkAvailability()
      .then((availability) => {
        if (cancelled) return;
        setTranslationAvailability(availability.available ? "available" : "unavailable");
      })
      .catch(() => {
        if (cancelled) return;
        setTranslationAvailability("unavailable");
      });

    return () => {
      cancelled = true;
    };
  }, [
    translationAvailabilityChecker,
    translationSettings.cli,
    translationSettings.model,
    translationSettings.provider,
    visibleBlocks.length,
  ]);

  async function handleCopyBlock(block: ConversationContentBlock) {
    try {
      await writeClipboardText(block.text);
      clearCopiedResetTimer(copiedResetTimerRef);
      setCopiedBlockId(block.id);
      copiedResetTimerRef.current = window.setTimeout(() => {
        setCopiedBlockId((current) => (current === block.id ? null : current));
        copiedResetTimerRef.current = null;
      }, 1400);
    } catch (error) {
      onCopyError?.(
        t("conversation.content.copyFailed", { message: errorMessage(error) }),
      );
    }
  }

  async function handleTranslateBlock(block: ConversationContentBlock) {
    if (translationAvailability !== "available") return;

    setTranslatingBlockIds((current) => new Set(current).add(block.id));
    setTranslationErrors((current) => {
      const next = { ...current };
      delete next[block.id];
      return next;
    });

    try {
      const result = await translator({
        cli: translationSettings.cli,
        model: translationSettings.model,
        promptTemplate: translationSettings.promptTemplate,
        provider: translationSettings.provider,
        targetLanguage: translationSettings.targetLanguage,
        text: block.text,
      });
      if (block.partId) {
        await translationSaver({
          partId: block.partId,
          recordKind,
          translatedText: result.translated_text,
        });
      }
      setTranslatedBlocks((current) => ({
        ...current,
        [block.id]: result.translated_text,
      }));
    } catch (error) {
      const message = errorMessage(error);
      setTranslationErrors((current) => ({
        ...current,
        [block.id]: message,
      }));
      onTranslationError?.(
        t("conversation.content.translationFailed", { message }),
      );
    } finally {
      setTranslatingBlockIds((current) => {
        const next = new Set(current);
        next.delete(block.id);
        return next;
      });
    }
  }

  if (visibleBlocks.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">
        {t("conversation.content.hidden")}
      </div>
    );
  }

  return (
    <div className="grid gap-3">
      {visibleNodes.map((node) => {
        if (node.type === "card") {
          return renderContentCard(node.block);
        }
        const executionKey = `${node.turnId}:${node.sourceExecutionId}`;
        return (
          <section
            className="overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/35"
            data-conversation-execution-id={node.sourceExecutionId}
            key={executionKey}
          >
            <header className="flex flex-wrap items-center gap-2 border-b border-theme-card-border bg-theme-card/55 px-4 py-2.5">
              <span className="text-label-caps text-on-surface-variant">
                {t("conversation.content.execution")}
              </span>
            </header>
            <div className="grid gap-3 p-3">
              {node.command ? renderContentCard(node.command) : null}
              {node.results.map(renderContentCard)}
            </div>
          </section>
        );
      })}
    </div>
  );

  function renderContentCard(block: ConversationContentBlock) {
    return (
      <ConversationContentCard
        block={block}
        colors={colors}
        copied={copiedBlockId === block.id}
        highlighted={activeBlockId === block.id || block.legacyAnchorIds?.includes(activeBlockId ?? "") === true}
        key={block.id}
        onCopy={() => void handleCopyBlock(block)}
        onTranslate={() => void handleTranslateBlock(block)}
        resultPreviewLineLimit={resultPreviewLineLimit}
        t={t}
        translatedText={translatedBlocks[block.id] ?? block.translatedText ?? undefined}
        translating={translatingBlockIds.has(block.id)}
        translationAvailability={translationAvailability}
        translationError={translationErrors[block.id]}
        translationTargetLanguage={translationSettings.targetLanguage}
      />
    );
  }
}

const DEFAULT_TRANSLATION_SETTINGS: ResolvedConversationTranslationSettings = {
  cli: "opencode",
  model: "",
  promptTemplate: DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  provider: "cli",
  targetLanguage: DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
};

function ConversationContentCard({
  block,
  colors,
  copied,
  highlighted,
  onCopy,
  onTranslate,
  resultPreviewLineLimit,
  t,
  translatedText,
  translating,
  translationAvailability,
  translationError,
  translationTargetLanguage,
}: {
  block: ConversationContentBlock;
  colors: ConversationContentCardColorSettings;
  copied: boolean;
  highlighted: boolean;
  onCopy: () => void;
  onTranslate: () => void;
  resultPreviewLineLimit: number;
  t: Translator;
  translatedText?: string;
  translating: boolean;
  translationAvailability: TranslationAvailabilityStatus;
  translationError?: string;
  translationTargetLanguage: ConversationTranslationTargetLanguage;
}) {
  const [expanded, setExpanded] = useState(false);
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  const { definitions } = useConversationCardKindRegistry();
  const definition = block.kind && block.kind === block.type ? definitions.get(block.kind) : undefined;
  const label = definition?.label ?? conversationCardLabel(block.type, t);
  const role = t(`conversation.part.role.${block.role}` as TranslationKey);
  const accentColor = conversationCardColor(block.type, colors);
  const copyLabel = copied
    ? t("conversation.content.copied")
    : t("conversation.content.copy", { type: label });
  const translationTargetLabel = normalizeConversationTranslationTargetLanguage(translationTargetLanguage);
  const translateDisabled = renderer === "path" || translationAvailability !== "available" || translating;
  const translateLabel = translationButtonLabel({
    hasTranslation: Boolean(translatedText),
    label,
    status: translationAvailability,
    t,
    targetLanguage: translationTargetLabel,
    translating,
  });
  const resultPresentation = block.type === "result"
    ? describeResultPresentation(block)
    : undefined;
  const diffSummary = renderer === "diff"
    ? summarizeConversationDiff(block.text)
    : undefined;
  const preview = resultPresentation?.type === "file-change"
    ? {
        hasOverflow: false,
        lines: [],
        visibleLineCount: 0,
        visibleValue: "",
      }
    : buildConversationCardPreview(block.text, resultPreviewLineLimit, expanded);
  const canExpandDiff = resultPresentation?.type === "file-change" && block.text.trim().length > 0;
  const canExpandResult = canExpandDiff || preview.hasOverflow;

  return (
    <section
      className={`scroll-mt-32 overflow-hidden rounded-xl border transition-shadow ${
        highlighted ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]" : ""
      }`}
      data-content-type={block.type}
      data-conversation-card-id={block.id}
      id={conversationCardDomId(block.id)}
      style={{
        backgroundColor: withAlpha(accentColor, "12"),
        borderColor: withAlpha(accentColor, "66"),
      }}
    >
      <header className="flex flex-wrap items-center justify-between gap-2 border-b border-inherit px-4 py-2.5">
        <div className="flex min-w-0 flex-wrap items-center gap-2 text-label-caps" style={{ color: accentColor }}>
          {isSuccessfulCommand(block) ? (
            <CheckCircle2 aria-hidden="true" size={15} />
          ) : (
            <ConversationCardKindIcon iconHint={definition?.icon_hint} kind={block.type} renderer={renderer} />
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
            title={block.id}
          >
            {conversationIdFragment(block.id)}
          </span>
          <span className="text-label-caps text-on-surface-muted">{role}</span>
          <button
            aria-label={copyLabel}
            className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-on-surface-muted transition-colors hover:text-on-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
            onClick={onCopy}
            title={copyLabel}
            type="button"
          >
            {copied ? <Check className="size-[1em]" /> : <Copy className="size-[1em]" />}
          </button>
          <button
            aria-label={translateLabel}
            className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-on-surface-muted transition-colors enabled:hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
            disabled={translateDisabled}
            onClick={onTranslate}
            title={translateLabel}
            type="button"
          >
            <Languages className={translating ? "size-[1em] animate-pulse" : "size-[1em]"} />
          </button>
        </div>
      </header>
      <div className="px-4 py-3">
        <ConversationCardBody
          block={block}
          label={label}
          text={resultPresentation?.type === "file-change" ? block.text : preview.visibleValue}
          expanded={expanded}
          diffSummary={diffSummary}
          resultPresentation={resultPresentation}
          t={t}
        />
        {canExpandResult ? (
          <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-inherit bg-theme-card/35 px-3 py-2">
            {canExpandDiff ? (
              <span className="text-code-sm text-on-surface-muted">
                {t("conversation.content.diffSummaryFiles", { count: resultPresentation.summary.files.length })}
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
              className="rounded-lg border border-theme-control-border bg-theme-control/80 px-2.5 py-1 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface"
              onClick={() => setExpanded((current) => !current)}
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
          <div className="mt-3 rounded-lg border border-inherit bg-theme-card/45 px-3 py-3">
            <div className="mb-2 text-label-caps text-on-surface-muted">
              {t("conversation.content.translation", { language: translationTargetLabel })}
            </div>
            <MarkdownContent value={translatedText} />
          </div>
        ) : null}
        {translationError ? (
          <div className="mt-3 rounded-lg border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove" role="alert">
            {t("conversation.content.translationFailed", { message: translationError })}
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
            className="rounded-lg border border-inherit bg-theme-card/45 px-3 py-3"
            data-result-format="markdown"
          >
            <MarkdownContent value={text} />
          </div>
        );
      }
      return (
        <pre className="max-h-[38rem] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-inherit bg-theme-card/45 p-3 text-code-sm leading-6 text-on-surface">
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

function describeResultPresentation(block: ConversationContentBlock): ConversationResultPresentation | undefined {
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  if (renderer === "diff") {
    return { type: "file-change", summary: summarizeConversationDiff(block.text) };
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
      <ConversationDiff summary={diffSummary ?? presentation.summary} value={block.text} />
    ) : (
      <FileChangeResultSummary summary={presentation.summary} t={t} />
    );
  }

  if (presentation.type === "success") {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-status-create/30 bg-status-create/10 px-3 py-2.5 text-body-sm text-status-create" data-result-summary="success">
        <CheckCircle2 aria-hidden="true" size={16} />
        <span>{t("conversation.content.resultSuccess")}</span>
        {block.exitCode != null ? <span>· {t("conversation.content.exitCode", { code: block.exitCode })}</span> : null}
      </div>
    );
  }

  return (
    <div className="grid gap-2" data-result-summary="failure">
      <div className="flex items-center gap-2 rounded-lg border border-status-remove/30 bg-status-remove/10 px-3 py-2.5 text-body-sm text-status-remove">
        <XCircle aria-hidden="true" size={16} />
        <span>{t("conversation.content.resultFailed")}</span>
        {block.exitCode != null ? <span>· {t("conversation.content.exitCode", { code: block.exitCode })}</span> : null}
      </div>
      {text ? (
        <pre className="max-h-[24rem] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-status-remove/20 bg-status-remove/[0.06] p-3 text-code-sm leading-6 text-on-surface">
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
        <GitCompareArrows aria-hidden="true" size={16} className="text-status-update" />
        <span>
          {t("conversation.content.changedFiles", { count: summary.files.length })}
          <span className="ml-2 font-mono text-code-sm font-normal text-status-create">+{summary.additions}</span>
          <span className="ml-1 font-mono text-code-sm font-normal text-status-remove">-{summary.deletions}</span>
        </span>
      </div>
      {summary.files.length > 0 ? (
        <div className="divide-y divide-theme-card-border overflow-hidden rounded-lg border border-theme-card-border bg-theme-card/45">
          {summary.files.map((file) => (
            <div className="flex items-center gap-3 px-3 py-2 font-mono text-code-sm" data-diff-summary-file={file.path} key={file.path}>
              <span className="w-4 shrink-0 text-center text-on-surface-muted">{fileStatusMark(file.status, file.binary)}</span>
              <span className="min-w-0 flex-1 truncate text-on-surface" title={file.path}>{file.path}</span>
              <span className="shrink-0 text-status-create">+{file.additions}</span>
              <span className="shrink-0 text-status-remove">-{file.deletions}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="rounded-lg border border-theme-card-border bg-theme-card/45 px-3 py-2 text-code-sm text-on-surface-muted">
          {t("conversation.content.diffSummaryUnavailable")}
        </div>
      )}
    </div>
  );
}

function fileStatusMark(status: "added" | "deleted" | "modified" | "renamed", binary: boolean) {
  if (binary) return "B";
  if (status === "added") return "A";
  if (status === "deleted") return "D";
  if (status === "renamed") return "R";
  return "M";
}

function LocalPathCardBody({ label, path, t }: { label: string; path: string; t: Translator }) {
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
        className="flex min-w-0 items-center rounded-lg border border-inherit bg-theme-card/45 px-3 py-2.5 text-left font-mono text-code-sm text-primary transition-colors hover:bg-theme-control hover:text-primary-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
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

const builtInCardKinds = new Set(["answer", "tool", "command", "code", "result"]);

export function conversationCardLabel(kind: string, t: Translator) {
  if (builtInCardKinds.has(kind)) {
    return t(`conversation.content.${kind}` as TranslationKey);
  }
  const segments = kind.split(".");
  const leaf = segments[segments.length - 1] ?? kind;
  return leaf
    .split(/[-_]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ") || kind;
}

export function conversationCardColor(
  kind: string,
  colors: ConversationContentCardColorSettings,
) {
  const configured = colors[kind];
  if (configured) return configured;
  let hash = 0;
  for (const character of kind) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }
  const hue = hash % 360;
  return hslToHex(hue, 48, 52);
}

function hslToHex(hue: number, saturation: number, lightness: number) {
  const s = saturation / 100;
  const l = lightness / 100;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const segment = hue / 60;
  const x = chroma * (1 - Math.abs((segment % 2) - 1));
  const [red, green, blue] = segment < 1
    ? [chroma, x, 0]
    : segment < 2
      ? [x, chroma, 0]
      : segment < 3
        ? [0, chroma, x]
        : segment < 4
          ? [0, x, chroma]
          : segment < 5
            ? [x, 0, chroma]
            : [chroma, 0, x];
  const match = l - chroma / 2;
  return `#${[red, green, blue]
    .map((channel) => Math.round((channel + match) * 255).toString(16).padStart(2, "0"))
    .join("")}`;
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
    ? t("conversation.content.retranslate", { language: targetLanguage, type: label })
    : t("conversation.content.translate", { language: targetLanguage, type: label });
}

function normalizeResultPreviewText(value: string) {
  return value.replace(/\r\n?/g, "\n").trimEnd();
}

function buildConversationCardPreview(value: string, lineLimit: number, expanded: boolean) {
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
    visibleValue: hasOverflow && !expanded
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
    <div className="mt-3 flex flex-wrap gap-2 border-t border-inherit pt-3">
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
  return ["success", "succeeded", "completed", "complete", "done", "ok"].includes(
    block.status?.toLowerCase() ?? "",
  );
}

function isSuccessfulResult(block: ConversationContentBlock) {
  if (block.type !== "result") return false;
  if (block.exitCode === 0) return true;
  return ["success", "succeeded", "completed", "complete", "done", "ok"].includes(
    block.status?.toLowerCase() ?? "",
  );
}

function isFailedResult(block: ConversationContentBlock) {
  if (block.type !== "result") return false;
  if (block.exitCode != null && block.exitCode !== 0) return true;
  return ["error", "failed", "failure", "cancelled", "canceled", "interrupted", "timeout", "timed_out"].includes(
    block.status?.toLowerCase() ?? "",
  );
}

function shouldDisplayContentBlock(block: ConversationContentBlock) {
  if (block.type !== "result") return true;
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  if (renderer === "diff") return block.text.trim().length > 0;
  return block.text.trim().length > 0 || !isSuccessfulResult(block);
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
    Object.prototype.hasOwnProperty.call(overrides, key) && overrides[key] !== undefined;
  const status = hasOverride("status") ? overrides.status : part.status;
  const exitCode = hasOverride("exitCode") ? overrides.exitCode : part.exit_code;
  const renderer = overrides.renderer ?? legacyRenderer(type, overrides.format);
  const statusOnlyResult = type === "result" && (
    status != null || exitCode != null || renderer === "diff"
  );
  if (!text && !statusOnlyResult) return [];

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

function createDeclaredContentBlock(part: ConversationPart): ConversationContentBlock[] {
  if (part.content_card) {
    const renderer = part.content_card.renderer ?? "plain";
    const declaredType = contentTypeValue(part.content_card.kind);
    if (!declaredType) return [];
    const type = conversationCardPresentationKind(declaredType);
    return createBlock(part, type, defaultContentCardText(part, type), type, "all", {
      renderer,
      format: renderer === "markdown" ? "markdown" : "plain",
    });
  }
  const card = contentCardMetadata(part.metadata_json);
  if (!card) return [];

  const declaredType = contentTypeValue(card.type);
  if (!declaredType) return [];
  const type = conversationCardPresentationKind(declaredType);

  const format = contentFormatValue(card.format);
  const renderer = rendererValue(card.renderer)
    ?? rendererValue(isRecord(card.presentation) ? card.presentation.renderer : undefined)
    ?? legacyRenderer(type, format);
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
    value === "diff"
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

function contentFormatValue(value: unknown): ConversationContentFormat | undefined {
  return value === "markdown" || value === "plain" ? value : undefined;
}

function defaultContentCardText(part: ConversationPart, type: ConversationContentType) {
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
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
