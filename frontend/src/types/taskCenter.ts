import type { AgentSessionRef } from "./agentSession";

export type TaskState =
  "pending" | "running" | "cancelling" | "succeeded" | "failed" | "canceled";

export type TaskOutcome =
  "success" | "partial_success" | "failure" | "canceled";

export type StageStatus =
  | "pending"
  | "running"
  | "succeeded"
  | "partial_success"
  | "failed"
  | "canceled"
  | "skipped";

export interface TaskProgress {
  current: number;
  total?: number | null;
  note?: string | null;
}

export interface TaskActivityView {
  stageId?: string;
  stage_id?: string;
  workerId?: string;
  worker_id?: string;
  operation: string;
  path?: string | null;
  displayPath?: string | null;
  display_path?: string | null;
  startedAt?: string;
  started_at?: string;
  current?: number | null;
  total?: number | null;
}

export interface TaskMetricView {
  code: string;
  value: number;
}

export interface TaskFailureView {
  code: string;
  message: string;
  stage: string;
  identity?: string | null;
  retryable: boolean;
  path?: string | null;
  timestamp: string;
}

export interface TaskSkippedGroupView {
  id: string;
  title: string;
  reason: string;
}

export interface TaskStageView {
  id: string;
  name: string;
  status: StageStatus;
  startedAt?: string | null;
  started_at?: string | null;
  finishedAt?: string | null;
  finished_at?: string | null;
  durationMs?: number | null;
  duration_ms?: number | null;
  progress?: TaskProgress | null;
  currentActivities?: TaskActivityView[];
  current_activities?: TaskActivityView[];
  metrics: TaskMetricView[];
  failures: TaskFailureView[];
  skipped?: TaskSkippedGroupView[];
  agentSessionRef?: AgentSessionRef | null;
  agent_session_ref?: AgentSessionRef | null;
}

export interface TaskCapabilitiesView {
  cancellable: boolean;
  retryable: boolean;
  clearable: boolean;
}

export interface TaskListParams {
  tenant_id?: string;
  kind?: string;
  state?: TaskState;
  limit?: number;
  active_only?: boolean;
}

export interface TaskGetParams {
  task_id: string;
}

export interface TaskCancelParams {
  task_id: string;
}

export interface TaskRetryParams {
  task_id: string;
}

export interface TaskClearParams {
  terminal_only?: boolean;
  tenant_id?: string;
  task_id?: string;
}

export interface TaskView {
  id: string;
  kind: string;
  title: string;
  tenantId?: string | null;
  tenant_id?: string | null;
  state: TaskState;
  outcome?: TaskOutcome | null;
  startedAt?: string | null;
  started_at?: string | null;
  updatedAt?: string | null;
  updated_at?: string | null;
  finishedAt?: string | null;
  finished_at?: string | null;
  progress?: TaskProgress | null;
  stages: TaskStageView[];
  metrics: TaskMetricView[];
  failures: TaskFailureView[];
  errorSummary?: string | null;
  error_summary?: string | null;
  resultSummary?: string | null;
  result_summary?: string | null;
  capabilities: TaskCapabilitiesView;
  revision: number;
  agentSessionRef?: AgentSessionRef | null;
  agent_session_ref?: AgentSessionRef | null;
}

/**
 * 规范化 TaskView，双向对齐 camelCase 与 snake_case，并确保数组不为 null/undefined
 */
export function normalizeTaskView(raw: unknown): TaskView {
  if (!raw || typeof raw !== "object") {
    return raw as TaskView;
  }

  const data = raw as Record<string, unknown>;

  const rawStages = (data.stages as unknown[]) ?? [];
  const normalizedStages: TaskStageView[] = rawStages.map((stRaw) => {
    const st = (stRaw && typeof stRaw === "object" ? stRaw : {}) as Record<
      string,
      unknown
    >;
    const rawActivities = (st.currentActivities ??
      st.current_activities ??
      []) as unknown[];
    const normalizedActivities: TaskActivityView[] = rawActivities.map(
      (actRaw) => {
        const act = (
          actRaw && typeof actRaw === "object" ? actRaw : {}
        ) as Record<string, unknown>;
        const workerId = (act.workerId ?? act.worker_id ?? "") as string;
        const stageId = (act.stageId ?? act.stage_id ?? "") as string;
        const startedAt = (act.startedAt ?? act.started_at ?? "") as string;
        const displayPath = (act.displayPath ?? act.display_path ?? null) as
          string | null;

        return {
          stageId,
          stage_id: stageId,
          workerId,
          worker_id: workerId,
          operation: (act.operation ?? "") as string,
          path: (act.path ?? null) as string | null,
          displayPath,
          display_path: displayPath,
          startedAt,
          started_at: startedAt,
          current: typeof act.current === "number" ? act.current : null,
          total: typeof act.total === "number" ? act.total : null,
        };
      },
    );

    const durationMs =
      typeof st.durationMs === "number"
        ? st.durationMs
        : typeof st.duration_ms === "number"
          ? st.duration_ms
          : null;
    const startedAt = (st.startedAt ?? st.started_at ?? null) as string | null;
    const finishedAt = (st.finishedAt ?? st.finished_at ?? null) as
      string | null;

    return {
      id: (st.id ?? "") as string,
      name: (st.name ?? "") as string,
      status: (st.status ?? "pending") as StageStatus,
      startedAt,
      started_at: startedAt,
      finishedAt,
      finished_at: finishedAt,
      durationMs,
      duration_ms: durationMs,
      progress: (st.progress ?? null) as TaskProgress | null,
      currentActivities: normalizedActivities,
      current_activities: normalizedActivities,
      metrics: (st.metrics as TaskMetricView[]) ?? [],
      failures: (st.failures as TaskFailureView[]) ?? [],
      skipped: (st.skipped as TaskSkippedGroupView[]) ?? [],
      agentSessionRef: (st.agentSessionRef ??
        st.agent_session_ref ??
        null) as AgentSessionRef | null,
      agent_session_ref: (st.agentSessionRef ??
        st.agent_session_ref ??
        null) as AgentSessionRef | null,
    };
  });

  const tenantId = (data.tenantId ?? data.tenant_id ?? null) as string | null;
  const startedAt = (data.startedAt ?? data.started_at ?? null) as
    string | null;
  const updatedAt = (data.updatedAt ?? data.updated_at ?? null) as
    string | null;
  const finishedAt = (data.finishedAt ?? data.finished_at ?? null) as
    string | null;
  const errorSummary = (data.errorSummary ?? data.error_summary ?? null) as
    string | null;
  const resultSummary = (data.resultSummary ?? data.result_summary ?? null) as
    string | null;
  const taskSessionRef =
    ((data.agentSessionRef ??
      data.agent_session_ref ??
      null) as AgentSessionRef | null) ||
    normalizedStages.find((s) => Boolean(s.agentSessionRef))?.agentSessionRef ||
    null;

  return {
    id: (data.id ?? "") as string,
    kind: (data.kind ?? "") as string,
    title: (data.title ?? "") as string,
    tenantId,
    tenant_id: tenantId,
    state: (data.state ?? "pending") as TaskState,
    outcome: (data.outcome ?? null) as TaskOutcome | null,
    startedAt,
    started_at: startedAt,
    updatedAt,
    updated_at: updatedAt,
    finishedAt,
    finished_at: finishedAt,
    progress: (data.progress ?? null) as TaskProgress | null,
    stages: normalizedStages,
    metrics: (data.metrics as TaskMetricView[]) ?? [],
    failures: (data.failures as TaskFailureView[]) ?? [],
    errorSummary,
    error_summary: errorSummary,
    resultSummary,
    result_summary: resultSummary,
    capabilities: (data.capabilities ?? {
      cancellable: false,
      retryable: false,
      clearable: false,
    }) as TaskCapabilitiesView,
    revision: typeof data.revision === "number" ? data.revision : 0,
    agentSessionRef: taskSessionRef,
    agent_session_ref: taskSessionRef,
  };
}
