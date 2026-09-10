import { Clock3, FolderOpen, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import type { Translator } from "../../i18n/I18nProvider";
import { listMemoryRecent } from "../../services/memory";
import type {
  RecentConversationView,
  RecentMemoryEvent,
  RecentMemorySession,
} from "../../types/memory";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { AppSkeleton } from "../foundation/skeleton";
import { Button } from "../ui/button";
import { MarkdownContent } from "../conversations/ConversationMarkdown";

export function MemoryRecentWorkspace({
  onEventOpen,
  t,
}: {
  onEventOpen?: (event: RecentMemoryEvent) => void;
  t: Translator;
}) {
  const [view, setView] = useState<RecentConversationView>("project");
  const [sessions, setSessions] = useState<RecentMemorySession[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setSessions(null);
    setError(null);
    void listMemoryRecent({ view })
      .then((items) => {
        if (!cancelled) setSessions(items);
      })
      .catch((loadError) => {
        if (!cancelled) setError(errorMessage(loadError));
      });
    return () => {
      cancelled = true;
    };
  }, [reloadKey, view]);

  const groups = useMemo(() => {
    if (!sessions || view === "time") return [];
    const grouped = new Map<string, RecentMemorySession[]>();
    for (const session of sessions) {
      const key = session.project_path ?? t("memory.recent.noProject");
      grouped.set(key, [...(grouped.get(key) ?? []), session]);
    }
    return [...grouped.entries()];
  }, [sessions, t, view]);

  if (sessions === null && !error) {
    return <AppSkeleton label={t("common.loading")} layout="list" />;
  }

  if (sessions && sessions.length === 0) {
    return (
      <EmptyState
        className="min-h-0 flex-1"
        description={t("memory.recent.emptyDescription")}
        icon={<Clock3 size={20} />}
        title={t("memory.recent.emptyTitle")}
      />
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      {/* 视图切换与刷新工具栏 */}
      <div className="flex shrink-0 items-center justify-between gap-3">
        <div className="flex rounded-xl border border-theme-control-border bg-theme-control/40 p-0.5">
          <button
            className={clsx(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-body-sm font-medium transition-colors",
              view === "project"
                ? "bg-surface-elevated text-on-surface shadow-sm"
                : "text-on-surface-variant hover:text-on-surface",
            )}
            onClick={() => setView("project")}
            type="button"
          >
            <FolderOpen size={14} />
            {t("memory.recent.projectView")}
          </button>
          <button
            className={clsx(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-body-sm font-medium transition-colors",
              view === "time"
                ? "bg-surface-elevated text-on-surface shadow-sm"
                : "text-on-surface-variant hover:text-on-surface",
            )}
            onClick={() => setView("time")}
            type="button"
          >
            <Clock3 size={14} />
            {t("memory.recent.timeView")}
          </button>
        </div>

        <Button
          aria-label={t("memory.recent.refresh")}
          onClick={() => setReloadKey((value) => value + 1)}
          size="sm"
          variant="outline"
        >
          <RefreshCw size={14} className="mr-1.5" />
          {t("memory.recent.refresh")}
        </Button>
      </div>

      {error ? (
        <div className="rounded-xl border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">
          {error}
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-auto pr-1">
        {view === "project"
          ? groups.map(([project, items]) => (
              <section className="mb-6 flex flex-col gap-3" key={project}>
                <div className="flex items-center gap-2 px-1 text-label-md font-semibold text-on-surface">
                  <span className="grid size-6 place-items-center rounded-md border border-theme-control-border bg-theme-control/60 text-primary">
                    <FolderOpen size={13} />
                  </span>
                  <span>{project}</span>
                  <span className="text-caption text-on-surface-variant">
                    ({items.length})
                  </span>
                </div>
                <div className="flex flex-col gap-3">
                  {items.map((session) => (
                    <RecentSessionCard
                      key={`${session.session.title}:${session.last_activity_at}`}
                      onEventOpen={onEventOpen}
                      session={session}
                      t={t}
                    />
                  ))}
                </div>
              </section>
            ))
          : sessions?.map((session) => (
              <div className="mb-3" key={`${session.session.title}:${session.last_activity_at}`}>
                <RecentSessionCard
                  onEventOpen={onEventOpen}
                  session={session}
                  t={t}
                />
              </div>
            ))}
      </div>
    </div>
  );
}

function RecentSessionCard({
  session,
  onEventOpen,
  t,
}: {
  session: RecentMemorySession;
  onEventOpen?: (event: RecentMemoryEvent) => void;
  t: Translator;
}) {
  return (
    <Panel className="flex flex-col gap-3 p-4" variant="default">
      <div className="flex items-start justify-between gap-3 border-b border-theme-control-border/60 pb-3">
        <div className="flex flex-col gap-1">
          <h3 className="text-title-sm font-semibold text-on-surface">
            {session.session.title}
          </h3>
          <div className="flex flex-wrap items-center gap-2 text-caption text-on-surface-variant">
            <span className="rounded-md border border-theme-control-border/80 bg-theme-control/40 px-1.5 py-0.5 font-mono text-[11px] text-primary">
              {session.source_agent}
            </span>
            <span>·</span>
            <span>{formatTime(session.last_activity_at)}</span>
            <span>·</span>
            <span>
              {session.question_count} {t("memory.recent.questions")}
            </span>
          </div>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        {session.recent_events.length ? (
          session.recent_events.map((event) => (
            <button
              className="group flex flex-col gap-1.5 rounded-xl border border-theme-control-border/60 bg-theme-control/20 p-3 text-left transition-all hover:border-theme-control-border hover:bg-theme-control/40"
              key={event.id}
              onClick={() => onEventOpen?.(event)}
              type="button"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-body-sm font-medium text-on-surface transition-colors group-hover:text-primary">
                  {event.title}
                </span>
                <span className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-caption font-medium text-on-surface-variant">
                  {event.category}
                </span>
              </div>
              <div className="line-clamp-2 text-body-sm text-on-surface-variant">
                <MarkdownContent value={event.summary} />
              </div>
              <span className="text-caption text-outline">
                {formatTime(event.occurred_at)}
              </span>
            </button>
          ))
        ) : (
          <div className="py-2 text-center text-caption text-on-surface-variant">
            {t("memory.recent.noEvents")}
          </div>
        )}
      </div>
    </Panel>
  );
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
