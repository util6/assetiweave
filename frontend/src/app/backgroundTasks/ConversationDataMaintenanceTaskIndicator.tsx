import { LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useConversationDataMaintenance } from "./ConversationDataMaintenanceProvider";

export function ConversationDataMaintenanceTaskIndicator() {
  const { cancel, tasks } = useConversationDataMaintenance();
  const [cancelling, setCancelling] = useState(false);
  const task = [...tasks]
    .filter((candidate) => candidate.status === "running" || candidate.status === "cancelling")
    .sort((left, right) => right.started_at.localeCompare(left.started_at))[0];

  if (!task) return null;
  const progress = task.progress.total_stage > 0
    ? Math.round((task.progress.completed_stage / task.progress.total_stage) * 100)
    : 0;

  async function handleCancel() {
    setCancelling(true);
    try {
      await cancel(task.id);
    } finally {
      setCancelling(false);
    }
  }

  return (
    <section
      aria-live="polite"
      className="aurora-task-indicator pointer-events-auto fixed bottom-20 left-5 z-40 flex w-[min(26rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <LoaderCircle aria-hidden="true" className="animate-spin" size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-body-sm font-semibold">
          Conversation data {task.operation}
        </span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {task.progress.note ?? task.progress.phase} · {progress}%
        </span>
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
