import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationSyncTaskProgress } from "../../services/conversations";

export function ConversationFullSyncProgress({
  progress,
  t,
}: {
  progress: ConversationSyncTaskProgress;
  t: Translator;
}) {
  const totalSourceCount = progress.total_source_count;
  const completedSourceCount = Math.min(progress.completed_source_count, totalSourceCount);
  const determinate = totalSourceCount > 0;
  const percent = determinate
    ? Math.round((completedSourceCount / totalSourceCount) * 100)
    : 0;

  return (
    <div
      aria-live="polite"
      className="rounded-lg border border-theme-control-border bg-theme-control/55 px-3 py-2.5"
    >
      <div className="flex items-start justify-between gap-3 text-body-sm">
        <div className="min-w-0 text-on-surface-variant">
          {progress.current_source_name ? (
            <p className="truncate">
              {t("settings.conversation.fullSyncCurrentSource")}
              <span className="font-semibold text-on-surface">{progress.current_source_name}</span>
            </p>
          ) : determinate ? (
            <p>
              {t("settings.conversation.fullSyncProgressCount", {
                completed: completedSourceCount,
                total: totalSourceCount,
              })}
            </p>
          ) : (
            <p>{t("settings.conversation.fullSyncPreparing")}</p>
          )}
        </div>
        <span className="shrink-0 font-mono text-code-sm text-on-surface">
          {determinate ? `${percent}%` : "…"}
        </span>
      </div>
      <div
        aria-label={t("settings.conversation.fullSyncProgressLabel")}
        aria-valuemax={determinate ? totalSourceCount : undefined}
        aria-valuemin={determinate ? 0 : undefined}
        aria-valuenow={determinate ? completedSourceCount : undefined}
        aria-valuetext={determinate
          ? t("settings.conversation.fullSyncProgressPercent", { percent })
          : t("settings.conversation.fullSyncPreparing")}
        className="mt-2 h-2 overflow-hidden rounded-full bg-theme-control-border/70"
        role="progressbar"
      >
        <div
          className={`h-full rounded-full transition-[width] duration-500 ${
            progress.phase === "failed"
              ? "bg-status-remove"
              : progress.phase === "completed"
                ? "bg-status-create"
                : "motion-safe:animate-pulse bg-status-update"
          }`}
          style={{ width: determinate ? `${percent}%` : "18%" }}
        />
      </div>
    </div>
  );
}
