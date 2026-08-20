import { LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useCatalogTasks } from "./CatalogTaskProvider";

export function CatalogTaskIndicator() {
  const { sourceScan, batchMount, cancelSourceScan, cancelBatchMount } = useCatalogTasks();
  const [cancelling, setCancelling] = useState(false);
  const active = [sourceScan, batchMount]
    .filter((task) => task?.status === "running" || task?.status === "cancelling")
    .sort((left, right) => (right?.started_at ?? "").localeCompare(left?.started_at ?? ""))[0];

  if (!active) return null;
  const sourceTask = sourceScan && sourceScan.id === active.id ? sourceScan : null;
  const batchTask = !sourceTask && batchMount && batchMount.id === active.id ? batchMount : null;
  const task = sourceTask ?? batchTask;
  if (!task) return null;
  const taskId = task.id;

  async function handleCancel() {
    setCancelling(true);
    try {
      if (sourceTask) {
        await cancelSourceScan(taskId);
      } else {
        await cancelBatchMount(taskId);
      }
    } finally {
      setCancelling(false);
    }
  }

  const label = sourceTask ? "Source scan" : "Batch mount";
  const progress = sourceTask
    ? `${sourceTask.progress.completed_source_count}/${sourceTask.progress.total_source_count ?? "?"}`
    : `${batchTask?.progress.completed ?? 0}/${batchTask?.progress.total ?? "?"}`;

  return (
    <section
      aria-live="polite"
      className="aurora-task-indicator pointer-events-auto fixed bottom-20 right-5 z-40 flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <LoaderCircle aria-hidden="true" className="animate-spin" size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-body-sm font-semibold">{label}</span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">{task.progress.phase} · {progress}</span>
      </span>
      <button
        aria-label="Cancel task"
        className="inline-grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control/70 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
        disabled={cancelling || task.status === "cancelling"}
        onClick={() => void handleCancel()}
        title="Cancel task"
        type="button"
      >
        <X aria-hidden="true" size={15} />
      </button>
    </section>
  );
}
