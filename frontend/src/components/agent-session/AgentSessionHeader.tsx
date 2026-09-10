import type { ReactNode } from "react";
import type { AgentSessionStatus } from "../../types/agentSession";

export interface AgentSessionHeaderProps {
  recipientTitle?: ReactNode;
  status?: AgentSessionStatus;
  headerActions?: ReactNode;
  testIdPrefix?: string;
}

export function AgentSessionHeader({
  headerActions,
  recipientTitle,
  status,
  testIdPrefix = "agent-session",
}: AgentSessionHeaderProps) {
  return (
    <div className="sticky top-0 z-10 flex shrink-0 items-center justify-between gap-3 border-b border-theme-card-border/45 bg-theme-card-header/85 px-4 py-2.5 backdrop-blur sm:px-5">
      <div className="min-w-0">
        <h3
          className="truncate text-title-sm font-bold text-on-surface"
          data-testid={`${testIdPrefix}-active-recipient`}
        >
          {recipientTitle}
        </h3>
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
