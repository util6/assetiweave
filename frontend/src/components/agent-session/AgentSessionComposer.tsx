import { Hand, Send, Square } from "lucide-react";
import type { FormEvent, KeyboardEvent, ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionCapabilities } from "../../types/agentSession";
import { Button } from "../ui/button";

export interface AgentSessionComposerProps {
  draft?: string;
  onDraftChange?: (draft: string) => void;
  onSubmit?: () => void | Promise<void>;
  onStop?: () => void | Promise<void>;
  onInterrupt?: () => void | Promise<void>;
  onQueue?: (message: string) => void | Promise<void>;
  disabled?: boolean;
  canSend?: boolean;
  isExecuting?: boolean;
  capabilities?: AgentSessionCapabilities;
  placeholder?: string;
  recipientTitle?: ReactNode;
  composerExtra?: ReactNode;
  statusLabel?: ReactNode;
  submitLabel?: string;
  stopLabel?: string;
  interruptLabel?: string;
  queueLabel?: string;
  testIdPrefix?: string;
}

export function AgentSessionComposer({
  canSend = true,
  capabilities,
  composerExtra,
  disabled = false,
  draft = "",
  interruptLabel,
  isExecuting = false,
  onDraftChange,
  onInterrupt,
  onQueue,
  onStop,
  onSubmit,
  placeholder,
  queueLabel,
  recipientTitle,
  statusLabel,
  stopLabel,
  submitLabel,
  testIdPrefix = "agent-session",
}: AgentSessionComposerProps) {
  const { t } = useI18n();

  const isStopMode = Boolean(isExecuting && capabilities?.stop);
  const isInterruptMode = Boolean(
    isExecuting && !capabilities?.stop && capabilities?.interrupt,
  );
  const isQueueMode = Boolean(
    isExecuting && !capabilities?.stop && capabilities?.queue,
  );

  const handleSubmit = (event?: FormEvent) => {
    event?.preventDefault();
    if (isStopMode) {
      void onStop?.();
      return;
    }
    if (isInterruptMode) {
      void onInterrupt?.();
      return;
    }
    if (isQueueMode) {
      if (draft.trim()) {
        void onQueue?.(draft.trim());
      }
      return;
    }
    if (canSend && !disabled && draft.trim()) {
      void onSubmit?.();
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const isComposing =
      event.nativeEvent.isComposing ||
      Boolean((event as unknown as { isComposing?: boolean }).isComposing);
    if (isComposing) {
      return;
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSubmit(event);
    }
  };

  return (
    <section
      aria-label={
        t("agentSession.composerLabel") || t("team.chat.composerLabel")
      }
      className="sticky bottom-0 shrink-0 border-t border-theme-card-border/65 bg-theme-card-header/90 px-4 py-3 shadow-[0_-10px_24px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur sm:px-5"
      data-testid={`${testIdPrefix}-composer`}
    >
      {recipientTitle || statusLabel ? (
        <div className="mb-2 flex flex-wrap items-center justify-between gap-2 text-caption">
          {recipientTitle ? (
            <>
              <span className="text-on-surface-variant">
                {t("agentSession.recipient") || t("team.chat.recipient")}
              </span>
              <span className="font-semibold text-primary">
                {recipientTitle}
              </span>
            </>
          ) : null}
          {statusLabel ? (
            <span className="ml-auto text-on-surface-variant">
              {statusLabel}
            </span>
          ) : null}
        </div>
      ) : null}

      {composerExtra}

      <form className="flex items-end gap-2" onSubmit={handleSubmit}>
        <textarea
          aria-label={
            t("agentSession.composerInput") || t("team.chat.composerInput")
          }
          className="min-h-16 min-w-0 flex-1 resize-none rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 py-2.5 text-body-sm text-on-surface shadow-[var(--theme-shadow-control-inset)] outline-none placeholder:text-outline focus:border-primary-strong/65 focus:ring-2 focus:ring-primary-strong/25 disabled:cursor-not-allowed disabled:opacity-70"
          disabled={disabled || (isExecuting && !capabilities?.queue)}
          onChange={(e) => onDraftChange?.(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={
            placeholder ||
            t("agentSession.composerPlaceholder") ||
            t("team.chat.composerPlaceholderFallback")
          }
          rows={2}
          value={draft}
        />
        {isStopMode ? (
          <Button
            aria-label={stopLabel || t("agentSession.stop")}
            data-testid={`${testIdPrefix}-stop`}
            disabled={disabled}
            onClick={() => void onStop?.()}
            size="sm"
            type="button"
            variant="destructive"
          >
            <Square size={14} />
            {stopLabel || t("agentSession.stop")}
          </Button>
        ) : isInterruptMode ? (
          <Button
            aria-label={interruptLabel || t("agentSession.interrupt")}
            data-testid={`${testIdPrefix}-interrupt`}
            disabled={disabled}
            onClick={() => void onInterrupt?.()}
            size="sm"
            type="button"
            variant="secondary"
          >
            <Hand size={14} />
            {interruptLabel || t("agentSession.interrupt")}
          </Button>
        ) : isQueueMode ? (
          <Button
            aria-label={queueLabel || t("agentSession.queue")}
            data-testid={`${testIdPrefix}-queue`}
            disabled={!draft.trim() || disabled}
            onClick={() => {
              if (draft.trim()) void onQueue?.(draft.trim());
            }}
            size="sm"
            type="button"
            variant="secondary"
          >
            {queueLabel || t("agentSession.queue")}
          </Button>
        ) : (
          <Button
            aria-label={
              submitLabel || t("agentSession.send") || t("team.chat.send")
            }
            data-testid={`${testIdPrefix}-send`}
            disabled={!canSend || disabled || !draft.trim()}
            size="sm"
            type="submit"
          >
            <Send size={14} />
            {submitLabel || t("agentSession.send") || t("team.chat.send")}
          </Button>
        )}
      </form>
    </section>
  );
}
