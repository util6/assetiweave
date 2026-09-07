const TERMINAL_STATUSES = new Set([
  "completed",
  "failed",
  "cancelled",
  "canceled",
  "success",
  "succeeded",
]);

export function isTerminalStatus(status: unknown): boolean {
  if (typeof status !== "string") return false;
  return TERMINAL_STATUSES.has(status.trim().toLowerCase());
}

export interface GenericTaskSnapshot {
  taskId?: string;
  task_id?: string;
  status?: string;
  state?: string;
  updated_at?: string | number;
  updatedAt?: string | number;
  created_at?: string | number;
  createdAt?: string | number;
  started_at?: string | number;
  completed_at?: string | number | null;
  tenant_id?: string;
  tenantId?: string;
  metadata?: Record<string, unknown> | null;
  scope?: Record<string, unknown> | null;
  [key: string]: unknown;
}

/**
 * 校验事件是否属于指定租户：
 * - 返回 true: 明确属于当前租户
 * - 返回 false: 明确属于其他租户（应直接过滤丢弃）
 * - 返回 null: 事件未携带租户信息（缺乏归属信息，应触发重查而非盲写）
 */
export function checkEventTenantMatch(
  event: unknown,
  activeTenantId: string,
): boolean | null {
  if (!event || typeof event !== "object") return null;
  const raw = event as Record<string, unknown>;
  const rawMeta =
    raw.metadata && typeof raw.metadata === "object"
      ? (raw.metadata as Record<string, unknown>)
      : null;
  const rawScope =
    raw.scope && typeof raw.scope === "object"
      ? (raw.scope as Record<string, unknown>)
      : null;

  const eventTenant =
    raw.tenant_id ??
    raw.tenantId ??
    rawMeta?.tenant_id ??
    rawMeta?.tenantId ??
    rawScope?.tenant_id ??
    rawScope?.tenantId;

  if (typeof eventTenant === "string") {
    return eventTenant === activeTenantId;
  }
  return null;
}

/**
 * 领域任务合并规则：
 * 1. 保护终态不可逆：若 current 已经是 completed/failed/cancelled，而 incoming 是 running/pending，拒绝倒退。
 * 2. 时间戳优先：若两者都有更新时间戳，晚者胜出。
 * 3. 数组按最新项提取。
 */
export function mergeTaskSnapshot<T>(
  current: T,
  incoming: T | T[],
): T {
  const actualIncoming = (
    Array.isArray(incoming) ? incoming[0] : incoming
  ) as T;

  if (!current) return actualIncoming;
  if (!actualIncoming) return current;

  const cur = current as Record<string, any>;
  const inc = actualIncoming as Record<string, any>;

  const currentTaskId = cur.id ?? cur.taskId ?? cur.task_id;
  const incomingTaskId = inc.id ?? inc.taskId ?? inc.task_id;

  // 同一个任务的合并规则
  if (currentTaskId && incomingTaskId && currentTaskId === incomingTaskId) {
    const curStatus = cur.status ?? cur.state;
    const incStatus = inc.status ?? inc.state;
    // 终态保护：当前已终态， incoming 是非终态，保持当前终态！
    if (isTerminalStatus(curStatus) && !isTerminalStatus(incStatus)) {
      return current;
    }
  }

  // 时间戳比较
  const currentTs =
    cur.finished_at ??
    cur.completed_at ??
    cur.updated_at ??
    cur.updatedAt ??
    cur.created_at ??
    cur.createdAt ??
    cur.started_at;
  const incomingTs =
    inc.finished_at ??
    inc.completed_at ??
    inc.updated_at ??
    inc.updatedAt ??
    inc.created_at ??
    inc.createdAt ??
    inc.started_at;

  if (currentTs && incomingTs) {
    const currentMs = new Date(currentTs).getTime();
    const incomingMs = new Date(incomingTs).getTime();
    if (!Number.isNaN(currentMs) && !Number.isNaN(incomingMs)) {
      if (currentMs > incomingMs) {
        return current;
      }
    }
  }

  return actualIncoming;
}
