import { LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useCatalogTasks } from "./CatalogTaskProvider";

export function CatalogTaskIndicator() {
  const { sourceScan, batchMount, cancelSourceScan, cancelBatchMount } =
    useCatalogTasks();
  const [cancelling, setCancelling] = useState(false);

  const isSourceActive =
    sourceScan &&
    (sourceScan.state === "running" || sourceScan.state === "cancelling");
  const isBatchActive =
    batchMount &&
    (batchMount.status === "running" || batchMount.status === "cancelling");

  if (!isSourceActive && !isBatchActive) return null;

  const sourceStarted =
    sourceScan?.started_at ?? sourceScan?.startedAt ?? "";
  const batchStarted = batchMount?.started_at ?? "";

  const showSource =
    isSourceActive && (!isBatchActive || sourceStarted >= batchStarted);

  async function handleCancel() {
    setCancelling(true);
    try {
      if (showSource && sourceScan) {
        await cancelSourceScan(sourceScan.id);
      } else if (batchMount) {
        await cancelBatchMount(batchMount.id);
      }
    } finally {
      setCancelling(false);
    }
  }

  const label = showSource ? "Source scan" : "Batch mount";
  const activeStage = showSource
    ? sourceScan?.stages?.find((s) => s.status === "running")?.name ??
      sourceScan?.title ??
      "Scanning"
    : batchMount?.progress.phase ?? "Mounting";

  const progressText = showSource
    ? sourceScan?.progress
      ? `${sourceScan.progress.current}/${sourceScan.progress.total ?? "?"}`
      : ""
    : `${batchMount?.progress.completed ?? 0}/${batchMount?.progress.total ?? "?"}`;

  const isCancellingTask = showSource
    ? sourceScan?.state === "cancelling"
    : batchMount?.status === "cancelling";

  return (
    <section
      aria-live="polite"
      className="ui-task-indicator pointer-events-auto fixed bottom-20 right-5 z-40 flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <LoaderCircle aria-hidden="true" className="animate-spin" size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-body-sm font-semibold">{label}</span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {activeStage} {progressText ? `· ${progressText}` : ""}
        </span>
      </span>
      <button
        aria-label="Cancel task"
        className="inline-grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control/70 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
        disabled={cancelling || isCancellingTask}
        onClick={() => void handleCancel()}
        title="Cancel task"
        type="button"
      >
        <X aria-hidden="true" size={15} />
      </button>
    </section>
  );
}
