import { CircleAlert } from "lucide-react";
import { useMemo } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionWorkspaceProps } from "../../types/agentSession";
import { EmptyState } from "../foundation/EmptyState";
import { AgentSessionComposer } from "./AgentSessionComposer";
import { AgentSessionHeader } from "./AgentSessionHeader";
import { AgentSessionTimeline } from "./AgentSessionTimeline";
import { useAgentSessionAutoScroll } from "./useAgentSessionAutoScroll";

export function AgentSessionWorkspace({
  activityDependencyKey,
  canSend,
  capabilities,
  className = "",
  composerExtra,
  disabled = false,
  draft = "",
  emptyDescription,
  emptyIcon,
  emptyTitle,
  headerActions,
  isExecuting = false,
  isReadOnly = false,
  items,
  model,
  onDraftChange,
  onInterrupt,
  onQueue,
  onSend,
  onStop,
  placeholder,
  recipientTitle,
  restoreState,
  sessionResetKey,
  status,
  stopLabel,
  submitLabel,
  testIdPrefix = "agent-session",
  timelineExtra,
  unavailable = false,
  unavailableDescription,
}: AgentSessionWorkspaceProps) {
  const { t } = useI18n();

  const fallbackActivityKey = useMemo(
    () =>
      JSON.stringify([
        items.map((i) => [i.id, i.sequence, i.state, i.text, i.code]),
        restoreState?.state,
      ]),
    [items, restoreState?.state],
  );

  const effectiveActivityKey = activityDependencyKey ?? fallbackActivityKey;

  const {
    onTimelineScroll,
    scrollTimelineToLatest,
    showNewActivity,
    timelineRef,
  } = useAgentSessionAutoScroll(effectiveActivityKey, sessionResetKey);

  const isSendAllowed = Boolean(
    capabilities.send &&
      (canSend !== undefined ? canSend : draft.trim().length > 0 && !disabled),
  );

  const handleSend = () => {
    if (!isSendAllowed && !disabled) return;
    return onSend?.(draft);
  };

  const showHeader = Boolean(
    recipientTitle || status || headerActions || model || isReadOnly,
  );

  return (
    <section
      aria-label="Agent Session Workspace"
      className={`flex min-h-0 flex-1 flex-col bg-theme-panel/25 ${className}`}
      data-testid={`${testIdPrefix}-workspace`}
    >
      {showHeader ? (
        <AgentSessionHeader
          headerActions={headerActions}
          isReadOnly={isReadOnly || !capabilities.send}
          model={model}
          recipientTitle={recipientTitle}
          status={status}
          testIdPrefix={testIdPrefix}
        />
      ) : null}

      {unavailable ? (
        <div
          className="flex min-h-0 flex-1 items-center justify-center p-6"
          data-testid={`${testIdPrefix}-unavailable`}
        >
          <EmptyState
            description={
              unavailableDescription ||
              t("agentSession.unavailableDescription")
            }
            icon={<CircleAlert size={28} className="text-status-warning" />}
            title={t("agentSession.unavailableTitle")}
          />
        </div>
      ) : (
        <>
          <AgentSessionTimeline
            emptyDescription={emptyDescription}
            emptyIcon={emptyIcon}
            emptyTitle={emptyTitle}
            items={items}
            onScroll={onTimelineScroll}
            onScrollToLatest={() => scrollTimelineToLatest("smooth")}
            restoreState={restoreState}
            showNewActivity={showNewActivity}
            testIdPrefix={testIdPrefix}
            timelineExtra={timelineExtra}
            timelineRef={timelineRef}
          />

          {capabilities.send ? (
            <AgentSessionComposer
              canSend={isSendAllowed}
              capabilities={capabilities}
              composerExtra={composerExtra}
              disabled={disabled}
              draft={draft}
              isExecuting={isExecuting}
              onDraftChange={onDraftChange}
              onInterrupt={onInterrupt}
              onQueue={onQueue}
              onStop={onStop}
              onSubmit={handleSend}
              placeholder={placeholder}
              recipientTitle={recipientTitle}
              stopLabel={stopLabel}
              submitLabel={submitLabel}
              testIdPrefix={testIdPrefix}
            />
          ) : null}
        </>
      )}
    </section>
  );
}
