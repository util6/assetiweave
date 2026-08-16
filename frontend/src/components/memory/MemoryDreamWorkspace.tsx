import { Archive, CheckCircle2, Eye, LoaderCircle, Play, RefreshCw, ShieldAlert, Sparkles, Square } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useMemoryTasks } from "../../app/backgroundTasks/MemoryTaskProvider";
import { EmptyState } from "../foundation/EmptyState";
import { MemoryDreamSkeleton } from "./MemorySkeletons";
import { Button } from "../ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../ui/card";
import {
  archiveMemoryDreamNote,
  getMemoryDreamNote,
  listMemoryDreamNotes,
  previewMemoryDream,
  promoteMemoryDreamNote,
} from "../../services/memory";
import type {
  MemoryDreamNote,
  MemoryDreamNoteDetail,
  MemoryDreamPreview,
  MemoryEvidenceSnapshot,
} from "../../types/memory";
import type { Translator } from "../../i18n/I18nProvider";

export function MemoryDreamWorkspace({
  onEvidenceOpen,
  t,
}: {
  onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void;
  t: Translator;
}) {
  const { autoDreamStatus, cancelTask, refreshAutoDreamStatus, startDream, task } = useMemoryTasks();
  const [notes, setNotes] = useState<MemoryDreamNote[] | null>(null);
  const [detail, setDetail] = useState<MemoryDreamNoteDetail | null>(null);
  const [preview, setPreview] = useState<MemoryDreamPreview | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const handledTaskRef = useRef<string | null>(null);

  async function refreshNotes() {
    const page = await listMemoryDreamNotes({ limit: 100 });
    setNotes(page?.items ?? null);
    setSelectedId((current) => current && page?.items.some((note) => note.id === current)
      ? current
      : page?.items[0]?.id ?? null);
  }

  useEffect(() => {
    void Promise.all([refreshNotes(), refreshAutoDreamStatus()]).catch((loadError) => {
      setError(errorMessage(loadError));
    });
  }, [refreshAutoDreamStatus]);

  useEffect(() => {
    if (!selectedId || notes === null) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    void getMemoryDreamNote(selectedId)
      .then((value) => {
        if (!cancelled) setDetail(value);
      })
      .catch((loadError) => {
        if (!cancelled) setError(errorMessage(loadError));
      });
    return () => {
      cancelled = true;
    };
  }, [notes, selectedId]);

  useEffect(() => {
    if (!task || task.status === "running" || handledTaskRef.current === task.id) return;
    handledTaskRef.current = task.id;
    void Promise.all([refreshNotes(), refreshAutoDreamStatus()]).catch(() => {});
  }, [refreshAutoDreamStatus, task?.id, task?.status]);

  const status = preview ?? autoDreamStatus;
  const running = task?.status === "running" && task.kind === "auto_dream";
  const selected = useMemo(() => notes?.find((note) => note.id === selectedId) ?? null, [notes, selectedId]);

  async function runAction(action: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (actionError) {
      setError(errorMessage(actionError));
    } finally {
      setBusy(false);
    }
  }

  if (notes === null && typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window)) {
    return <EmptyState description={t("memory.dreams.browserDescription")} icon={<Sparkles size={20} />} title={t("memory.dreams.browserTitle")} />;
  }
  if (notes === null) {
    return <MemoryDreamSkeleton label={t("common.loading")} />;
  }

  return (
    <div className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(20rem,0.8fr)_minmax(32rem,1.7fr)]">
      <div className="flex min-h-0 flex-col gap-4 overflow-auto pr-1">
        <Card>
          <CardHeader className="flex-row items-center justify-between gap-3">
            <CardTitle>{t("memory.dreams.gates")}</CardTitle>
            <Button onClick={() => void runAction(async () => {
              setPreview(await previewMemoryDream());
            })} size="sm" type="button" variant="outline">
              <Eye size={15} /> {t("memory.dreams.preview")}
            </Button>
          </CardHeader>
          <CardContent className="grid gap-2">
            {status?.gates.map((gate) => (
              <div className="flex items-start gap-2 rounded-md border border-outline-variant px-3 py-2" key={gate.gate}>
                {gate.passed ? <CheckCircle2 className="mt-0.5 text-status-create" size={16} /> : <ShieldAlert className="mt-0.5 text-status-remove" size={16} />}
                <div className="min-w-0 flex-1">
                  <div className="text-label-md text-on-surface">{gateLabel(gate.gate, t)}</div>
                  <div className="text-body-sm text-on-surface-variant">{gate.message}</div>
                </div>
                {gate.actual !== null && gate.required !== null ? (
                  <span className="text-label-sm text-on-surface-variant">{gate.actual}/{gate.required}</span>
                ) : null}
              </div>
            )) ?? <div className="text-body-sm text-on-surface-variant">{t("common.loading")}</div>}
            {status ? (
              <div className="grid grid-cols-3 gap-2 pt-1 text-center text-body-sm">
                <Metric label={t("memory.dreams.sessions")} value={status.session_count} />
                <Metric label={t("memory.dreams.questions")} value={status.question_count} />
                <Metric label={t("memory.dreams.characters")} value={status.input_char_count} />
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader><CardTitle>{t("memory.dreams.actions")}</CardTitle></CardHeader>
          <CardContent className="flex flex-wrap gap-2">
            <Button disabled={busy || running || !status?.sessions.length} onClick={() => void runAction(() => startDream())} type="button">
              {running ? <LoaderCircle className="animate-spin" size={15} /> : <Play size={15} />}
              {t("memory.dreams.runNow")}
            </Button>
            {running ? (
              <Button onClick={() => void cancelTask(task.id)} type="button" variant="outline"><Square size={14} /> {t("common.cancel")}</Button>
            ) : null}
            <Button onClick={() => void runAction(async () => {
              await Promise.all([refreshNotes(), refreshAutoDreamStatus()]);
            })} type="button" variant="outline"><RefreshCw size={15} /> {t("common.refresh")}</Button>
          </CardContent>
          {running ? (
            <CardContent className="pt-0 text-body-sm text-on-surface-variant">
              {task.phase} · {task.processed_count}/{task.total_count || "?"}
            </CardContent>
          ) : null}
        </Card>

        <Card className="min-h-0">
          <CardHeader><CardTitle>{t("memory.dreams.history")}</CardTitle></CardHeader>
          <CardContent className="grid gap-2">
            {notes?.length ? notes.map((note) => (
              <button
                className={`rounded-md border px-3 py-2 text-left ${selectedId === note.id ? "border-primary bg-primary/8" : "border-outline-variant hover:bg-surface-container-high"}`}
                key={note.id}
                onClick={() => setSelectedId(note.id)}
                type="button"
              >
                <div className="flex items-center justify-between gap-2 text-label-md">
                  <span>{new Date(note.created_at).toLocaleString()}</span>
                  <span className="text-on-surface-variant">{note.status}</span>
                </div>
                <div className="mt-1 line-clamp-2 text-body-sm text-on-surface-variant">{note.markdown}</div>
              </button>
            )) : <div className="text-body-sm text-on-surface-variant">{t("memory.dreams.empty")}</div>}
          </CardContent>
        </Card>
      </div>

      <Card className="min-h-0 overflow-hidden">
        <CardHeader className="flex-row items-center justify-between gap-3">
          <CardTitle>{t("memory.dreams.detail")}</CardTitle>
          {selected ? (
            <div className="flex gap-2">
              <Button disabled={busy || selected.status === "promoted" || selected.status === "archived"} onClick={() => void runAction(async () => {
                await promoteMemoryDreamNote(selected.id);
                await refreshNotes();
              })} size="sm" type="button"><Sparkles size={14} /> {t("memory.dreams.promote")}</Button>
              <Button disabled={busy || selected.status === "archived"} onClick={() => void runAction(async () => {
                await archiveMemoryDreamNote(selected.id);
                await refreshNotes();
              })} size="sm" type="button" variant="outline"><Archive size={14} /> {t("memory.action.archive")}</Button>
            </div>
          ) : null}
        </CardHeader>
        <CardContent className="grid h-[calc(100%-4rem)] min-h-0 grid-rows-[minmax(12rem,auto)_minmax(0,1fr)] gap-4 overflow-auto">
          {detail ? (
            <>
              <pre className="whitespace-pre-wrap rounded-md border border-outline-variant bg-surface-container-low p-4 font-sans text-body-md text-on-surface">{detail.note.markdown}</pre>
              <div className="grid content-start gap-2">
                <div className="text-label-lg">{t("memory.detail.evidence")}</div>
                {detail.evidence.map((evidence) => (
                  <button className="rounded-md border border-outline-variant px-3 py-2 text-left hover:bg-surface-container-high" key={evidence.id} onClick={() => onEvidenceOpen?.(evidence)} type="button">
                    <div className="text-label-sm text-on-surface-variant">{evidence.record_kind} · {evidence.block_id}</div>
                    <div className="mt-1 line-clamp-3 text-body-sm">{evidence.excerpt}</div>
                  </button>
                ))}
              </div>
            </>
          ) : <EmptyState description={t("memory.dreams.selectDescription")} icon={<Sparkles size={20} />} title={t("memory.dreams.selectTitle")} />}
          {error ? <div className="rounded-md border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">{error}</div> : null}
        </CardContent>
      </Card>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return <div className="rounded-md bg-surface-container-low px-2 py-2"><div className="text-title-sm">{value}</div><div className="text-label-sm text-on-surface-variant">{label}</div></div>;
}

function gateLabel(gate: string, t: Translator) {
  const key = `memory.dreams.gate.${gate}` as Parameters<Translator>[0];
  return t(key);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
