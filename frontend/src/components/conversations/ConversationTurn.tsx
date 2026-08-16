import { Check, Copy, Scissors } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type {
  ConversationQuestionDetail,
  ConversationRecordKind,
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import type { Translator } from "../../i18n/I18nProvider";
import type {
  ConversationContentBlock,
  ConversationContentVisibility,
  ConversationDisplayNode,
} from "./ConversationContentCards";
import {
  buildConversationContentBlocks,
  buildConversationDisplayNodes,
  conversationCardDomId,
  ConversationContentCards,
} from "./ConversationContentCards";
import type { ConversationContentController } from "./useConversationContentController";
import type {
  ConversationContentCardColorSettings,
  ResolvedConversationTranslationSettings,
} from "../../store/settings/AppSettingsProvider";
import { conversationIdFragment } from "../../utils/conversationIds";
import { MarkdownContent } from "./ConversationMarkdown";

export interface ConversationTurnPresentation {
  blocks: ConversationContentBlock[];
  displayNodes?: ConversationDisplayNode[];
  hasContent: boolean;
  promptBlockId: string;
  turn: ConversationTurnRecord;
}

export function buildConversationTurnPresentations(
  question: ConversationQuestionDetail,
): ConversationTurnPresentation[] {
  const usesStructuredContent = Boolean(question.cards && question.content_nodes);
  return question.turns.map((turn) => {
    const turnParts = usesStructuredContent
      ? []
      : question.parts.filter((part) => part.turn_id === turn.id);
    const displayNodes = usesStructuredContent
      ? buildConversationDisplayNodes(
          question.cards ?? [],
          question.content_nodes?.filter((node) => node.turn_id === turn.id) ?? [],
        )
      : undefined;
    const blocks = displayNodes
      ? []
      : buildConversationContentBlocks(
          turnParts,
          question.cards?.filter((card) => turnParts.some((part) => part.id === card.part_id)),
        );

    return {
      blocks,
      displayNodes,
      hasContent: displayNodes ? displayNodes.length > 0 : blocks.length > 0,
      promptBlockId: `${turn.id}-question`,
      turn,
    };
  });
}

export function collectConversationTurnBlocks(
  models: readonly ConversationTurnPresentation[],
): ConversationContentBlock[] {
  return models.flatMap((model) => (
    model.displayNodes
      ? model.displayNodes.flatMap((node) => node.type === "card"
        ? [node.block]
        : [...(node.command ? [node.command] : []), ...node.results])
      : model.blocks
  ));
}

export function buildConversationBlockTurnIndex(
  models: readonly ConversationTurnPresentation[],
): Map<string, string> {
  const index = new Map<string, string>();
  for (const model of models) {
    index.set(model.promptBlockId, model.turn.id);
    for (const block of model.blocks) {
      index.set(block.id, model.turn.id);
      for (const legacyAnchorId of block.legacyAnchorIds ?? []) index.set(legacyAnchorId, model.turn.id);
    }
    for (const node of model.displayNodes ?? []) {
      if (node.type === "card") {
        index.set(node.block.id, model.turn.id);
        for (const legacyAnchorId of node.block.legacyAnchorIds ?? []) index.set(legacyAnchorId, model.turn.id);
        continue;
      }
      index.set(node.sourceExecutionId, model.turn.id);
      for (const block of [node.command, ...node.results]) {
        if (!block) continue;
        index.set(block.id, model.turn.id);
        for (const legacyAnchorId of block.legacyAnchorIds ?? []) index.set(legacyAnchorId, model.turn.id);
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
  onSplit,
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
  onSplit?: (turnId: string) => void;
  recordKind: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility: ConversationContentVisibility;
}) {
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<number | null>(null);
  const promptHighlighted = activeBlockId === model.promptBlockId;

  useEffect(() => () => {
    if (copiedTimerRef.current !== null) window.clearTimeout(copiedTimerRef.current);
  }, []);

  async function copyPrompt() {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard API is unavailable");
      await navigator.clipboard.writeText(model.turn.user_text);
      if (copiedTimerRef.current !== null) window.clearTimeout(copiedTimerRef.current);
      setCopied(true);
      copiedTimerRef.current = window.setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 1400);
    } catch (error) {
      onCopyError?.(t("conversation.content.copyFailed", { message: errorMessage(error) }));
    }
  }

  return (
    <section className="conversation-turn mb-6" data-conversation-turn-id={model.turn.id}>
      <div
        className={`conversation-prompt-block scroll-mt-32 rounded-xl border border-primary/30 bg-primary/[0.055] px-4 py-3 transition-shadow ${
          promptHighlighted ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]" : ""
        }`}
        data-conversation-card-id={model.promptBlockId}
        id={conversationCardDomId(model.promptBlockId)}
      >
        <div className="mb-2 flex items-center justify-between gap-3">
          <h3 className="flex items-center gap-2 text-label-caps text-primary">
            <span className="size-2 rounded-full bg-primary" />
            {t("conversation.question.userPrompt")}
          </h3>
          <div className="flex items-center gap-2">
            <span
              className="select-text rounded-md border border-primary/25 bg-theme-card/45 px-1.5 py-0.5 font-mono text-code-sm normal-case text-on-surface-muted"
              title={model.promptBlockId}
            >
              {conversationIdFragment(model.promptBlockId)}
            </span>
            {index > 0 && onSplit ? (
              <ToolbarTextButton
                icon={<Scissors size={15} />}
                label={t("conversation.question.splitHere")}
                onClick={() => onSplit(model.turn.id)}
              />
            ) : null}
            <PromptCopyButton copied={copied} onClick={() => void copyPrompt()} t={t} />
          </div>
        </div>
        <MarkdownContent value={model.turn.user_text} />
      </div>
      <div className="mt-3 pl-3">
        <h3 className="mb-3 text-label-caps text-on-surface-muted">{t("conversation.question.parts")}</h3>
        {!model.hasContent ? (
          <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
        ) : (
          <ConversationContentCards
            activeBlockId={activeBlockId}
            blocks={model.blocks}
            colors={colors}
            controller={controller}
            nodes={model.displayNodes}
            onCopyError={onCopyError}
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

function PromptCopyButton({ copied, onClick, t }: { copied: boolean; onClick: () => void; t: Translator }) {
  const label = copied ? t("conversation.content.copied") : t("conversation.question.copyPrompt");
  return (
    <button
      aria-label={label}
      className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-label-caps text-primary/80 transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
      onClick={onClick}
      title={label}
      type="button"
    >
      {copied ? <Check className="size-[1em]" /> : <Copy className="size-[1em]" />}
    </button>
  );
}

function ToolbarTextButton({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
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
  return <div className="conversation-empty-state m-2 rounded-2xl p-6 text-center text-body-sm text-on-surface-variant">{children}</div>;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
