import { AlertTriangle, Brain, CheckSquare, History, Sparkles } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useMemoryTasks } from "../../app/backgroundTasks/MemoryTaskProvider";
import type { Translator } from "../../i18n/I18nProvider";
import { getMemoryOverview } from "../../services/memory";
import type { MemoryEvidenceSnapshot, MemoryOverview } from "../../types/memory";
import { EmptyState } from "../foundation/EmptyState";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";

export function MemoryOverviewWorkspace({
  onEvidenceOpen,
  t,
}: {
  onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void;
  t: Translator;
}) {
  const { task } = useMemoryTasks();
  const [overview, setOverview] = useState<MemoryOverview | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const handledTaskRef = useRef<string | null>(null);

  async function refresh() {
    setOverview(await getMemoryOverview());
  }

  useEffect(() => {
    void refresh().catch((loadError) => setError(errorMessage(loadError)));
  }, []);

  useEffect(() => {
    if (!task || task.status === "running" || handledTaskRef.current === task.id) return;
    handledTaskRef.current = task.id;
    void refresh().catch(() => {});
  }, [task?.id, task?.status]);

  if (overview === null) {
    return <EmptyState description={t("memory.overview.browserDescription")} icon={<Brain size={20} />} title={t("memory.overview.browserTitle")} />;
  }
  if (overview === undefined) {
    return <div className="grid min-h-[20rem] place-items-center text-body-sm text-on-surface-variant">{t("common.loading")}</div>;
  }

  return (
    <div className="grid min-h-0 flex-1 content-start gap-4 overflow-auto xl:grid-cols-2">
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><CheckSquare size={17} /> {t("memory.overview.followUps")}</CardTitle></CardHeader>
        <CardContent className="grid gap-2">
          {overview.follow_ups.length ? overview.follow_ups.map((item) => <OverviewItem key={item.id} title={item.title} content={item.content_markdown} />) : <Muted text={t("memory.overview.noFollowUps")} />}
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><History size={17} /> {t("memory.overview.recent")}</CardTitle></CardHeader>
        <CardContent className="grid gap-2">
          {overview.recent_items.length ? overview.recent_items.map((item) => <OverviewItem key={item.id} title={item.title} content={item.content_markdown} />) : <Muted text={t("memory.overview.noRecent")} />}
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><Sparkles size={17} /> {t("memory.overview.latestDream")}</CardTitle></CardHeader>
        <CardContent>
          {overview.latest_dream ? (
            <div className="grid gap-3">
              <pre className="whitespace-pre-wrap font-sans text-body-sm">{overview.latest_dream.note.markdown}</pre>
              <div className="flex flex-wrap gap-2">
                {overview.latest_dream.evidence.slice(0, 6).map((evidence) => (
                  <button className="rounded border border-outline-variant px-2 py-1 text-label-sm hover:bg-surface-container-high" key={evidence.id} onClick={() => onEvidenceOpen?.(evidence)} type="button">{evidence.block_id}</button>
                ))}
              </div>
            </div>
          ) : <Muted text={t("memory.overview.noDream")} />}
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><AlertTriangle size={17} /> {t("memory.overview.attention")}</CardTitle></CardHeader>
        <CardContent className="grid grid-cols-2 gap-3">
          <Metric label={t("memory.overview.candidates")} value={overview.candidate_count} />
          <Metric label={t("memory.overview.stale")} value={overview.stale_count} />
          <div className="col-span-2 rounded-md border border-outline-variant p-3">
            <div className="text-label-md">{t("memory.overview.nextDream")}</div>
            <div className="mt-2 grid gap-1 text-body-sm text-on-surface-variant">
              {overview.dream_status.gates.map((gate) => <div key={gate.gate}>{gate.passed ? "✓" : "○"} {gate.message}</div>)}
            </div>
          </div>
        </CardContent>
      </Card>
      {error ? <div className="xl:col-span-2 rounded-md border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">{error}</div> : null}
    </div>
  );
}

function OverviewItem({ title, content }: { title: string; content: string }) {
  return <div className="rounded-md border border-outline-variant px-3 py-2"><div className="text-label-md">{title}</div><div className="mt-1 line-clamp-2 text-body-sm text-on-surface-variant">{content}</div></div>;
}

function Metric({ label, value }: { label: string; value: number }) {
  return <div className="rounded-md bg-surface-container-low p-3 text-center"><div className="text-title-lg">{value}</div><div className="text-label-sm text-on-surface-variant">{label}</div></div>;
}

function Muted({ text }: { text: string }) {
  return <div className="text-body-sm text-on-surface-variant">{text}</div>;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
