import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import { conversationIdFragment } from "../../utils/conversationIds";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";

const COPY_FEEDBACK_MS = 1600;

/**
 * 会话短 ID 胶囊：展示可直接在 Agent 侧使用的 8 位短 ID，点击即复制。
 * 短 ID 与 CLI / Outline 使用的片段保持一致（conversationIdFragment）。
 */
export function SessionShortIdChip({
  className = "",
  sessionId,
  t,
}: {
  className?: string;
  sessionId: string;
  t: Translator;
}) {
  const shortId = conversationIdFragment(sessionId);
  const [copied, setCopied] = useState(false);
  const resetTimerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (resetTimerRef.current !== null) {
        window.clearTimeout(resetTimerRef.current);
      }
    },
    [],
  );

  const handleCopy = useCallback(() => {
    if (
      !shortId ||
      typeof navigator === "undefined" ||
      typeof navigator.clipboard?.writeText !== "function"
    ) {
      return;
    }
    void navigator.clipboard
      .writeText(shortId)
      .then(() => {
        setCopied(true);
        if (resetTimerRef.current !== null) {
          window.clearTimeout(resetTimerRef.current);
        }
        resetTimerRef.current = window.setTimeout(() => {
          setCopied(false);
          resetTimerRef.current = null;
        }, COPY_FEEDBACK_MS);
      })
      .catch(() => undefined);
  }, [shortId]);

  if (!shortId) return null;

  const label = copied
    ? t("conversation.content.copied")
    : t("conversation.session.copyShortId", { id: shortId });

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={label}
          className={`inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-theme-control-border bg-theme-control/80 px-2.5 text-on-surface-variant shadow-[var(--theme-shadow-control-inset)] transition-colors hover:bg-theme-control hover:text-on-surface ${
            copied ? "text-on-surface" : ""
          } ${className}`}
          onClick={handleCopy}
          type="button"
        >
          <span className="font-mono text-body-sm leading-none">{shortId}</span>
          {copied ? (
            <Check className="size-3 text-status-create" />
          ) : (
            <Copy className="size-3 opacity-60" />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
