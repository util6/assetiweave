import { Send } from "lucide-react";
import type { FormEvent, KeyboardEvent, ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import { Button } from "../ui/button";

export interface AgentSessionComposerProps {
  draft?: string;
  onDraftChange?: (draft: string) => void;
  onSubmit?: () => void | Promise<void>;
  disabled?: boolean;
  canSend?: boolean;
  placeholder?: string;
  recipientTitle?: ReactNode;
  composerExtra?: ReactNode;
  statusLabel?: ReactNode;
  submitLabel?: string;
  testIdPrefix?: string;
}

export function AgentSessionComposer({
  canSend = true,
  composerExtra,
  disabled = false,
  draft = "",
  onDraftChange,
  onSubmit,
  placeholder,
  recipientTitle,
  statusLabel,
  submitLabel,
  testIdPrefix = "agent-session",
}: AgentSessionComposerProps) {
  const { t } = useI18n();

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    void onSubmit?.();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      void onSubmit?.();
    }
  };

  return (
    <section
      aria-label={t("agentSession.composerLabel") || t("team.chat.composerLabel")}
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
          disabled={disabled}
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
        <Button
          aria-label={
            submitLabel || t("agentSession.send") || t("team.chat.send")
          }
          disabled={!canSend || disabled}
          size="sm"
          type="submit"
        >
          <Send size={14} />
          {submitLabel || t("agentSession.send") || t("team.chat.send")}
        </Button>
      </form>
    </section>
  );
}
