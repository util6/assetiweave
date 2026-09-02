import { CheckCircle2, CircleAlert, Clock3, LoaderCircle } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TeamMember, TeamTask } from "../../types/team";

export interface TeamTaskCardProps {
  task: TeamTask;
  owner: TeamMember | undefined;
}

export function teamTaskAnchor(taskId: string): string {
  return `team-task-${taskId}`;
}

export function TeamTaskCard({ owner, task }: TeamTaskCardProps) {
  const { t } = useI18n();
  const status = taskStatus(task.state, t);

  return (
    <li
      aria-label={t("team.workflow.taskProjectionLabel", { title: task.title })}
      className="rounded-xl border border-theme-card-border/65 bg-theme-card/55 px-3.5 py-3"
      data-testid={`team-task-card-${task.id}`}
      id={teamTaskAnchor(task.id)}
      tabIndex={-1}
    >
      <div className="flex items-start gap-3">
        <span className={`grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border/70 bg-theme-control/70 ${status.className}`}>
          {status.icon}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-label-caps uppercase text-on-surface-variant">{t("team.workflow.taskProjection")}</span>
            <span className="truncate text-body-sm font-semibold text-on-surface">{task.title}</span>
            <span className={`ml-auto text-label-caps uppercase ${status.className}`}>{status.label}</span>
          </div>
          <p className="mt-1 whitespace-pre-wrap break-words text-body-sm text-on-surface-variant">{task.description}</p>
          <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-on-surface-variant">
            <span>{t("team.workflow.ownerValue", { owner: owner?.id ?? task.owner_member_id ?? t("team.workflow.unassigned") })}</span>
            {task.result ? <span className="text-status-create">{t("team.workflow.taskResult", { result: task.result })}</span> : null}
            {task.error_code ? <span className="text-status-remove">{t("team.workflow.taskError", { error: task.error_code })}</span> : null}
          </div>
        </div>
      </div>
    </li>
  );
}

function taskStatus(
  state: TeamTask["state"],
  t: ReturnType<typeof useI18n>["t"],
) {
  switch (state) {
    case "queued":
      return { className: "text-status-update", icon: <Clock3 size={15} />, label: t("team.workflow.taskState.queued") };
    case "running":
      return { className: "text-status-update", icon: <LoaderCircle className="animate-spin" size={15} />, label: t("team.workflow.taskState.running") };
    case "succeeded":
      return { className: "text-status-create", icon: <CheckCircle2 size={15} />, label: t("team.workflow.taskState.succeeded") };
    case "failed":
      return { className: "text-status-remove", icon: <CircleAlert size={15} />, label: t("team.workflow.taskState.failed") };
    case "canceled":
      return { className: "text-status-conflict", icon: <CircleAlert size={15} />, label: t("team.workflow.taskState.canceled") };
    case "draft":
      return { className: "text-on-surface-variant", icon: <Clock3 size={15} />, label: t("team.workflow.taskState.draft") };
  }
}
