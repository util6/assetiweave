import { LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { isActiveAiExecutionTask, useAiExecutionTasks } from "./AiExecutionTaskProvider";

export function AiExecutionTaskIndicator() {
  const { t } = useI18n();
  const { cancelTask, tasks } = useAiExecutionTasks();
  const [cancellingTaskId, setCancellingTaskId] = useState<string | null>(null);
  const [cancelErrorTaskId, setCancelErrorTaskId] = useState<string | null>(null);
  const activeTasks = tasks
    .filter(isActiveAiExecutionTask)
    .sort((left, right) => (
      right.updated_at.localeCompare(left.updated_at) || right.id.localeCompare(left.id)
    ));
  const latestTask = activeTasks[0];

  if (!latestTask) return null;

  async function handleCancel() {
    setCancelErrorTaskId(null);
    setCancellingTaskId(latestTask.id);
    try {
      await cancelTask(latestTask.id);
    } catch {
      setCancelErrorTaskId(latestTask.id);
    } finally {
      setCancellingTaskId(null);
    }
  }

  const cancelling = latestTask.phase === "cancelling" ||
    latestTask.phase === "cleaning_up" ||
    cancellingTaskId === latestTask.id;

  return (
    <section
      aria-live="polite"
      className="aurora-task-indicator pointer-events-auto fixed bottom-5 right-5 z-40 flex w-[min(24rem,calc(100vw-2.5rem))] items-center gap-3 rounded-2xl border px-4 py-3 text-on-surface"
      role="status"
    >
      <span className="aurora-task-indicator-icon grid size-9 shrink-0 place-items-center rounded-xl text-status-update">
        <LoaderCircle aria-hidden="true" className="animate-spin" size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-body-sm font-semibold">
          {t("ai.execution.global.title", { count: activeTasks.length })}
        </span>
        <span className="mt-0.5 block text-code-sm text-on-surface-variant">
          {t(`ai.execution.phase.${latestTask.phase}` as TranslationKey)}
        </span>
        {cancelErrorTaskId === latestTask.id ? (
          <span className="mt-1 block text-code-sm text-status-remove" role="alert">
            {t("ai.execution.cancelFailed")}
          </span>
        ) : null}
      </span>
      <button
        aria-label={t("ai.execution.cancel")}
        className="inline-grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control/70 text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
        disabled={cancelling}
        onClick={() => void handleCancel()}
        title={t("ai.execution.cancel")}
        type="button"
      >
        <X aria-hidden="true" size={15} />
      </button>
    </section>
  );
}
