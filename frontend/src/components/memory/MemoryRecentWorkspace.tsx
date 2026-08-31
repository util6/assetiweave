import { Clock3, FolderOpen, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import { listMemoryRecent } from "../../services/memory";
import type { RecentConversationView, RecentMemoryEvent, RecentMemorySession } from "../../types/memory";
import { EmptyState } from "../foundation/EmptyState";
import { Button } from "../ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
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
    return <div className="grid min-h-0 flex-1 place-items-center text-body-sm text-on-surface-variant">{t("common.loading")}</div>;
  }
  if (sessions && sessions.length === 0) {
    return <EmptyState className="min-h-0 flex-1" description={t("memory.recent.emptyDescription")} icon={<Clock3 size={20} />} title={t("memory.recent.emptyTitle")} />;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      <div className="flex shrink-0 items-center justify-between gap-3">
        <div className="flex gap-2">
          <Button onClick={() => setView("project")} variant={view === "project" ? "default" : "outline"}>{t("memory.recent.projectView")}</Button>
          <Button onClick={() => setView("time")} variant={view === "time" ? "default" : "outline"}>{t("memory.recent.timeView")}</Button>
        </div>
        <Button aria-label={t("memory.recent.refresh")} onClick={() => setReloadKey((value) => value + 1)} variant="outline"><RefreshCw size={15} />{t("memory.recent.refresh")}</Button>
      </div>
      {error ? <div className="rounded-md border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">{error}</div> : null}
      <div className="min-h-0 flex-1 overflow-auto pr-1">
        {view === "project" ? groups.map(([project, items]) => (
          <section className="mb-4 grid gap-2" key={project}>
            <h2 className="flex items-center gap-2 px-1 text-label-caps text-on-surface-variant"><FolderOpen size={14} />{project}</h2>
            {items.map((session) => <RecentSessionCard key={`${session.session.title}:${session.last_activity_at}`} session={session} onEventOpen={onEventOpen} t={t} />)}
          </section>
        )) : sessions?.map((session) => <RecentSessionCard key={`${session.session.title}:${session.last_activity_at}`} session={session} onEventOpen={onEventOpen} t={t} />)}
      </div>
    </div>
  );
}

function RecentSessionCard({ session, onEventOpen, t }: { session: RecentMemorySession; onEventOpen?: (event: RecentMemoryEvent) => void; t: Translator }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{session.session.title}</CardTitle>
        <div className="text-label-sm text-on-surface-variant">{session.source_agent} · {formatTime(session.last_activity_at)} · {session.question_count} {t("memory.recent.questions")}</div>
      </CardHeader>
      <CardContent className="grid gap-2">
        {session.recent_events.length ? session.recent_events.map((event) => (
          <button className="grid gap-1 rounded-md border border-theme-card-border px-3 py-2 text-left hover:bg-surface-container-high" key={event.id} onClick={() => onEventOpen?.(event)} type="button">
            <span className="text-label-md">{event.title}</span>
            <MarkdownContent value={event.summary} />
            <span className="text-label-sm text-on-surface-variant">{event.category} · {formatTime(event.occurred_at)}</span>
          </button>
        )) : <div className="text-body-sm text-on-surface-variant">{t("memory.recent.noEvents")}</div>}
      </CardContent>
    </Card>
  );
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
