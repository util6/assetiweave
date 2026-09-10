import { Eye } from "lucide-react";
import type { ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionStatus } from "../../types/agentSession";

export interface AgentSessionHeaderProps {
  recipientTitle?: ReactNode;
  model?: string | null;
  status?: AgentSessionStatus;
  headerActions?: ReactNode;
  testIdPrefix?: string;
  isReadOnly?: boolean;
}

export function AgentSessionHeader({
  headerActions,
  isReadOnly = false,
  model,
  recipientTitle,
  status,
  testIdPrefix = "agent-session",
}: AgentSessionHeaderProps) {
  const { t } = useI18n();

  return (
    <div className="sticky top-0 z-10 flex shrink-0 items-center justify-between gap-3 border-b border-theme-card-border/45 bg-theme-card-header/85 px-4 py-2.5 backdrop-blur sm:px-5">
      <div className="flex min-w-0 items-center gap-2">
        <h3
          className="truncate text-title-sm font-bold text-on-surface"
          data-testid={`${testIdPrefix}-active-recipient`}
        >
          {recipientTitle}
        </h3>
        {model ? (
          <span
            className="rounded border border-theme-control-border/60 bg-theme-control/60 px-1.5 py-0.5 text-caption font-mono text-on-surface-variant"
            data-testid={`${testIdPrefix}-header-model`}
          >
            {model}
          </span>
        ) : null}
        {isReadOnly ? (
          <span
            className="inline-flex items-center gap-1 rounded bg-theme-control/40 px-2 py-0.5 text-caption text-outline"
            data-testid={`${testIdPrefix}-header-readonly`}
          >
            <Eye size={12} />
            {t("agentSession.readOnly")}
          </span>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-3">
        {status ? (
          <div
            className={`flex shrink-0 items-center gap-1.5 text-caption ${status.className}`}
          >
            {status.icon}
            <span>{status.label}</span>
          </div>
        ) : null}
        {headerActions}
      </div>
    </div>
  );
}
