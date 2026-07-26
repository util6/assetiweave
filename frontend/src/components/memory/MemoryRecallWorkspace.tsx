import { AlertTriangle, BrainCircuit, Eye, LoaderCircle, Search, Square } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useMemoryTasks } from "../../app/backgroundTasks/MemoryTaskProvider";
import type { Translator } from "../../i18n/I18nProvider";
import { memoryRecallRunResultSchema } from "../../schemas/memory";
import { previewMemoryRecall } from "../../services/memory";
import type { MemoryEvidenceSnapshot, MemoryRecallMode, MemoryRecallPreview, MemoryRecallPreviewParams } from "../../types/memory";
import { EmptyState } from "../foundation/EmptyState";
import { Button } from "../ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import { Input } from "../ui/input";

export function MemoryRecallWorkspace({ onEvidenceOpen, t }: { onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void; t: Translator }) {
  const { cancelTask, startRecall, tasks } = useMemoryTasks();
  const [mode, setMode] = useState<MemoryRecallMode>("exact");
  const [query, setQuery] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [includeUnavailable, setIncludeUnavailable] = useState(false);
  const [offset, setOffset] = useState(0);
  const [preview, setPreview] = useState<MemoryRecallPreview | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const task = [...tasks].reverse().find((item) => item.kind === (mode === "full" ? "full_organize" : "deep_recall")) ?? null;
  const result = useMemo(() => task?.status === "completed" ? memoryRecallRunResultSchema.safeParse(task.result) : null, [task]);

  useEffect(() => {
    if (task?.status === "completed" && result?.success) setPreview(result.data.preview);
  }, [result, task?.status]);

  const params = (pageOffset = offset): MemoryRecallPreviewParams => ({
    mode,
    query: mode === "exact" ? query : null,
    scope: { app_id: null, source_id: null, project_path: projectPath.trim() || null, session_id: null },
    include_unavailable: includeUnavailable,
    limit: mode === "full" ? 100 : 24,
    offset: mode === "full" ? pageOffset : 0,
  });

  async function loadPreview(pageOffset = offset) {
    setBusy(true); setError(null);
    try { setPreview(await previewMemoryRecall(params(pageOffset))); setOffset(pageOffset); }
    catch (loadError) { setError(errorMessage(loadError)); }
    finally { setBusy(false); }
  }

  async function runAi() {
    setBusy(true); setError(null);
    try { await startRecall({ ...params(), synthesize: true }); }
    catch (runError) { setError(errorMessage(runError)); }
    finally { setBusy(false); }
  }

  if (preview === null) return <EmptyState description={t("memory.recall.browserDescription")} icon={<Search size={20} />} title={t("memory.recall.browserTitle")} />;
  const running = task?.status === "running";
  const runResult = result?.success ? result.data : null;

  return <div className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(21rem,0.75fr)_minmax(32rem,1.65fr)]">
    <div className="flex min-h-0 flex-col gap-4 overflow-auto pr-1">
      <Card><CardHeader><CardTitle>{t("memory.recall.strategy")}</CardTitle></CardHeader><CardContent className="grid gap-3">
        <div className="grid grid-cols-2 gap-2">
          <Button onClick={() => { setMode("exact"); setOffset(0); setPreview(undefined); }} variant={mode === "exact" ? "default" : "outline"}>{t("memory.recall.exact")}</Button>
          <Button onClick={() => { setMode("full"); setOffset(0); setPreview(undefined); }} variant={mode === "full" ? "default" : "outline"}>{t("memory.recall.full")}</Button>
        </div>
        {mode === "exact" ? <Input aria-label={t("memory.recall.query")} onChange={(event) => setQuery(event.target.value)} placeholder={t("memory.recall.queryPlaceholder")} value={query} /> : <Input aria-label={t("memory.recall.projectPath")} onChange={(event) => setProjectPath(event.target.value)} placeholder={t("memory.recall.projectPlaceholder")} value={projectPath} />}
        <label className="flex items-center gap-2 text-body-sm"><input checked={includeUnavailable} onChange={(event) => setIncludeUnavailable(event.target.checked)} type="checkbox" />{t("memory.recall.includeUnavailable")}</label>
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy || running || (mode === "exact" ? !query.trim() : !projectPath.trim())} onClick={() => void loadPreview()} variant="outline"><Eye size={15} />{t("memory.recall.preview")}</Button>
          <Button disabled={busy || running || !preview?.evidence.length} onClick={() => void runAi()}>{running ? <LoaderCircle className="animate-spin" size={15} /> : <BrainCircuit size={15} />}{t("memory.recall.synthesize")}</Button>
          {running ? <Button onClick={() => void cancelTask(task.id)} variant="outline"><Square size={14} />{t("common.cancel")}</Button> : null}
        </div>
        {running ? <div className="text-body-sm text-on-surface-variant">{task.phase} · {task.processed_count}/{task.total_count || "?"}</div> : null}
        {error ? <div className="rounded-md border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">{error}</div> : null}
      </CardContent></Card>
      {preview ? <Card><CardHeader><CardTitle>{t("memory.recall.coverage")}</CardTitle></CardHeader><CardContent className="grid grid-cols-2 gap-2 text-center">
        <Metric label={t("memory.recall.total")} value={preview.total_question_count} /><Metric label={t("memory.recall.selected")} value={preview.selected_question_count} />
        <Metric label={t("memory.recall.skipped")} value={preview.skipped_question_count} /><Metric label={t("memory.recall.evidence")} value={preview.evidence_count} /><Metric label={t("memory.recall.characters")} value={preview.input_char_count} />
        <div className="col-span-2 text-body-sm text-on-surface-variant">{t("memory.recall.backend")}: {preview.backend}{preview.truncated ? ` · ${t("memory.recall.truncated")}` : ""}</div>
        {mode === "full" ? <div className="col-span-2 flex justify-end gap-2">
          <Button disabled={busy || running || offset === 0} onClick={() => void loadPreview(Math.max(0, offset - 100))} variant="outline">{t("memory.action.previous")}</Button>
          <Button disabled={busy || running || offset + preview.selected_question_count >= preview.total_question_count} onClick={() => void loadPreview(offset + 100)} variant="outline">{t("memory.action.next")}</Button>
        </div> : null}
      </CardContent></Card> : null}
    </div>
    <Card className="min-h-0 overflow-hidden"><CardHeader><CardTitle>{runResult?.answer_markdown ? t("memory.recall.answer") : t("memory.recall.bundle")}</CardTitle></CardHeader><CardContent className="grid h-[calc(100%-4rem)] min-h-0 content-start gap-4 overflow-auto">
      {runResult?.answer_markdown ? <pre className="whitespace-pre-wrap rounded-md border border-outline-variant bg-surface-container-low p-4 font-sans text-body-md">{runResult.answer_markdown}</pre> : null}
      {runResult?.insufficient_evidence ? <div className="flex gap-2 rounded-md border border-status-warning/40 p-3 text-body-sm"><AlertTriangle size={16} />{t("memory.recall.insufficient")}</div> : null}
      {preview?.questions.length ? preview.questions.map((question) => <div className="rounded-md border border-outline-variant p-3" key={`${question.record_kind}:${question.question_id}`}>
        <div className="text-label-md">{question.session_title} · Q{question.question_index + 1}</div><div className="mt-1 text-body-sm">{question.question_title}</div>
        <div className="mt-2 grid gap-2">{question.evidence_ids.map((id) => { const item = preview.evidence.find((evidence) => evidence.reference === id); return item ? <button className="rounded border border-outline-variant px-2 py-2 text-left text-body-sm hover:bg-surface-container-high" key={id} onClick={() => onEvidenceOpen?.(toNavigationEvidence(item))} type="button"><span className="text-label-sm text-on-surface-variant">{id} · {item.card_type}</span><div className="line-clamp-3">{item.snapshot.excerpt}</div></button> : null; })}</div>
      </div>) : <EmptyState description={t("memory.recall.emptyDescription")} icon={<Search size={20} />} title={t("memory.recall.emptyTitle")} />}
    </CardContent></Card>
  </div>;
}

function Metric({ label, value }: { label: string; value: number }) { return <div className="rounded-md bg-surface-container-low p-3"><div className="text-title-lg">{value}</div><div className="text-label-sm text-on-surface-variant">{label}</div></div>; }
function toNavigationEvidence(item: MemoryRecallPreview["evidence"][number]): MemoryEvidenceSnapshot { return { ...item.snapshot, id: item.reference, created_at: item.snapshot.event_time ?? "", updated_at: item.snapshot.event_time ?? "" }; }
function errorMessage(error: unknown) { return error instanceof Error ? error.message : String(error); }
