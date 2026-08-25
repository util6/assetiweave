import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import {
  auditConversationData,
  cancelConversationDataMaintenance,
  listConversationDataMaintenanceTasks,
  repairConversationData,
  subscribeConversationDataMaintenanceTasks,
  type ConversationDataAuditParams,
  type ConversationDataMaintenanceTaskSnapshot,
  type ConversationDataRepairParams,
} from "../../services/conversations";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

interface ConversationDataMaintenanceContextValue {
  task: ConversationDataMaintenanceTaskSnapshot | null;
  tasks: ConversationDataMaintenanceTaskSnapshot[];
  audit: (params?: ConversationDataAuditParams) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  repair: (params?: ConversationDataRepairParams) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  cancel: (taskId: string) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  refresh: () => Promise<void>;
}

const ConversationDataMaintenanceContext = createContext<ConversationDataMaintenanceContextValue | null>(null);

export function ConversationDataMaintenanceProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<ConversationDataMaintenanceTaskSnapshot[], ConversationDataMaintenanceTaskSnapshot>>(
    () => ({
      initialState: [],
      isRunning: (tasks) => tasks.some((task) => task.status === "running" || task.status === "cancelling"),
      merge: (current, incoming) => mergeMaintenanceTask(current, incoming),
      refresh: listConversationDataMaintenanceTasks,
      subscribe: (listener) => subscribeConversationDataMaintenanceTasks(listener),
    }),
    [],
  );
  const { merge, refresh, state: tasks } = useBackgroundTaskRuntime(adapter);

  const audit = useCallback(async (params: ConversationDataAuditParams = {}) => {
    const task = await auditConversationData(params);
    merge(task);
    return task;
  }, [merge]);
  const repair = useCallback(async (params: ConversationDataRepairParams = {}) => {
    const task = await repairConversationData(params);
    merge(task);
    return task;
  }, [merge]);
  const cancel = useCallback(async (taskId: string) => {
    const task = await cancelConversationDataMaintenance(taskId);
    merge(task);
    return task;
  }, [merge]);
  const task = tasks[tasks.length - 1] ?? null;
  const value = useMemo<ConversationDataMaintenanceContextValue>(
    () => ({ task, tasks, audit, repair, cancel, refresh: async () => { await refresh(); } }),
    [audit, cancel, refresh, repair, task, tasks],
  );

  return (
    <ConversationDataMaintenanceContext.Provider value={value}>
      {children}
    </ConversationDataMaintenanceContext.Provider>
  );
}

export function useConversationDataMaintenance() {
  const context = useContext(ConversationDataMaintenanceContext);
  if (!context) {
    throw new Error("useConversationDataMaintenance must be used inside ConversationDataMaintenanceProvider");
  }
  return context;
}

function mergeMaintenanceTask(
  current: ConversationDataMaintenanceTaskSnapshot[],
  incoming: ConversationDataMaintenanceTaskSnapshot | ConversationDataMaintenanceTaskSnapshot[],
) {
  const snapshots = Array.isArray(incoming) ? incoming : [incoming];
  const byId = new Map(current.map((task) => [task.id, task]));
  for (const snapshot of snapshots) {
    byId.set(snapshot.id, snapshot);
  }
  return [...byId.values()].sort((left, right) => left.started_at.localeCompare(right.started_at));
}
