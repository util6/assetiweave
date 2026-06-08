import { Braces, CheckCircle2, FileText, Terminal, Wrench } from "lucide-react";
import type { ReactNode } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type {
  ConversationPart,
  ConversationPartRole,
} from "../../types";
import {
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
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
  blocks,
  colors = DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  t,
  visibility,
}: {
  blocks: ConversationContentBlock[];
  colors?: ConversationContentCardColorSettings;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  const visibleBlocks = blocks.filter((block) => visibility[block.type]);

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
        <ConversationContentCard block={block} colors={colors} key={block.id} t={t} />
      ))}
    </div>
  );
}

function ConversationContentCard({
  block,
  colors,
  t,
}: {
  block: ConversationContentBlock;
  colors: ConversationContentCardColorSettings;
  t: Translator;
}) {
  const label = t(`conversation.content.${block.type}` as TranslationKey);
  const role = t(`conversation.part.role.${block.role}` as TranslationKey);
  const accentColor = colors[block.type];

  return (
    <section
      className="overflow-hidden rounded-xl border"
      data-content-type={block.type}
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
        <span className="text-label-caps text-on-surface-muted">{role}</span>
      </header>
      <div className="px-4 py-3">
        {block.type === "code" || block.type === "command" ? (
          <pre className="overflow-auto text-code-sm leading-6 text-on-surface">
            <code>{block.text}</code>
          </pre>
        ) : (
          <MarkdownContent value={block.text} />
        )}
        <BlockMetadata block={block} t={t} />
      </div>
    </section>
  );
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
