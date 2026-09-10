import { useMemo } from "react";
import type { AgentSessionWorkspaceProps } from "../../types/agentSession";
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
  items,
  onDraftChange,
  onSend,
  placeholder,
  recipientTitle,
  restoreState,
  sessionResetKey,
  status,
  submitLabel,
  testIdPrefix = "agent-session",
  timelineExtra,
}: AgentSessionWorkspaceProps) {
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

  return (
    <section
      aria-label="Agent Session Workspace"
      className={`flex min-h-0 flex-1 flex-col bg-theme-panel/25 ${className}`}
      data-testid={`${testIdPrefix}-workspace`}
    >
      {recipientTitle || status || headerActions ? (
        <AgentSessionHeader
          headerActions={headerActions}
          recipientTitle={recipientTitle}
          status={status}
          testIdPrefix={testIdPrefix}
        />
      ) : null}

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
          composerExtra={composerExtra}
          disabled={disabled}
          draft={draft}
          onDraftChange={onDraftChange}
          onSubmit={handleSend}
          placeholder={placeholder}
          recipientTitle={recipientTitle}
          submitLabel={submitLabel}
          testIdPrefix={testIdPrefix}
        />
      ) : null}
    </section>
  );
}
