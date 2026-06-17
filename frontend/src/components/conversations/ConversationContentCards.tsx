import { Braces, Check, CheckCircle2, Copy, FileText, Terminal, Wrench } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type {
  ConversationPart,
  ConversationPartRole,
} from "../../types";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  type ConversationContentCardColorSettings,
} from "../../store/settings/AppSettingsProvider";
import { abbreviateHomePath } from "../../utils/path";
import { MarkdownContent } from "./ConversationMarkdown";

export type ConversationContentType = "answer" | "tool" | "command" | "code" | "result";

export type ConversationContentVisibility = Record<ConversationContentType, boolean>;

export interface ConversationContentBlock {
  id: string;
  type: ConversationContentType;
  role: ConversationPartRole;
  text: string;
  language?: string | null;
  cwd?: string | null;
  status?: string | null;
  exitCode?: number | null;
}

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

const icons: Record<ConversationContentType, ReactNode> = {
  answer: <FileText size={15} />,
  tool: <Wrench size={15} />,
  command: <Terminal size={15} />,
  code: <Braces size={15} />,
  result: <CheckCircle2 size={15} />,
};

export function buildConversationContentBlocks(parts: ConversationPart[]): ConversationContentBlock[] {
  return parts.flatMap((part) => {
    if (part.kind === "code_block") {
      return createBlock(part, "code", part.text);
    }

    if (part.kind === "command") {
      const command = part.command?.trim() || part.text?.trim();
      const output = commandOutput(part);
      return [
        ...createBlock(part, "command", command, "command", "command"),
        ...createBlock(part, "result", output, "result", "result"),
      ];
    }

    if (part.kind === "tool") {
      return createBlock(part, isToolResult(part) ? "result" : "tool", part.text);
    }

    if (part.kind === "text") {
      return createBlock(part, part.role === "tool" ? "result" : "answer", part.text);
    }

    return createBlock(part, "tool", part.text ?? part.metadata_json);
  });
}

export function ConversationContentCards({
  activeBlockId,
  blocks,
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  onCopyError,
  resultPreviewLineLimit = DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  t,
  visibility,
}: {
  activeBlockId?: string | null;
  blocks: ConversationContentBlock[];
  colors?: ConversationContentCardColorSettings;
  onCopyError?: (message: string) => void;
  resultPreviewLineLimit?: number;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  const visibleBlocks = blocks.filter((block) => visibility[block.type]);
  const [copiedBlockId, setCopiedBlockId] = useState<string | null>(null);
  const copiedResetTimerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      clearCopiedResetTimer(copiedResetTimerRef);
    },
    [],
  );

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

  if (visibleBlocks.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">
        {t("conversation.content.hidden")}
      </div>
    );
  }

  return (
    <div className="grid gap-3">
      {visibleBlocks.map((block) => (
        <ConversationContentCard
          block={block}
          colors={colors}
          copied={copiedBlockId === block.id}
          highlighted={activeBlockId === block.id}
          key={block.id}
          onCopy={() => void handleCopyBlock(block)}
          resultPreviewLineLimit={resultPreviewLineLimit}
          t={t}
        />
      ))}
    </div>
  );
}

function ConversationContentCard({
  block,
  colors,
  copied,
  highlighted,
  onCopy,
  resultPreviewLineLimit,
  t,
}: {
  block: ConversationContentBlock;
  colors: ConversationContentCardColorSettings;
  copied: boolean;
  highlighted: boolean;
  onCopy: () => void;
  resultPreviewLineLimit: number;
  t: Translator;
}) {
  const label = t(`conversation.content.${block.type}` as TranslationKey);
  const role = t(`conversation.part.role.${block.role}` as TranslationKey);
  const accentColor = colors[block.type];
  const copyLabel = copied
    ? t("conversation.content.copied")
    : t("conversation.content.copy", { type: label });

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
          {icons[block.type]}
          <span>{label}</span>
        </div>
        <div className="flex items-center gap-1.5 text-label-caps">
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
        </div>
      </header>
      <div className="px-4 py-3">
        {block.type === "code" || block.type === "command" ? (
          <pre className="overflow-auto text-code-sm leading-6 text-on-surface">
            <code>{block.text}</code>
          </pre>
        ) : block.type === "result" ? (
          <CommandResultPreview lineLimit={resultPreviewLineLimit} t={t} value={block.text} />
        ) : (
          <MarkdownContent value={block.text} />
        )}
        <BlockMetadata block={block} t={t} />
      </div>
    </section>
  );
}

function CommandResultPreview({
  lineLimit,
  t,
  value,
}: {
  lineLimit: number;
  t: Translator;
  value: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const safeLineLimit = Number.isFinite(lineLimit)
    ? Math.max(1, Math.round(lineLimit))
    : DEFAULT_RESULT_PREVIEW_LINE_LIMIT;
  const formattedValue = formatCommandResultText(value);
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

function formatCommandResultText(value: string) {
  return value
    .replace(/\r\n?/g, "\n")
    .replace(/\bOutput:\s+(?=\S)/g, "Output:\n")
    .replace(
      /[ \t]+(?=(?:\.{1,2}\/|~\/|\/)[^\s:\n][^\s\n]{0,240}:\d+(?::\d+)?:)/g,
      "\n",
    )
    .replace(/[ \t]+(?=Chunk ID:\s*[a-z0-9])/gi, "\n")
    .trimEnd();
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
  suffix = type,
  metadataMode: "all" | "command" | "result" = "all",
): ConversationContentBlock[] {
  const text = value?.trim();
  if (!text) return [];

  return [
    {
      id: `${part.id}-${suffix}`,
      type,
      role: part.role,
      text,
      language: metadataMode === "result" ? null : part.language,
      cwd: metadataMode === "result" ? null : part.cwd,
      status: metadataMode === "command" ? null : part.status,
      exitCode: metadataMode === "command" ? null : part.exit_code,
    },
  ];
}

function commandOutput(part: ConversationPart) {
  const text = part.text?.trim();
  if (text && text !== part.command?.trim()) return text;
  if (part.status) return part.status;
  if (part.exit_code != null) return `Exit code ${part.exit_code}`;
  return null;
}

function isToolResult(part: ConversationPart) {
  if (part.status || part.exit_code != null) return true;

  const metadata = part.metadata_json?.toLowerCase() ?? "";
  return [
    "tool_result",
    "tool-result",
    "tool_output",
    "tooloutput",
    "function_call_output",
    '"output"',
    '"result"',
  ].some((marker) => metadata.includes(marker));
}
