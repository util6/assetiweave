import {
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  LoaderCircle,
  Play,
  XCircle,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  TeamDetail,
  TeamRunSnapshot,
} from "../../types/team";
import { Button } from "../ui/button";

export interface TeamPlanCardProps {
  team: TeamDetail;
  snapshot: TeamRunSnapshot;
  busy: boolean;
  error: string | null;
  onTaskChange: (taskId: string, patch: { title?: string; description?: string; owner_member_id?: string }) => void;
  onMoveTask: (taskId: string, direction: -1 | 1) => void;
  onTaskNavigate: (taskId: string, ownerMemberId: string | null) => void;
  onReview: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}
export function TeamPlanCard({
  busy,
  error,
  onCancel,
  onConfirm,
  onMoveTask,
  onTaskNavigate,
  onReview,
  onTaskChange,
  snapshot,
  team,
}: TeamPlanCardProps) {
  const { t } = useI18n();
  const teammates = snapshot.run.roster_snapshot.filter((member) => member.role === "teammate");
  const tasks = [...snapshot.tasks].sort((left, right) => left.sort_order - right.sort_order);
  const awaitingReview = snapshot.run.state === "awaiting_review";
  const terminal = snapshot.run.state === "terminal";
  const completedCount = tasks.filter((task) => task.state === "succeeded").length;
  const canNavigate = snapshot.run.state === "executing" || terminal;

  return (
    <li className="rounded-xl border border-theme-nav-active-border/45 bg-theme-nav-active/10 px-3.5 py-3" data-testid="team-plan-card">
      <div className="flex items-start gap-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-theme-nav-active-border/55 bg-theme-nav-active/20 text-primary">
          {snapshot.run.state === "drafting" || snapshot.run.state === "executing"
            ? <LoaderCircle className="animate-spin" size={15} />
            : <CheckCircle2 size={15} />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-label-caps uppercase text-on-surface-variant">{t("team.workflow.plan")}</span>
            <span className="text-caption text-on-surface-variant">{t("team.workflow.state", { state: snapshot.run.state })}</span>
            <span className="ml-auto text-caption text-on-surface-variant">{t("team.workflow.revision", { revision: snapshot.run.revision })}</span>
          </div>
          <p className="mt-1 text-body-sm text-on-surface">{t("team.workflow.planDescription")}</p>
          {tasks.length > 0 && !awaitingReview ? (
            <p className="mt-2 text-caption text-on-surface-variant" data-testid="team-plan-progress">
              {t("team.workflow.progress", { completed: completedCount, total: tasks.length })}
            </p>
          ) : null}

          {tasks.length === 0 ? (
            <p className="mt-3 rounded-lg border border-theme-card-border/60 bg-theme-control/25 p-3 text-caption text-on-surface-variant">
              {snapshot.run.state === "drafting" ? t("team.workflow.drafting") : t("team.workflow.noTasks")}
            </p>
          ) : (
            <div className="mt-3 grid gap-2">
              {tasks.map((task, index) => {
                const recommended = teammates.find((member) => member.member_id === task.recommended_member_id);
                return (
                  <div className="grid gap-2 rounded-lg border border-theme-card-border/60 bg-theme-control/35 p-3" data-testid={`team-plan-task-${task.id}`} key={task.id}>
                    <div className="flex items-start justify-between gap-3">
                      {task.owner_member_id && canNavigate ? (
                        <button
                          aria-label={t("team.workflow.jumpTask", { number: index + 1 })}
                          className="rounded-md text-left text-label-caps text-on-surface-variant underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55"
                          data-testid={`team-plan-task-jump-${task.id}`}
                          onClick={() => onTaskNavigate(task.id, task.owner_member_id)}
                          type="button"
                        >
                          {t("team.workflow.taskNumber", { number: index + 1 })}
                        </button>
                      ) : (
                        <span className="text-label-caps text-on-surface-variant">{t("team.workflow.taskNumber", { number: index + 1 })}</span>
                      )}
                      <span className="shrink-0 text-label-caps text-primary">{task.state}</span>
                    </div>
                    <label className="grid gap-1 text-caption text-on-surface-variant">
                      {t("team.workflow.taskTitle")}
                      <input
                        aria-label={`${t("team.workflow.taskTitle")} ${index + 1}`}
                        className="h-9 rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 text-body-sm text-on-surface outline-none focus:border-primary-strong/65 focus:ring-2 focus:ring-primary-strong/25 disabled:cursor-not-allowed disabled:opacity-60"
                        disabled={busy || !awaitingReview}
                        onChange={(event) => onTaskChange(task.id, { title: event.target.value })}
                        value={task.title}
                      />
                    </label>
                    <label className="grid gap-1 text-caption text-on-surface-variant">
                      {t("team.workflow.taskDescription")}
                      <textarea
                        aria-label={`${t("team.workflow.taskDescription")} ${index + 1}`}
                        className="min-h-16 resize-y rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 py-2 text-body-sm text-on-surface outline-none focus:border-primary-strong/65 focus:ring-2 focus:ring-primary-strong/25 disabled:cursor-not-allowed disabled:opacity-60"
                        disabled={busy || !awaitingReview}
                        onChange={(event) => onTaskChange(task.id, { description: event.target.value })}
                        value={task.description}
                      />
                    </label>
                    <div className="flex flex-wrap items-end gap-2">
                      <label className="grid min-w-56 flex-1 gap-1 text-caption text-on-surface-variant">
                        {t("team.workflow.owner")}
                        <select
                          aria-label={`${t("team.workflow.owner")} ${index + 1}`}
                          className="h-9 rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 text-body-sm text-on-surface outline-none focus:border-primary-strong/65 focus:ring-2 focus:ring-primary-strong/25 disabled:cursor-not-allowed disabled:opacity-60"
                          disabled={busy || !awaitingReview}
                          onChange={(event) => onTaskChange(task.id, { owner_member_id: event.target.value })}
                          value={task.owner_member_id ?? task.recommended_member_id}
                        >
                          {teammates.map((member) => (
                            <option key={member.member_id} value={member.member_id}>{member.member_id} · {member.agent_id}</option>
                          ))}
                        </select>
                      </label>
                      <span className="max-w-full text-caption text-on-surface-variant">
                        {t("team.workflow.recommended", { owner: recommended?.member_id ?? task.recommended_member_id })}
                      </span>
                      <Button aria-label={t("team.action.moveUp")} disabled={busy || !awaitingReview || index === 0} onClick={() => onMoveTask(task.id, -1)} size="icon-sm" type="button" variant="ghost"><ArrowUp size={14} /></Button>
                      <Button aria-label={t("team.action.moveDown")} disabled={busy || !awaitingReview || index === tasks.length - 1} onClick={() => onMoveTask(task.id, 1)} size="icon-sm" type="button" variant="ghost"><ArrowDown size={14} /></Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <Button disabled={busy || !awaitingReview || tasks.length === 0} onClick={onReview} size="sm" type="button" variant="outline">{t("team.workflow.review")}</Button>
            <Button disabled={busy || !awaitingReview || tasks.length === 0} onClick={onConfirm} size="sm" type="button"><Play size={14} />{t("team.workflow.confirm")}</Button>
            <Button disabled={busy || terminal} onClick={onCancel} size="sm" type="button" variant="outline"><XCircle size={14} />{t("team.workflow.cancel")}</Button>
          </div>

          {error ? <p className="mt-3 rounded-lg border border-status-remove/35 bg-status-remove/10 p-2 text-caption text-status-remove">{error}</p> : null}
        </div>
      </div>
    </li>
  );
}
