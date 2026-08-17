import { LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import { isActiveAgentLifecycleTask, useAgentLifecycleTasks } from "./AgentLifecycleTaskProvider";

export function AgentLifecycleTaskIndicator() {
  const { t } = useI18n();
  const { cancelTask, tasks } = useAgentLifecycleTasks();
  const [cancellingTaskId, setCancellingTaskId] = useState<string | null>(null);
  const activeTasks = tasks
    .filter(isActiveAgentLifecycleTask)
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt) || right.id.localeCompare(left.id));
  const latestTask = activeTasks[0];

  if (!latestTask) return null;

  async function handleCancel() {
    setCancellingTaskId(latestTask.id);
    try {
      await cancelTask(latestTask.id);
    } finally {
      setCancellingTaskId(null);
    }
  }

  return (
    <section
      aria-live="polite"
      className="aurora-task-indicator pointer-events-auto fixed bottom-20 right-5 z-40 flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="aurora-task-indicator-icon grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <LoaderCircle aria-hidden="true" className="animate-spin" size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-body-sm font-semibold">
          {t("agent.lifecycle.global.title", { count: activeTasks.length })}
        </span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {latestTask.agentId} · {latestTask.phase}
        </span>
      </span>
      <button
        aria-label={t("agent.lifecycle.cancel")}
        className="inline-grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control/70 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
        disabled={cancellingTaskId === latestTask.id}
        onClick={() => void handleCancel()}
        title={t("agent.lifecycle.cancel")}
        type="button"
      >
        <X aria-hidden="true" size={15} />
      </button>
    </section>
  );
}
