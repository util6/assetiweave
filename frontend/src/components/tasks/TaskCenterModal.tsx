import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertCircle,
  AlertTriangle,
  Archive,
  ArrowLeft,
  Ban,
  Brain,
  Check,
  CheckCircle2,
  ChevronRight,
  Clock,
  Copy,
  Layers,
  ListTodo,
  Loader2,
  MessageSquare,
  RefreshCw,
  RotateCcw,
  Search,
  Sparkles,
  Trash2,
  XCircle,
} from "lucide-react";
import clsx from "clsx";
import { DialogFrame } from "../foundation/DialogFrame";
import { EmptyState } from "../foundation/EmptyState";
import { ErrorBoundary } from "../foundation/ErrorBoundary";
import { Button } from "../ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import { useTaskCenter } from "../../app/backgroundTasks/TaskCenterProvider";
import { AgentSessionWorkspace } from "../agent-session";
import { mapSessionItemSnapshotToView } from "../team/teamSessionAdapter";
import {
  getAgentSession,
  subscribeAgentSessionUpdated,
} from "../../services/agentSessionService";
import {
  isAgentSessionAvailable,
  type AgentSessionGetResult,
  type AgentSessionRef,
} from "../../types/agentSession";
import type {
  TaskActivityView,
  TaskStageView,
  TaskState,
  TaskView,
} from "../../types/taskCenter";

export interface TaskCenterModalProps {
  open: boolean;
  onClose: () => void;
}

type FilterStatus = "all" | "running" | "completed" | "failed";

export function TaskCenterModal({ open, onClose }: TaskCenterModalProps) {
  if (!open) return null;

  return (
    <ErrorBoundary
      fallback={(error, reset) => (
        <DialogFrame
          className="flex h-[400px] w-[90vw] max-w-[500px] flex-col overflow-hidden"
          closeLabel="关闭"
          icon={<AlertCircle className="size-5 text-status-conflict" />}
          onClose={onClose}
          size="md"
          title="任务中心发生异常"
        >
          <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4 text-center">
            <p className="text-body-sm text-status-conflict font-medium">
              渲染任务中心时遇到错误
            </p>
            <p className="max-w-xs font-mono text-caption text-on-surface-variant">
              {error.message}
            </p>
            <div className="mt-2 flex gap-2">
              <Button onClick={reset} size="sm" type="button" variant="outline">
                <RotateCcw className="mr-1.5 size-3.5" />
                重试
              </Button>
              <Button
                onClick={onClose}
                size="sm"
                type="button"
                variant="default"
              >
                关闭
              </Button>
            </div>
          </div>
        </DialogFrame>
      )}
    >
      <TaskCenterModalContent onClose={onClose} open={open} />
    </ErrorBoundary>
  );
}

function TaskCenterModalContent({
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const taskCenter = useTaskCenter();
  const tasks = taskCenter.tasks ?? [];
  const selectedTaskId = taskCenter.selectedTaskId;
  const setSelectedTaskId = taskCenter.setSelectedTaskId;
  const cancelTask = taskCenter.cancelTask;
  const retryTask = taskCenter.retryTask;
  const clearTerminal = taskCenter.clearTerminal;
  const activeCount = taskCenter.activeCount ?? 0;
  const failureCount = taskCenter.failureCount ?? 0;

  const [filterStatus, setFilterStatus] = useState<FilterStatus>("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [actionBusy, setActionBusy] = useState(false);
  const [viewingSessionRef, setViewingSessionRef] =
    useState<AgentSessionRef | null>(null);

  useEffect(() => {
    setViewingSessionRef(null);
  }, [selectedTaskId]);

  // 过滤任务列表
  const filteredTasks = useMemo(() => {
    return tasks.filter((task) => {
      const failures = task.failures ?? [];
      // 状态过滤
      if (filterStatus === "running") {
        if (
          task.state !== "running" &&
          task.state !== "pending" &&
          task.state !== "cancelling"
        )
          return false;
      } else if (filterStatus === "completed") {
        if (
          task.state !== "succeeded" &&
          task.outcome !== "success" &&
          task.outcome !== "partial_success"
        )
          return false;
      } else if (filterStatus === "failed") {
        if (
          task.state !== "failed" &&
          task.outcome !== "failure" &&
          task.outcome !== "partial_success" &&
          failures.length === 0
        )
          return false;
      }

      // 搜索关键词过滤
      if (searchQuery.trim()) {
        const query = searchQuery.trim().toLowerCase();
        const matchesTitle = (task.title ?? "").toLowerCase().includes(query);
        const matchesId = (task.id ?? "").toLowerCase().includes(query);
        const matchesKind = (task.kind ?? "").toLowerCase().includes(query);
        return matchesTitle || matchesId || matchesKind;
      }

      return true;
    });
  }, [tasks, filterStatus, searchQuery]);

  const selectedTask = useMemo(() => {
    if (selectedTaskId) {
      const found = filteredTasks.find((t) => t.id === selectedTaskId);
      if (found) return found;
    }
    return filteredTasks[0] ?? null;
  }, [selectedTaskId, filteredTasks]);

  const handleCancel = async (task: TaskView) => {
    if (actionBusy || !task.capabilities?.cancellable) return;
    setActionBusy(true);
    try {
      await cancelTask(task.id);
    } finally {
      setActionBusy(false);
    }
  };

  const handleRetry = async (task: TaskView) => {
    if (actionBusy || !task.capabilities?.retryable) return;
    setActionBusy(true);
    try {
      await retryTask(task.id);
    } finally {
      setActionBusy(false);
    }
  };

  const handleClearTerminal = async () => {
    if (actionBusy) return;
    setActionBusy(true);
    try {
      await clearTerminal();
    } finally {
      setActionBusy(false);
    }
  };

  return (
    <DialogFrame
      className="flex h-[86vh] max-h-[920px] w-[95vw] max-w-[1240px] flex-col overflow-hidden"
      closeLabel={t("common.close")}
      contentClassName="flex min-h-0 flex-1 overflow-hidden p-0"
      description={t("tasks.page.description")}
      footer={
        <div className="flex w-full items-center justify-between">
          <div className="flex items-center gap-3 text-caption text-on-surface-variant">
            <span>
              {t("tasks.filter.running")}:{" "}
              <strong className="text-on-surface">{activeCount}</strong>
            </span>
            {failureCount > 0 ? (
              <span className="text-status-conflict">
                {t("tasks.filter.failed")}: <strong>{failureCount}</strong>
              </span>
            ) : null}
            <span>
              {t("tasks.filter.all")}:{" "}
              <strong className="text-on-surface">{tasks.length}</strong>
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              disabled={
                actionBusy ||
                tasks.length === 0 ||
                tasks.every((t) => t.state === "running")
              }
              onClick={() => void handleClearTerminal()}
              size="sm"
              type="button"
              variant="outline"
            >
              <Trash2 className="mr-1.5 size-3.5" />
              {t("tasks.action.clearCompleted")}
            </Button>
            <Button onClick={onClose} size="sm" type="button" variant="default">
              {t("common.close")}
            </Button>
          </div>
        </div>
      }
      icon={<ListTodo className="size-5 text-primary" />}
      onClose={onClose}
      size="2xl"
      title={t("tasks.page.title")}
    >
      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* 左栏：任务列表面板 */}
        <aside className="flex w-[360px] shrink-0 flex-col border-r border-theme-control-border bg-surface-elevated/40">
          {/* 搜索与过滤工具区 */}
          <div className="flex flex-col gap-2.5 border-b border-theme-control-border p-3">
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-on-surface-variant" />
              <input
                className="h-8 w-full rounded-md border border-theme-control-border bg-theme-control/60 pl-8 pr-3 text-code-sm text-on-surface placeholder:text-outline focus:border-primary/60 focus:outline-none"
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("tasks.search.placeholder")}
                value={searchQuery}
              />
            </div>

            {/* 状态过滤切换 */}
            <div className="flex rounded-lg border border-theme-control-border bg-theme-control/40 p-0.5">
              {(
                [
                  ["all", t("tasks.filter.all")],
                  ["running", t("tasks.filter.running")],
                  ["completed", t("tasks.filter.completed")],
                  ["failed", t("tasks.filter.failed")],
                ] as const
              ).map(([status, label]) => (
                <button
                  className={clsx(
                    "flex-1 rounded-md py-1.5 text-center text-caption font-medium transition-all duration-200 cursor-pointer",
                    filterStatus === status
                      ? "bg-surface-elevated text-on-surface shadow-sm font-semibold border border-theme-control-border/50"
                      : "text-on-surface-variant hover:text-on-surface hover:bg-theme-control/40",
                  )}
                  key={status}
                  onClick={() => setFilterStatus(status)}
                  type="button"
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          {/* 任务列表条目 */}
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
            {filteredTasks.length === 0 ? (
              <div className="flex flex-1 flex-col items-center justify-center p-6 text-center text-on-surface-variant">
                <ListTodo className="mb-2 size-8 stroke-1 text-outline" />
                <p className="text-body-sm font-medium">
                  {t("tasks.empty.title")}
                </p>
                <p className="mt-1 text-caption text-outline">
                  {searchQuery || filterStatus !== "all"
                    ? t("tasks.empty.noMatch")
                    : t("tasks.empty.description")}
                </p>
              </div>
            ) : (
              <div
                className="aurora-view-transition flex flex-col gap-2"
                data-testid="task-list"
                key={filterStatus}
              >
                {filteredTasks.map((task) => {
                  const isSelected = selectedTask?.id === task.id;
                  const percent = task.progress?.total
                    ? Math.round(
                        (task.progress.current / task.progress.total) * 100,
                      )
                    : null;
                  const startedAt = task.startedAt ?? task.started_at;
                  return (
                    <button
                      aria-pressed={isSelected}
                      className={clsx(
                        "conversation-row group grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 p-3 text-left transition-all cursor-pointer",
                        isSelected ? "text-on-surface" : "",
                      )}
                      data-selected={isSelected}
                      key={task.id}
                      onClick={() => setSelectedTaskId(task.id)}
                      type="button"
                    >
                      {/* 左侧：统一的方圆图标容器 */}
                      <span
                        className={clsx(
                          "grid size-9 shrink-0 place-items-center rounded-xl border transition-colors",
                          isSelected
                            ? "border-primary/50 bg-primary/20 text-primary shadow-sm"
                            : "border-theme-control-border bg-theme-control text-primary/80 group-hover:border-primary/30 group-hover:text-primary",
                        )}
                      >
                        <TaskKindIcon className="size-4.5" kind={task.kind} />
                      </span>

                      {/* 中间：标题与元信息 */}
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate text-body-sm font-semibold text-on-surface">
                            {task.title}
                          </span>
                        </div>

                        <div className="mt-1 flex items-center gap-2 text-code-sm text-on-surface-variant">
                          <span className="capitalize">{task.kind}</span>
                          <span className="text-on-surface-muted">•</span>
                          <span>{formatTimeShort(startedAt)}</span>
                        </div>

                        {task.progress && task.state === "running" ? (
                          <div className="mt-1.5 flex flex-col gap-1">
                            <div className="flex justify-between text-[11px] text-on-surface-variant">
                              <span className="line-clamp-1">
                                {task.progress.note || ""}
                              </span>
                              {percent !== null ? (
                                <span>{percent}%</span>
                              ) : null}
                            </div>
                            {percent !== null ? (
                              <div className="h-1.5 w-full overflow-hidden rounded-full bg-theme-control">
                                <div
                                  className="h-full rounded-full bg-primary transition-all duration-300"
                                  style={{
                                    width: `${Math.min(100, Math.max(0, percent))}%`,
                                  }}
                                />
                              </div>
                            ) : null}
                          </div>
                        ) : null}
                      </div>

                      {/* 右侧：状态徽章与箭头指示符 */}
                      <div className="flex shrink-0 items-center gap-2">
                        <TaskStateBadge
                          outcome={task.outcome}
                          state={task.state}
                        />
                        <ChevronRight
                          className={clsx(
                            "size-4 transition-transform duration-200 group-hover:translate-x-0.5",
                            isSelected
                              ? "text-primary"
                              : "text-on-surface-muted group-hover:text-on-surface",
                          )}
                        />
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </aside>

        {/* 右栏：选中任务详情面板（带平滑视图切换动效与流动微光进度条） */}
        <main className="relative flex min-h-0 flex-1 flex-col overflow-y-auto bg-surface-base p-6">
          {activeCount > 0 || actionBusy ? (
            <div className="aurora-route-progress" />
          ) : null}

          {selectedTask ? (
            viewingSessionRef ? (
              <TaskAgentSessionObserver
                onBack={() => setViewingSessionRef(null)}
                sessionRef={viewingSessionRef}
                taskTitle={selectedTask.title}
              />
            ) : (
              <div
                className="aurora-view-transition flex flex-col gap-6"
                key={selectedTask.id}
              >
                {/* 头部摘要与操作 */}
                {(() => {
                  const tenantId =
                    selectedTask.tenantId ?? selectedTask.tenant_id;
                  const startedAt =
                    selectedTask.startedAt ?? selectedTask.started_at;
                  const finishedAt =
                    selectedTask.finishedAt ?? selectedTask.finished_at;
                  const errorSummary =
                    selectedTask.errorSummary ?? selectedTask.error_summary;
                  const resultSummary =
                    selectedTask.resultSummary ?? selectedTask.result_summary;
                  const metrics = selectedTask.metrics ?? [];
                  const stages = selectedTask.stages ?? [];
                  const failures = selectedTask.failures ?? [];
                  const capabilities = selectedTask.capabilities ?? {
                    cancellable: false,
                    retryable: false,
                  };

                  return (
                    <>
                      <div className="flex flex-col gap-3 rounded-2xl border border-theme-control-border bg-surface-elevated/60 p-5 backdrop-blur">
                        <div className="flex items-start justify-between gap-4">
                          <div className="flex flex-col gap-1">
                            <div className="flex items-center gap-2">
                              <TaskKindIcon
                                className="size-5 text-primary"
                                kind={selectedTask.kind}
                              />
                              <h2 className="text-title-lg font-semibold text-on-surface">
                                {selectedTask.title}
                              </h2>
                            </div>
                            <div className="flex flex-wrap items-center gap-3 text-caption text-on-surface-variant">
                              <span>
                                ID:{" "}
                                <code className="font-mono">
                                  {selectedTask.id}
                                </code>
                              </span>
                              {tenantId ? (
                                <span>
                                  租户:{" "}
                                  <strong className="text-on-surface">
                                    {tenantId}
                                  </strong>
                                </span>
                              ) : null}
                              {startedAt ? (
                                <span>
                                  开始于: {new Date(startedAt).toLocaleString()}
                                </span>
                              ) : null}
                              {finishedAt ? (
                                <span>
                                  结束于:{" "}
                                  {new Date(finishedAt).toLocaleString()}
                                </span>
                              ) : null}
                            </div>
                          </div>

                          {/* 任务控制操作 */}
                          <div className="flex items-center gap-2">
                            {capabilities.cancellable &&
                            selectedTask.state === "running" ? (
                              <Button
                                disabled={actionBusy}
                                onClick={() => void handleCancel(selectedTask)}
                                size="sm"
                                type="button"
                                variant="destructive"
                              >
                                <Ban className="mr-1.5 size-3.5" />
                                {t("tasks.action.cancel")}
                              </Button>
                            ) : null}

                            {capabilities.retryable &&
                            (selectedTask.state === "failed" ||
                              selectedTask.outcome === "failure") ? (
                              <Button
                                disabled={actionBusy}
                                onClick={() => void handleRetry(selectedTask)}
                                size="sm"
                                type="button"
                                variant="outline"
                              >
                                <RotateCcw className="mr-1.5 size-3.5" />
                                {t("tasks.action.retry")}
                              </Button>
                            ) : null}
                          </div>
                        </div>

                        {/* 终态结果/错误摘要 */}
                        {errorSummary ? (
                          <div className="rounded-xl border border-status-conflict/30 bg-status-conflict/10 p-3 text-body-sm text-status-conflict">
                            <div className="flex items-center gap-2 font-medium">
                              <AlertCircle className="size-4" />
                              <span>异常信息</span>
                            </div>
                            <p className="mt-1 text-caption text-on-surface-variant">
                              {errorSummary}
                            </p>
                          </div>
                        ) : null}

                        {resultSummary ? (
                          <div className="rounded-xl border border-status-create/30 bg-status-create/10 p-3 text-body-sm text-status-create">
                            <div className="flex items-center gap-2 font-medium">
                              <Check className="size-4" />
                              <span>执行结果</span>
                            </div>
                            <p className="mt-1 text-caption text-on-surface-variant">
                              {resultSummary}
                            </p>
                          </div>
                        ) : null}

                        {/* 总体指标展示 */}
                        {metrics.length > 0 ? (
                          <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4 md:grid-cols-6">
                            {metrics.map((metric) => (
                              <div
                                className="flex flex-col rounded-xl border border-theme-control-border/60 bg-theme-control/20 p-2.5"
                                key={metric.code}
                              >
                                <span className="text-caption text-on-surface-variant">
                                  {metric.code}
                                </span>
                                <span className="text-title-sm font-semibold text-on-surface">
                                  {metric.value}
                                </span>
                              </div>
                            ))}
                          </div>
                        ) : null}
                      </div>

                      {/* 阶段流水线 (Stages) */}
                      <div className="flex flex-col gap-3">
                        <div className="flex items-center gap-2 text-title-sm font-semibold text-on-surface">
                          <Layers className="size-4 text-primary" />
                          <span>{t("tasks.section.stages")}</span>
                          <span className="text-caption text-on-surface-variant">
                            ({stages.length})
                          </span>
                        </div>

                        {stages.length === 0 ? (
                          <div className="rounded-2xl border border-dashed border-theme-control-border p-6 text-center text-caption text-on-surface-variant">
                            该任务尚未上报阶段划分
                          </div>
                        ) : (
                          <div className="flex flex-col gap-3">
                            {stages.map((stage, idx) => (
                              <StageCard
                                index={idx + 1}
                                key={stage.id || `stage-${idx}`}
                                onViewSession={(ref) =>
                                  setViewingSessionRef(ref)
                                }
                                stage={stage}
                              />
                            ))}
                          </div>
                        )}
                      </div>

                      {/* 失败与审计留痕 */}
                      {failures.length > 0 ? (
                        <div className="flex flex-col gap-3">
                          <div className="flex items-center gap-2 text-title-sm font-semibold text-status-conflict">
                            <AlertTriangle className="size-4" />
                            <span>{t("tasks.section.failures")}</span>
                            <span className="text-caption">
                              ({failures.length})
                            </span>
                          </div>

                          <div className="flex flex-col gap-2">
                            {failures.map((failure, idx) => (
                              <div
                                className="flex flex-col gap-1.5 rounded-xl border border-status-conflict/30 bg-surface-elevated p-3"
                                key={idx}
                              >
                                <div className="flex items-center justify-between">
                                  <span className="font-mono text-caption font-semibold text-status-conflict">
                                    [{failure.code}]
                                  </span>
                                  <span className="text-caption text-on-surface-variant">
                                    阶段: {failure.stage}
                                  </span>
                                </div>
                                <p className="text-body-sm text-on-surface">
                                  {failure.message}
                                </p>
                                {failure.path ? (
                                  <span className="font-mono text-caption text-on-surface-variant">
                                    路径: {failure.path}
                                  </span>
                                ) : null}
                              </div>
                            ))}
                          </div>
                        </div>
                      ) : null}
                    </>
                  );
                })()}
              </div>
            )
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center text-center text-on-surface-variant">
              <EmptyState
                description={t("tasks.empty.select")}
                icon={<ListTodo className="size-10 stroke-1 text-outline" />}
                title={t("tasks.empty.title")}
              />
            </div>
          )}
        </main>
      </div>
    </DialogFrame>
  );
}

function StageCard({
  stage,
  index,
  onViewSession,
}: {
  stage: TaskStageView;
  index: number;
  onViewSession?: (sessionRef: AgentSessionRef) => void;
}) {
  const [copied, setCopied] = useState<string | null>(null);

  const copyPath = (path: string) => {
    void navigator.clipboard.writeText(path);
    setCopied(path);
    setTimeout(() => setCopied(null), 1500);
  };

  const percent = stage.progress?.total
    ? Math.round((stage.progress.current / stage.progress.total) * 100)
    : null;

  const durationMs = stage.durationMs ?? stage.duration_ms;
  const currentActivities =
    stage.currentActivities ?? stage.current_activities ?? [];
  const metrics = stage.metrics ?? [];
  const sessionRef = stage.agentSessionRef ?? stage.agent_session_ref;

  return (
    <div className="flex flex-col gap-2.5 rounded-2xl border border-theme-control-border bg-surface-elevated/50 p-4 transition-all hover:border-theme-control-border">
      {/* 阶段标题栏 */}
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <span className="flex size-6 items-center justify-center rounded-full bg-theme-control text-caption font-semibold text-on-surface">
            {index}
          </span>
          <span className="text-body-sm font-semibold text-on-surface">
            {stage.name}
          </span>
          <StageStatusBadge status={stage.status} />
        </div>

        <div className="flex items-center gap-3 text-caption text-on-surface-variant">
          {durationMs ? <span>耗时: {formatDuration(durationMs)}</span> : null}
          {percent !== null ? <span>{percent}%</span> : null}
          {sessionRef ? (
            <Button
              className="h-7 gap-1 px-2.5 text-caption font-medium"
              data-testid={`task-stage-view-session-${stage.id}`}
              onClick={() => onViewSession?.(sessionRef)}
              size="sm"
              variant="secondary"
            >
              <Sparkles className="size-3 text-primary" />
              <span>查看执行现场</span>
            </Button>
          ) : null}
        </div>
      </div>

      {stage.progress?.note ? (
        <p className="text-body-sm text-on-surface-variant">
          {stage.progress.note}
        </p>
      ) : null}

      {/* 活跃 Worker 活动 - 防御式读取 currentActivities */}
      {currentActivities.length > 0 ? (
        <div className="mt-1 flex flex-col gap-1.5 rounded-xl border border-theme-control-border/50 bg-theme-control/20 p-2.5">
          <div className="flex items-center gap-1.5 text-caption font-medium text-primary">
            <Activity className="size-3.5 animate-pulse" />
            <span>活跃 Workers ({currentActivities.length})</span>
          </div>

          <div className="flex flex-col gap-1">
            {currentActivities.map((act: TaskActivityView, actIdx: number) => {
              const workerId =
                act.workerId ?? act.worker_id ?? `worker-${actIdx}`;
              return (
                <div
                  className="flex items-center justify-between gap-2 rounded-lg bg-surface-base/60 px-2.5 py-1 text-caption"
                  key={workerId}
                >
                  <div className="flex items-center gap-2 overflow-hidden">
                    <span className="font-mono font-medium text-on-surface">
                      [{workerId}]
                    </span>
                    <span className="text-on-surface-variant">
                      {act.operation}
                    </span>
                    {act.path ? (
                      <button
                        className="group flex items-center gap-1 font-mono text-outline hover:text-on-surface"
                        onClick={() => copyPath(act.path!)}
                        title="点击复制路径"
                        type="button"
                      >
                        <span className="max-w-[280px] truncate">
                          {act.path}
                        </span>
                        {copied === act.path ? (
                          <Check className="size-3 text-status-create" />
                        ) : (
                          <Copy className="size-3 opacity-0 group-hover:opacity-100" />
                        )}
                      </button>
                    ) : null}
                  </div>

                  {act.current && act.total ? (
                    <span className="shrink-0 text-outline">
                      {act.current} / {act.total}
                    </span>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      ) : null}

      {/* 阶段指标 */}
      {metrics.length > 0 ? (
        <div className="flex flex-wrap gap-2 pt-1">
          {metrics.map((m) => (
            <span
              className="inline-flex items-center gap-1 rounded-md border border-theme-control-border/60 bg-theme-control/30 px-2 py-0.5 text-caption text-on-surface-variant"
              key={m.code}
            >
              <span>{m.code}:</span>
              <strong className="text-on-surface">{m.value}</strong>
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function StageStatusBadge({ status }: { status: string }) {
  switch (status) {
    case "running":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
          <Loader2 className="size-3 animate-spin" />
          运行中
        </span>
      );
    case "succeeded":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-create/30 bg-status-create/10 px-2 py-0.5 text-caption font-medium text-status-create">
          <CheckCircle2 className="size-3" />
          完成
        </span>
      );
    case "partial_success":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-warning/30 bg-status-warning/10 px-2 py-0.5 text-caption font-medium text-status-warning">
          <AlertTriangle className="size-3" />
          部分成功
        </span>
      );
    case "failed":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-conflict/30 bg-status-conflict/10 px-2 py-0.5 text-caption font-medium text-status-conflict">
          <XCircle className="size-3" />
          失败
        </span>
      );
    case "canceled":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/40 px-2 py-0.5 text-caption font-medium text-on-surface-variant">
          <Ban className="size-3" />
          已取消
        </span>
      );
    default:
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/30 px-2 py-0.5 text-caption text-outline">
          <Clock className="size-3" />
          等待
        </span>
      );
  }
}

function TaskStateBadge({
  state,
  outcome,
}: {
  state: TaskState;
  outcome?: string | null;
}) {
  if (state === "running" || state === "pending" || state === "cancelling") {
    return (
      <span className="inline-flex items-center gap-1 rounded-md border border-primary/30 bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
        <Loader2 className="size-3 animate-spin" />
        {state === "cancelling" ? "取消中" : "运行中"}
      </span>
    );
  }

  if (outcome === "partial_success") {
    return (
      <span className="inline-flex items-center gap-1 rounded-md border border-status-warning/30 bg-status-warning/10 px-2 py-0.5 text-caption font-medium text-status-warning">
        <AlertTriangle className="size-3" />
        部分成功
      </span>
    );
  }

  if (state === "succeeded" || outcome === "success") {
    return (
      <span className="inline-flex items-center gap-1 rounded-md border border-status-create/30 bg-status-create/10 px-2 py-0.5 text-caption font-medium text-status-create">
        <Check className="size-3" />
        成功
      </span>
    );
  }

  if (state === "canceled" || outcome === "canceled") {
    return (
      <span className="inline-flex items-center gap-1 rounded-md border border-theme-control-border bg-theme-control/40 px-2 py-0.5 text-caption text-on-surface-variant">
        <Ban className="size-3" />
        已取消
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 rounded-md border border-status-conflict/30 bg-status-conflict/10 px-2 py-0.5 text-caption font-medium text-status-conflict">
      <AlertCircle className="size-3" />
      失败
    </span>
  );
}

function TaskKindIcon({
  kind,
  className = "size-3.5",
}: {
  kind: string;
  className?: string;
}) {
  switch (kind) {
    case "ConversationSync":
      return <MessageSquare className={className} />;
    case "Memory":
      return <Brain className={className} />;
    case "Backup":
      return <Archive className={className} />;
    case "Scan":
      return <RefreshCw className={className} />;
    case "AiExecution":
      return <Sparkles className={className} />;
    default:
      return <Layers className={className} />;
  }
}

function formatDuration(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  const secs = (ms / 1000).toFixed(1);
  return `${secs}s`;
}

function formatTimeShort(dateStr?: string | null) {
  if (!dateStr) return "-";
  try {
    const d = new Date(dateStr);
    return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}:${d.getSeconds().toString().padStart(2, "0")}`;
  } catch {
    return dateStr;
  }
}

interface TaskAgentSessionObserverProps {
  sessionRef: AgentSessionRef;
  taskTitle?: string;
  onBack: () => void;
}

function TaskAgentSessionObserver({
  sessionRef,
  taskTitle,
  onBack,
}: TaskAgentSessionObserverProps) {
  const [result, setResult] = useState<AgentSessionGetResult | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    const fetchSession = async () => {
      try {
        const data = await getAgentSession({ sessionRef });
        if (!cancelled) {
          setResult(data);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    void fetchSession();

    void subscribeAgentSessionUpdated(sessionRef, () => {
      void fetchSession();
    }).then((dispose) => {
      if (cancelled) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });

    const timer = setInterval(() => {
      if (
        result &&
        (!isAgentSessionAvailable(result) || result.state === "terminal")
      ) {
        return;
      }
      void fetchSession();
    }, 1500);

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
      clearInterval(timer);
    };
  }, [sessionRef]);

  const timelineItems = useMemo(() => {
    if (!result || !isAgentSessionAvailable(result)) return [];
    return result.items.map(mapSessionItemSnapshotToView);
  }, [result]);

  const recipientTitle = useMemo(() => {
    if (result && isAgentSessionAvailable(result)) {
      return (
        result.agent.displayName || result.agent.id || "Session Memory Agent"
      );
    }
    return "Session Memory Agent";
  }, [result]);

  const isAvailable = isAgentSessionAvailable(result);

  return (
    <div
      className="flex min-h-[500px] flex-1 flex-col overflow-hidden rounded-2xl border border-theme-control-border bg-surface-elevated/40"
      data-testid="task-agent-session-observer"
    >
      {/* 顶部观察者导航栏 */}
      <div className="flex items-center justify-between border-b border-theme-control-border/60 bg-surface-base/80 px-4 py-3 backdrop-blur-sm">
        <div className="flex items-center gap-2.5">
          <Button
            className="h-8 gap-1.5 px-2 text-caption font-medium text-on-surface hover:text-primary"
            data-testid="task-observer-back-btn"
            onClick={onBack}
            size="sm"
            variant="ghost"
          >
            <ArrowLeft className="size-4" />
            <span>返回任务</span>
          </Button>
          <div className="h-4 w-px bg-theme-control-border" />
          <span className="text-body-sm font-semibold text-on-surface">
            {taskTitle ? `${taskTitle} · ` : ""}执行现场
          </span>
          <span className="rounded-md border border-primary/20 bg-primary/10 px-2 py-0.5 text-caption font-medium text-primary">
            只读观察模式
          </span>
        </div>

        {result &&
        isAgentSessionAvailable(result) &&
        result.state === "terminal" ? (
          <span className="text-caption text-on-surface-variant">
            任务阶段已归档
          </span>
        ) : null}
      </div>

      {/* 现场内容主体 */}
      <div className="flex min-h-0 flex-1 flex-col">
        {loading && !result ? (
          <div
            className="flex flex-1 items-center justify-center gap-2 p-8 text-caption text-on-surface-variant"
            data-testid="task-observer-loading"
          >
            <Loader2 className="size-4 animate-spin text-primary" />
            <span>加载执行现场...</span>
          </div>
        ) : error ? (
          <div
            className="flex flex-1 flex-col items-center justify-center p-8 text-center"
            data-testid="task-observer-error"
          >
            <AlertCircle className="size-8 text-status-conflict" />
            <p className="mt-2 text-body-sm text-status-conflict">{error}</p>
          </div>
        ) : (
          <AgentSessionWorkspace
            capabilities={{
              send: false,
              stop: false,
              retry: false,
              queue: false,
              interrupt: false,
              attach: false,
              mention: false,
              slashCommand: false,
              modelSelect: false,
              permissionResponse: false,
              copy: true,
              openArtifact: true,
            }}
            disabled={true}
            emptyDescription="该阶段尚未产生交互事件或正在初始化"
            emptyTitle="暂无执行记录"
            isReadOnly={true}
            items={timelineItems}
            recipientTitle={recipientTitle}
            status={
              isAvailable
                ? {
                    className:
                      result.state === "active"
                        ? "text-primary"
                        : "text-on-surface-variant",
                    label: result.state === "active" ? "执行中" : "已结束",
                  }
                : undefined
            }
            testIdPrefix="task-agent-observer"
            unavailable={!isAvailable}
            unavailableDescription={
              !isAvailable && result
                ? result.reason === "notFoundOrExpired"
                  ? "该阶段执行现场已过期（执行流为内存临时态，只在任务存续期间及归档前保留）"
                  : result.reason
                : undefined
            }
          />
        )}
      </div>
    </div>
  );
}
