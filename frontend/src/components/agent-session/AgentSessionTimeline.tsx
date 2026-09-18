import { ArrowDown, MessageSquare } from "lucide-react";
import { useMemo, type ReactNode, type RefObject } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  AgentSessionItemView,
  AgentSessionRestoreStateInfo,
} from "../../types/agentSession";
import { EmptyState } from "../foundation/EmptyState";
import { Button } from "../ui/button";
import { buildAgentSessionTurns } from "./agentSessionReducer";
import { AgentSessionTurn } from "./AgentSessionTurn";
import { useAgentSessionExpansion } from "./useAgentSessionExpansion";

export interface AgentSessionTimelineProps {
  items: AgentSessionItemView[];
  timelineRef?: RefObject<HTMLDivElement | null>;
  onScroll?: () => void;
  showNewActivity?: boolean;
  onScrollToLatest?: () => void;
  restoreState?: AgentSessionRestoreStateInfo | null;
  timelineExtra?: ReactNode;
  emptyTitle?: string;
  emptyDescription?: string;
  emptyIcon?: ReactNode;
  testIdPrefix?: string;
  ariaLabel?: string;
}

export function AgentSessionTimeline({
  ariaLabel,
  emptyDescription,
  emptyIcon,
  emptyTitle,
  items,
  onScroll,
  onScrollToLatest,
  restoreState,
  showNewActivity = false,
  testIdPrefix = "agent-session",
  timelineExtra,
  timelineRef,
}: AgentSessionTimelineProps) {
  const { t } = useI18n();
  const turns = useMemo(() => buildAgentSessionTurns(items), [items]);
  const expansion = useAgentSessionExpansion();

  const hasContent = items.length > 0 || Boolean(timelineExtra);

  return (
    <div
      aria-atomic="false"
      aria-label={
        ariaLabel || t("agentSession.timeline") || t("team.chat.sessionArea")
      }
      aria-live="polite"
      className="relative min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-5"
      data-testid={`${testIdPrefix}-timeline`}
      id={`${testIdPrefix}-session-timeline`}
      onScroll={onScroll}
      ref={timelineRef}
      role="log"
      tabIndex={0}
    >
      {showNewActivity ? (
        <div className="pointer-events-none absolute inset-x-0 top-2 z-20 flex justify-center">
          <Button
            className="pointer-events-auto shadow-[var(--theme-shadow-panel)]"
            data-testid={`${testIdPrefix}-new-activity`}
            onClick={onScrollToLatest}
            size="sm"
            type="button"
            variant="secondary"
          >
            <ArrowDown size={14} />
            {t("agentSession.newActivity") || t("team.chat.newActivity")}
          </Button>
        </div>
      ) : null}

      {restoreState ? (
        <div
          aria-live="polite"
          className={`mb-3 flex flex-wrap items-center gap-2 rounded-lg border px-3 py-2 text-caption ${
            restoreState.className ||
            "border-theme-nav-active-border/40 bg-theme-nav-active/10 text-on-surface-variant"
          }`}
          data-state={restoreState.state}
          data-testid={`${testIdPrefix}-restore-status`}
          role="status"
        >
          {restoreState.icon}
          <span className="font-semibold">{restoreState.label}</span>
          {restoreState.errorCode ? (
            <code>{restoreState.errorCode}</code>
          ) : null}
        </div>
      ) : null}

      {hasContent ? (
        <ol className="mx-auto grid w-full max-w-3xl gap-3">
          {turns.map((turn) => (
            <AgentSessionTurn
              expansion={expansion}
              key={turn.turnId}
              testIdPrefix={testIdPrefix}
              turn={turn}
            />
          ))}
          {timelineExtra}
        </ol>
      ) : (
        <EmptyState
          className="min-h-56 border-0 bg-transparent shadow-none"
          data-testid={`${testIdPrefix}-empty-state`}
          description={
            emptyDescription ||
            t("agentSession.emptyDescription") ||
            t("team.chat.emptyDescription") ||
            "This session has not recorded any activity."
          }
          icon={emptyIcon || <MessageSquare size={21} />}
          title={
            emptyTitle ||
            t("agentSession.emptyTitle") ||
            t("team.chat.emptyTitle") ||
            "No activity yet"
          }
        />
      )}
    </div>
  );
}
