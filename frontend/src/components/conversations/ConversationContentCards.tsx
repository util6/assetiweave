import { Check, Copy, Languages } from "lucide-react";
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
import { ConversationDiff } from "./ConversationDiff";
import { isDiffLanguage, isUnifiedDiffText } from "./conversationDiffLanguage";

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
    const results = node.results.filter((block) => visibility[block.type] ?? true);
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
            <header className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-card-border bg-theme-card/55 px-4 py-2.5">
              <span className="text-label-caps text-on-surface-variant">
                {t("conversation.content.execution")}
              </span>
              <span
                className="select-text rounded-md border border-theme-card-border bg-theme-card/65 px-1.5 py-0.5 font-mono text-code-sm normal-case text-on-surface-muted"
                title={node.sourceExecutionId}
              >
                {conversationIdFragment(node.sourceExecutionId)}
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
        <div className="flex items-center gap-2 text-label-caps" style={{ color: accentColor }}>
          <ConversationCardKindIcon iconHint={definition?.icon_hint} kind={block.type} renderer={renderer} />
          <span>{label}</span>
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
          resultPreviewLineLimit={resultPreviewLineLimit}
          t={t}
        />
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
  label,
  resultPreviewLineLimit,
  t,
}: {
  block: ConversationContentBlock;
  label: string;
  resultPreviewLineLimit: number;
  t: Translator;
}) {
  const renderer = block.renderer ?? legacyRenderer(block.type, block.format);
  switch (renderer) {
    case "markdown":
      return <MarkdownContent value={block.text} />;
    case "terminal_output":
      if (isUnifiedDiffText(block.text)) {
        return <ConversationDiff value={block.text} />;
      }
      return (
        <CommandResultPreview
          format={block.format ?? "plain"}
          lineLimit={resultPreviewLineLimit}
          t={t}
          value={block.text}
        />
      );
    case "path":
      return <LocalPathCardBody label={label} path={block.text} t={t} />;
    case "json":
    case "code":
      if (isDiffLanguage(block.language)) {
        return <ConversationDiff value={block.text} />;
      }
      return (
        <pre className="overflow-auto whitespace-pre-wrap break-words text-code-sm leading-6 text-on-surface">
          <code>{block.text}</code>
        </pre>
      );
    case "command":
    case "plain":
      if (isUnifiedDiffText(block.text)) {
        return <ConversationDiff value={block.text} />;
      }
      return (
        <pre className="overflow-auto whitespace-pre-wrap break-words text-code-sm leading-6 text-on-surface">
          <code>{block.text}</code>
        </pre>
      );
  }
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

function CommandResultPreview({
  format,
  lineLimit,
  t,
  value,
}: {
  format: ConversationContentFormat;
  lineLimit: number;
  t: Translator;
  value: string;
}) {
  const [expanded, setExpanded] = useState(false);
  if (format === "markdown") {
    return (
      <div
        className="rounded-lg border border-inherit bg-theme-card/45 px-3 py-3"
        data-result-format="markdown"
      >
        <MarkdownContent value={value} />
      </div>
    );
  }

  const safeLineLimit = Number.isFinite(lineLimit)
    ? Math.max(1, Math.round(lineLimit))
    : DEFAULT_RESULT_PREVIEW_LINE_LIMIT;
  const formattedValue = normalizeResultPreviewText(value);
  const lines = formattedValue.split("\n");
  const hasOverflow = lines.length > safeLineLimit;
  const visibleLineCount = hasOverflow && !expanded ? safeLineLimit : lines.length;
  const visibleValue = hasOverflow && !expanded
    ? lines.slice(0, safeLineLimit).join("\n")
    : formattedValue;

  return (
    <div className="grid gap-2">
      <pre className="max-h-[38rem] overflow-auto whitespace-pre-wrap break-words rounded-lg border border-inherit bg-theme-card/45 p-3 text-code-sm leading-6 text-on-surface">
        <code>{visibleValue}</code>
      </pre>
      {hasOverflow ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-inherit bg-theme-card/35 px-3 py-2">
          <span className="text-code-sm text-on-surface-muted">
            {t("conversation.content.resultPreviewLines", {
              shown: visibleLineCount,
              total: lines.length,
            })}
          </span>
          <button
            className="rounded-lg border border-theme-control-border bg-theme-control/80 px-2.5 py-1 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            onClick={() => setExpanded((current) => !current)}
            type="button"
          >
            {expanded
              ? t("conversation.content.collapseResult")
              : t("conversation.content.expandResult")}
          </button>
        </div>
      ) : null}
    </div>
  );
}

function normalizeResultPreviewText(value: string) {
  return value.replace(/\r\n?/g, "\n").trimEnd();
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
    block.status,
    block.exitCode == null
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

function createBlock(
  part: ConversationPart,
  type: ConversationContentType,
  value?: string | null,
  suffix: string = type,
  metadataMode: "all" | "command" | "result" = "all",
  overrides: Partial<ConversationContentBlock> = {},
): ConversationContentBlock[] {
  const text = visibleCardText(value);
  if (!text) return [];
  const hasOverride = (key: keyof ConversationContentBlock) =>
    Object.prototype.hasOwnProperty.call(overrides, key);

  return [
    {
      id: `${part.id}-${suffix}`,
      partId: part.id,
      type,
      renderer: overrides.renderer ?? legacyRenderer(type, overrides.format),
      role: part.role,
      text,
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
    const type = contentTypeValue(part.content_card.kind);
    if (!type) return [];
    return createBlock(part, type, defaultContentCardText(part, type), type, "all", {
      renderer,
      format: renderer === "markdown" ? "markdown" : "plain",
    });
  }
  const card = contentCardMetadata(part.metadata_json);
  if (!card) return [];

  const type = contentTypeValue(card.type);
  if (!type) return [];

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
    value === "terminal_output"
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
