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
  ChevronDown,
  Clock,
  Copy,
  Layers,
  ListTodo,
  Loader2,
  MessageSquare,
  Minus,
  RefreshCw,
  RotateCcw,
  Search,
  Sparkles,
  Trash2,
  XCircle,
} from "lucide-react";
import clsx from "clsx";
import { Group, Panel, Separator } from "react-resizable-panels";
import { DialogFrame } from "../foundation/DialogFrame";
import { EmptyState } from "../foundation/EmptyState";
import { ErrorBoundary } from "../foundation/ErrorBoundary";
import { Button } from "../ui/button";
import { PillTabs } from "../common/PillTabs";
import { ToolbarSearch } from "../common/DataToolbar";
import { useI18n } from "../../i18n/I18nProvider";
import { useTaskCenter } from "../../app/backgroundTasks/TaskCenterProvider";
import {
  AgentSessionWorkspace,
  mapSessionItemSnapshotToView,
} from "../agent-session";
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

function formatTaskCategoryLabel(
  category?: string | null,
  kind?: string,
): string {
  if (!category && !kind) return "";
  const cat = (category ?? "").toLowerCase();
  if (cat === "conversation/session_sync") return "会话同步";
  if (cat === "conversation/web_sync") return "网页记录同步";
  if (cat === "conversation/sync") return "全量会话同步";
  if (cat === "conversation/pricing_sync") return "模型价格同步与重算";
  if (cat === "conversation/usage_scan") return "用量统计扫描";
  if (kind === "ConversationUsageScan") return "用量统计扫描";
  return category || kind || "";
}

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
        const friendlyCategory = formatTaskCategoryLabel(
          task.category,
          task.kind,
        ).toLowerCase();
        const matchesCategory =
          (task.category ?? "").toLowerCase().includes(query) ||
          friendlyCategory.includes(query);
        const matchesKind =
          (task.kind ?? "").toLowerCase().includes(query) || matchesCategory;
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
              size="default"
              type="button"
              variant="outline"
            >
              <Trash2 className="mr-1.5 size-4" />
              {t("tasks.action.clearCompleted")}
            </Button>
            <Button
              onClick={onClose}
              size="default"
              type="button"
              variant="default"
            >
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
      <Group
        className="flex h-full w-full min-h-0 flex-1 overflow-hidden"
        orientation="horizontal"
      >
        {/* 左栏：任务列表面板（可调宽度，最大 50%） */}
        <Panel
          className="flex min-h-0 min-w-0 flex-col"
          defaultSize="29%"
          id="task-center-left-panel"
          maxSize="50%"
          minSize="22%"
        >
          <aside className="flex h-full w-full flex-col bg-surface-elevated/40">
            {/* 搜索与过滤工具区 */}
            <div className="flex flex-col gap-2.5 border-b border-theme-control-border p-3">
              <ToolbarSearch
                ariaLabel={t("tasks.search.placeholder")}
                className="w-full !min-w-0"
                onChange={(val) => setSearchQuery(val)}
                placeholder={t("tasks.search.placeholder")}
                value={searchQuery}
              />

              {/* 状态过滤切换 */}
              <PillTabs<FilterStatus>
                activeId={filterStatus}
                ariaLabel={t("tasks.filter.all")}
                fullWidth
                items={[
                  { id: "all", label: t("tasks.filter.all") },
                  { id: "running", label: t("tasks.filter.running") },
                  { id: "completed", label: t("tasks.filter.completed") },
                  { id: "failed", label: t("tasks.filter.failed") },
                ]}
                onSelect={(status) => setFilterStatus(status)}
                size="sm"
              />
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
                  className="ui-view-transition flex flex-col gap-2"
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
                            <span className="capitalize">
                              {formatTaskCategoryLabel(
                                task.category,
                                task.kind,
                              )}
                            </span>
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
        </Panel>

        {/* 可拖拽分栏手柄 */}
        <Separator
          aria-label={t("layout.resizeColumns")}
          className={clsx(
            "relative z-10 w-3 -translate-x-1/2 cursor-col-resize touch-none outline-none",
            "ui-resize-handle before:absolute before:inset-y-0 before:left-1/2 before:w-px before:-translate-x-1/2 before:bg-theme-card-border",
            "after:absolute after:left-1/2 after:top-1/2 after:h-10 after:w-1.5 after:-translate-x-1/2 after:-translate-y-1/2 after:rounded-full after:bg-theme-control-border after:opacity-0 after:transition-opacity",
            "hover:after:opacity-100 focus-visible:after:opacity-100 focus-visible:ring-2 focus-visible:ring-primary-strong/55",
          )}
          id="task-center-separator"
        />

        {/* 右栏：选中任务详情面板（带平滑视图切换动效与流动微光进度条） */}
        <Panel
          className="flex min-h-0 min-w-0 flex-col"
          defaultSize="71%"
          id="task-center-right-panel"
          minSize="50%"
        >
          <main className="relative flex h-full min-h-0 flex-1 flex-col overflow-y-auto bg-surface-base p-6">
            {activeCount > 0 || actionBusy ? (
              <div className="ui-route-progress" />
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
                  className="ui-view-transition flex flex-col gap-6"
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
                                {selectedTask.category ? (
                                  <span>
                                    分类:{" "}
                                    <code className="rounded-full bg-theme-control/50 px-2 py-0.5 font-mono text-[11px] text-primary">
                                      {formatTaskCategoryLabel(
                                        selectedTask.category,
                                        selectedTask.kind,
                                      )}
                                    </code>
                                  </span>
                                ) : null}
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
                                    开始于:{" "}
                                    {new Date(startedAt).toLocaleString()}
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
                                  onClick={() =>
                                    void handleCancel(selectedTask)
                                  }
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

                          {resultSummary && selectedTask.state !== "failed" ? (
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

                          {/* 全局执行进度条 */}
                          {(() => {
                            const totalStages = stages.length;
                            const completedStages = stages.filter(
                              (s) => s.status === "succeeded",
                            ).length;
                            const percent =
                              totalStages > 0
                                ? Math.round(
                                    (completedStages / totalStages) * 100,
                                  )
                                : selectedTask.state === "succeeded"
                                  ? 100
                                  : 0;
                            return (
                              <div className="flex flex-col gap-1.5 pt-2 border-t border-theme-control-border/40">
                                <div className="flex items-center justify-between text-caption font-medium text-on-surface-variant">
                                  <span>执行进度</span>
                                  <span className="font-mono">{percent}%</span>
                                </div>
                                <div className="h-2 w-full overflow-hidden rounded-full bg-theme-control/60">
                                  <div
                                    className={clsx(
                                      "h-full rounded-full transition-all duration-500",
                                      selectedTask.state === "succeeded"
                                        ? "bg-status-create"
                                        : selectedTask.state === "failed"
                                          ? "bg-status-conflict"
                                          : "bg-primary",
                                    )}
                                    style={{ width: `${percent}%` }}
                                  />
                                </div>
                              </div>
                            );
                          })()}
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
                            <div className="flex flex-col">
                              {stages.map((stage, idx) => (
                                <StageCard
                                  index={idx + 1}
                                  isLast={idx === stages.length - 1}
                                  key={stage.id || `stage-${idx}`}
                                  onViewSession={(ref) =>
                                    setViewingSessionRef(ref)
                                  }
                                  stage={stage}
                                  taskErrorSummary={errorSummary}
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
        </Panel>
      </Group>
    </DialogFrame>
  );
}

function StageCard({
  stage,
  index,
  isLast,
  onViewSession,
  taskErrorSummary,
}: {
  stage: TaskStageView;
  index: number;
  isLast: boolean;
  onViewSession?: (sessionRef: AgentSessionRef) => void;
  taskErrorSummary?: string | null;
}) {
  const [expanded, setExpanded] = useState<boolean>(
    stage.status === "running" || stage.status === "failed",
  );
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    if (stage.status === "running" || stage.status === "failed") {
      setExpanded(true);
    }
  }, [stage.status]);

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
  const steps = stage.steps ?? [];
  const failures = stage.failures ?? [];
  const metrics = stage.metrics ?? [];
  const sessionRef = stage.agentSessionRef ?? stage.agent_session_ref;

  return (
    <div className="relative flex items-start gap-3.5">
      {/* 左侧垂直导轨与节点圆徽 */}
      <div className="relative flex flex-col items-center shrink-0 self-stretch">
        <div
          className={clsx(
            "flex size-7 items-center justify-center rounded-full transition-all duration-200 z-10",
            stage.status === "succeeded" &&
              "border border-status-create/40 bg-status-create/15 text-status-create shadow-sm shadow-status-create/10",
            stage.status === "running" &&
              "border border-primary/50 bg-primary/20 text-primary shadow-sm shadow-primary/20 ring-2 ring-primary/20 animate-pulse",
            stage.status === "failed" &&
              "border border-status-conflict/40 bg-status-conflict/15 text-status-conflict",
            stage.status === "skipped" &&
              "border border-theme-control-border bg-theme-control/40 text-on-surface-muted",
            stage.status === "pending" &&
              "border border-theme-control-border/60 bg-theme-control/20 text-on-surface-muted",
          )}
        >
          {stage.status === "succeeded" && (
            <Check className="size-3.5 stroke-[2.5]" />
          )}
          {stage.status === "running" && (
            <Loader2 className="size-3.5 animate-spin stroke-[2.5]" />
          )}
          {stage.status === "failed" && (
            <XCircle className="size-3.5 stroke-[2.5]" />
          )}
          {stage.status === "skipped" && (
            <Minus className="size-3.5 stroke-[2.5]" />
          )}
          {stage.status === "pending" && (
            <span className="text-code-xs font-semibold">{index}</span>
          )}
        </div>

        {/* 垂直导轨竖线 */}
        {!isLast ? (
          <div
            className={clsx(
              "w-0.5 flex-1 my-1 min-h-[36px] rounded-full transition-colors duration-300",
              stage.status === "succeeded"
                ? "bg-status-create/30"
                : "bg-theme-control-border/40",
            )}
          />
        ) : null}
      </div>

      {/* 右侧阶段内容卡片 */}
      <div
        className={clsx(
          "flex flex-1 flex-col gap-2 rounded-2xl border border-theme-control-border bg-surface-elevated/40 p-4 transition-all mb-4",
          stage.status === "running" &&
            "border-primary/40 bg-surface-elevated/70 shadow-sm shadow-primary/5",
          stage.status === "failed" &&
            "border-status-conflict/40 bg-surface-elevated/70 shadow-sm shadow-status-conflict/5",
          stage.status === "skipped" && "opacity-60 bg-surface-elevated/20",
        )}
      >
        {/* 阶段标题栏 */}
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <span className="text-body-base font-semibold text-on-surface">
              {stage.name}
            </span>
            {durationMs ? (
              <span className="rounded-full bg-theme-control/40 px-2 py-0.5 text-code-xs text-on-surface-variant font-mono">
                耗时: {formatDuration(durationMs)}
              </span>
            ) : null}
          </div>

          <div className="flex items-center gap-3">
            <StageStatusBadge status={stage.status} />
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

        {/* 阶段 Note 说明 (常驻展示，即使完成也不丢失) */}
        {stage.progress?.note ? (
          <p className="text-body-sm text-on-surface-variant break-all">
            {stage.progress.note}
          </p>
        ) : null}

        {/* 阶段数值进度条 */}
        {percent !== null ? (
          <div className="flex flex-col gap-1 py-0.5">
            <div className="flex justify-between text-caption text-on-surface-variant font-mono">
              <span>
                {stage.progress?.current} / {stage.progress?.total}
              </span>
              <span>{percent}%</span>
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-theme-control/60">
              <div
                className="h-full rounded-full bg-primary transition-all duration-300"
                style={{ width: `${percent}%` }}
              />
            </div>
          </div>
        ) : null}

        {/* 阶段内联异常警示面板 (完整展现失败原因、错误码与路径) */}
        {failures.length > 0 ? (
          <div className="flex flex-col gap-2 rounded-xl border border-status-conflict/40 bg-status-conflict/10 p-3 text-body-sm">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2 font-medium">
                <AlertCircle className="size-4 shrink-0 text-status-conflict" />
                <span className="font-semibold text-status-conflict">
                  阶段异常报错 ({failures.length})
                </span>
              </div>
            </div>
            <div className="flex flex-col gap-2">
              {failures.map((fail, fIdx) => (
                <div
                  className="flex flex-col gap-1.5 rounded-xl border border-status-conflict/20 bg-surface-base/80 p-2.5 text-on-surface"
                  key={fIdx}
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-1.5">
                      <span className="rounded-full bg-status-conflict/20 px-2 py-0.5 font-mono text-code-xs font-semibold text-status-conflict">
                        {fail.code}
                      </span>
                      <span
                        className={clsx(
                          "rounded-full px-2 py-0.5 text-code-xs font-mono font-medium",
                          fail.retryable
                            ? "bg-status-warning/20 text-status-warning"
                            : "bg-status-conflict/15 text-status-conflict",
                        )}
                      >
                        {fail.retryable ? "可重试" : "致命异常"}
                      </span>
                    </div>
                    {fail.timestamp ? (
                      <span className="font-mono text-code-xs text-on-surface-muted">
                        {new Date(fail.timestamp).toLocaleTimeString()}
                      </span>
                    ) : null}
                  </div>

                  <p className="font-mono text-body-sm text-on-surface-variant break-all leading-relaxed whitespace-pre-wrap">
                    {fail.message}
                  </p>

                  {fail.path || fail.identity ? (
                    <div className="flex items-center justify-between gap-2 rounded-lg bg-theme-control/20 px-2 py-1 font-mono text-code-xs text-on-surface-muted">
                      <span
                        className="truncate max-w-[420px]"
                        title={fail.path ?? fail.identity ?? ""}
                      >
                        {fail.path ?? fail.identity}
                      </span>
                      <button
                        className="shrink-0 p-1 hover:text-on-surface transition-colors"
                        onClick={() =>
                          copyPath(fail.path ?? fail.identity ?? "")
                        }
                        title="复制路径/标识"
                        type="button"
                      >
                        {copied === (fail.path ?? fail.identity) ? (
                          <Check className="size-3 text-status-create" />
                        ) : (
                          <Copy className="size-3" />
                        )}
                      </button>
                    </div>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        ) : stage.status === "failed" ? (
          <div className="flex flex-col gap-1.5 rounded-xl border border-status-conflict/30 bg-status-conflict/10 p-2.5 text-body-sm text-status-conflict">
            <div className="flex items-center gap-2 font-medium">
              <AlertCircle className="size-4 shrink-0 text-status-conflict" />
              <span>该阶段执行失败并中断</span>
            </div>
            {taskErrorSummary ? (
              <p className="font-mono text-caption text-on-surface-variant break-all leading-relaxed whitespace-pre-wrap pl-6">
                {taskErrorSummary}
              </p>
            ) : null}
          </div>
        ) : null}

        {/* 活跃 Workers (DAG 子节点卡片网格，实时更新且不闪烁) */}
        {currentActivities.length > 0 ? (
          <div className="flex flex-col gap-2 pt-1">
            <div className="flex items-center justify-between">
              <span className="text-caption font-medium text-primary flex items-center gap-1.5">
                <Activity className="size-3.5 animate-pulse" />
                活跃 Workers ({currentActivities.length})
              </span>
              <span className="font-mono text-code-xs text-on-surface-variant">
                DAG 工作流活跃节点
              </span>
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {currentActivities.map((act, actIdx) => {
                const workerId =
                  act.workerId ?? act.worker_id ?? `worker-${actIdx}`;
                const path =
                  act.path ?? act.displayPath ?? act.display_path ?? null;
                return (
                  <div
                    className="flex flex-col gap-1.5 rounded-xl border border-primary/25 bg-surface-base/80 p-2.5 text-caption shadow-xs hover:border-primary/40 transition-colors"
                    key={workerId}
                  >
                    <div className="flex items-center justify-between gap-1.5">
                      <div className="flex items-center gap-1.5 overflow-hidden">
                        <span className="relative flex size-2 shrink-0">
                          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-75" />
                          <span className="relative inline-flex size-2 rounded-full bg-primary" />
                        </span>
                        <span className="font-mono text-code-xs font-semibold text-primary truncate">
                          [{workerId}]
                        </span>
                      </div>
                      {act.current != null && act.total != null ? (
                        <span className="shrink-0 font-mono text-code-xs text-on-surface-variant">
                          {act.current} / {act.total}
                        </span>
                      ) : null}
                    </div>
                    <div className="text-body-xs font-medium text-on-surface truncate">
                      {act.operation}
                    </div>
                    {path ? (
                      <div className="flex items-center justify-between gap-1 rounded-lg bg-theme-control/25 px-1.5 py-0.5 font-mono text-code-xs text-on-surface-variant overflow-hidden">
                        <span className="truncate" title={path}>
                          {path}
                        </span>
                        <button
                          className="shrink-0 p-0.5 hover:text-on-surface transition-colors"
                          onClick={() => copyPath(path)}
                          title="复制路径"
                          type="button"
                        >
                          {copied === path ? (
                            <Check className="size-2.5 text-status-create" />
                          ) : (
                            <Copy className="size-2.5 text-on-surface-muted" />
                          )}
                        </button>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {/* 展开/收起流程历史记录 (完成状态后亦可常驻展开回放) */}
        {steps.length > 0 ? (
          <div className="flex flex-col gap-2 pt-1">
            <button
              className="flex items-center gap-1.5 text-caption font-medium text-primary hover:text-primary-hover transition-colors self-start"
              onClick={() => setExpanded(!expanded)}
              type="button"
            >
              <ChevronDown
                className={clsx(
                  "size-3.5 transition-transform duration-200",
                  !expanded && "-rotate-90",
                )}
              />
              <span>
                {expanded
                  ? "收起流程明细"
                  : `查看详细流程 (${steps.length} 个子节点步骤)`}
              </span>
            </button>

            {expanded ? (
              <div className="flex flex-col gap-1.5 rounded-xl border border-theme-control-border/60 bg-surface-base/60 p-2.5">
                <span className="text-code-xs text-on-surface-variant font-medium px-1">
                  执行步骤序列 ({steps.length})
                </span>
                <div className="max-h-64 overflow-y-auto pr-1 flex flex-col gap-1.5">
                  {steps.map((step, sIdx) => {
                    const timeStr = step.timestamp
                      ? new Date(step.timestamp).toLocaleTimeString()
                      : "";
                    const isFailureStep =
                      step.operation.includes("失败") ||
                      step.operation.toLowerCase().includes("failed") ||
                      (step.detail &&
                        (step.detail.includes("失败") ||
                          step.detail.toLowerCase().includes("error")));
                    return (
                      <div
                        className={clsx(
                          "flex items-start justify-between gap-2 rounded-xl border px-2.5 py-1.5 text-caption transition-colors",
                          isFailureStep
                            ? "border-status-conflict/30 bg-status-conflict/10 text-status-conflict"
                            : "border-theme-control-border/30 bg-theme-control/10 text-on-surface hover:bg-theme-control/20",
                        )}
                        key={sIdx}
                      >
                        <div className="flex items-start gap-2 overflow-hidden flex-1">
                          <span className="shrink-0 text-code-xs text-on-surface-muted font-mono pt-0.5">
                            {timeStr || `#${sIdx + 1}`}
                          </span>
                          <span
                            className={clsx(
                              "shrink-0 font-mono text-code-xs rounded-full px-2 py-0.5 font-medium",
                              isFailureStep
                                ? "bg-status-conflict/20 text-status-conflict"
                                : "bg-primary/10 text-primary",
                            )}
                          >
                            {step.operation}
                          </span>
                          {step.detail ? (
                            <span
                              className="font-mono text-code-xs text-on-surface-variant break-all pt-0.5"
                              title={step.detail}
                            >
                              {step.detail}
                            </span>
                          ) : null}
                        </div>
                        {step.current != null && step.total != null ? (
                          <span className="shrink-0 font-mono text-on-surface-variant text-code-xs pt-0.5">
                            {step.current}/{step.total}
                          </span>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              </div>
            ) : null}
          </div>
        ) : null}

        {/* 当无子步骤且阶段处于活动/终态时的友好提示 */}
        {steps.length === 0 &&
        stage.status !== "pending" &&
        stage.status !== "skipped" ? (
          <div className="flex items-center gap-1.5 pt-1 text-code-xs text-on-surface-muted italic">
            <span>暂无子节点明细步骤记录</span>
          </div>
        ) : null}

        {/* 阶段指标 */}
        {metrics.length > 0 ? (
          <div className="flex flex-wrap gap-2 pt-1">
            {metrics.map((m) => (
              <span
                className="inline-flex items-center gap-1 rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2.5 py-0.5 text-caption text-on-surface-variant"
                key={m.code}
              >
                <span>{m.code}:</span>
                <strong className="text-on-surface">{m.value}</strong>
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function StageStatusBadge({ status }: { status: string }) {
  switch (status) {
    case "running":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-caption font-medium text-primary">
          <Loader2 className="size-3 animate-spin" />
          运行中
        </span>
      );
    case "succeeded":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-create/30 bg-status-create/10 px-2.5 py-0.5 text-caption font-medium text-status-create">
          <CheckCircle2 className="size-3" />
          已完成
        </span>
      );
    case "partial_success":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-warning/30 bg-status-warning/10 px-2.5 py-0.5 text-caption font-medium text-status-warning">
          <AlertTriangle className="size-3" />
          部分成功
        </span>
      );
    case "failed":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-conflict/30 bg-status-conflict/10 px-2.5 py-0.5 text-caption font-medium text-status-conflict">
          <XCircle className="size-3" />
          失败
        </span>
      );
    case "canceled":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/40 px-2.5 py-0.5 text-caption font-medium text-on-surface-variant">
          <Ban className="size-3" />
          已取消
        </span>
      );
    case "skipped":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/20 px-2.5 py-0.5 text-caption font-medium text-outline">
          <Minus className="size-3" />
          已跳过
        </span>
      );
    default:
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/30 px-2.5 py-0.5 text-caption text-outline">
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
      <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-caption font-medium text-primary">
        <Loader2 className="size-3 animate-spin" />
        {state === "cancelling" ? "取消中" : "运行中"}
      </span>
    );
  }

  if (outcome === "partial_success") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-status-warning/30 bg-status-warning/10 px-2.5 py-0.5 text-caption font-medium text-status-warning">
        <AlertTriangle className="size-3" />
        部分成功
      </span>
    );
  }

  if (state === "succeeded" || outcome === "success") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-status-create/30 bg-status-create/10 px-2.5 py-0.5 text-caption font-medium text-status-create">
        <Check className="size-3" />
        成功
      </span>
    );
  }

  if (state === "canceled" || outcome === "canceled") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/40 px-2.5 py-0.5 text-caption text-on-surface-variant">
        <Ban className="size-3" />
        已取消
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-status-conflict/30 bg-status-conflict/10 px-2.5 py-0.5 text-caption font-medium text-status-conflict">
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
    case "ConversationUsageScan":
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
          <span className="rounded-full border border-primary/25 bg-primary/10 px-2.5 py-0.5 text-caption font-medium text-primary">
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
