import { Archive, RefreshCw } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import type { SkillBackupTaskSnapshot } from "../../services/catalog";

export function SkillBackupBackgroundTaskIndicator({
  task,
  t,
}: {
  task: SkillBackupTaskSnapshot | null;
  t: Translator;
}) {
  if (task?.status !== "running") {
    return null;
  }

  const progress = taskProgress(task);
  return (
    <section
      aria-live="polite"
      className="pointer-events-auto w-[min(24rem,calc(100vw-2.5rem))] rounded-xl border border-status-update/40 bg-theme-card/95 px-4 py-3 text-on-surface shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.28)] backdrop-blur"
      role="status"
    >
      <div className="flex items-center gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-status-update/15 text-status-update">
          <RefreshCw className="animate-spin" size={17} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-body-sm font-semibold">{t("backup.background.title")}</span>
          <span className="mt-0.5 block text-code-sm text-on-surface-variant">
            {t("backup.background.description", {
              completed: task.completed_count,
              total: task.total_count,
            })}
          </span>
        </span>
        <span className="shrink-0 text-code-sm font-semibold text-status-update">{progress}%</span>
      </div>
      <div
        aria-label={t("backup.background.title")}
        aria-valuemax={task.total_count}
        aria-valuemin={0}
        aria-valuenow={task.completed_count}
        className="mt-3 h-1.5 overflow-hidden rounded-full bg-theme-control"
        role="progressbar"
      >
        <div
          className="h-full rounded-full bg-status-update transition-[width] duration-300"
          style={{ width: `${progress}%` }}
        />
      </div>
      {task.current_asset_id ? (
        <p className="mt-2 truncate text-code-sm text-on-surface-muted">
          {t("backup.background.current", { name: task.current_asset_id })}
        </p>
      ) : null}
    </section>
  );
}

export function SkillBackupInlineProgress({
  assetIds,
  task,
  t,
}: {
  assetIds: string[];
  task: SkillBackupTaskSnapshot | null;
  t: Translator;
}) {
  if (!task || !isSkillBackupRunningFor(task, assetIds)) {
    return null;
  }

  const progress = taskProgress(task);
  return (
    <div className="mt-2 min-w-48" role="status">
      <div className="flex items-center justify-between gap-3 text-code-sm text-on-surface-variant">
        <span>{t("backup.action.running")}</span>
        <span>{t("backup.action.runningCount", { completed: task.completed_count, total: task.total_count })}</span>
      </div>
      <div
        aria-label={t("backup.action.running")}
        aria-valuemax={task.total_count}
        aria-valuemin={0}
        aria-valuenow={task.completed_count}
        className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-theme-control"
        role="progressbar"
      >
        <div
          className="h-full rounded-full bg-status-update transition-[width] duration-300"
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}

export function SkillBackupButtonContent({
  assetIds,
  defaultLabel,
  task,
  t,
}: {
  assetIds: string[];
  defaultLabel: string;
  task: SkillBackupTaskSnapshot | null;
  t: Translator;
}) {
  if (!task || !isSkillBackupRunningFor(task, assetIds)) {
    return (
      <>
        <Archive size={16} />
        {defaultLabel}
      </>
    );
  }

  return (
    <>
      <RefreshCw className="animate-spin" size={16} />
      {t("backup.action.runningCount", {
        completed: task.completed_count,
        total: task.total_count,
      })}
    </>
  );
}

export function isSkillBackupRunning(task: SkillBackupTaskSnapshot | null) {
  return task?.status === "running";
}

export function isSkillBackupRunningFor(
  task: SkillBackupTaskSnapshot | null,
  assetIds: readonly string[],
) {
  if (task?.status !== "running") {
    return false;
  }

  const taskAssetIds = new Set(task.asset_ids);
  return assetIds.some((assetId) => taskAssetIds.has(assetId));
}

function taskProgress(task: SkillBackupTaskSnapshot) {
  if (task.total_count === 0) {
    return 0;
  }
  return Math.min(100, Math.round((task.completed_count / task.total_count) * 100));
}
