import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import {
  cancelAgentLifecycleTask,
  listAgentLifecycleTasks,
  subscribeAgentLifecycleTasks,
  type AgentLifecycleTaskSnapshot,
} from "../../services/agentRuntime";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

const TERMINAL_TASK_LIMIT = 100;

interface AgentLifecycleRuntimeEvent {
  snapshot: AgentLifecycleTaskSnapshot;
}

interface AgentLifecycleTaskContextValue {
  tasks: AgentLifecycleTaskSnapshot[];
  cancelTask: (taskId: string) => Promise<AgentLifecycleTaskSnapshot>;
  getTask: (taskId: string) => AgentLifecycleTaskSnapshot | undefined;
  refresh: () => Promise<void>;
  mergeSnapshot: (snapshot: AgentLifecycleTaskSnapshot) => void;
}

const AgentLifecycleTaskContext = createContext<AgentLifecycleTaskContextValue | null>(null);

export function AgentLifecycleTaskProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<AgentLifecycleTaskSnapshot[], AgentLifecycleRuntimeEvent>>(
    () => ({
      initialState: [],
      isRunning: (tasks) => tasks.some(isActiveAgentLifecycleTask),
      merge: (current, incoming) => mergeAgentLifecycleTaskSnapshots(
        current,
        "snapshot" in incoming ? [incoming.snapshot] : incoming,
      ),
      refresh: listAgentLifecycleTasks,
      subscribe: (listener) => subscribeAgentLifecycleTasks((snapshot) => listener({ snapshot })),
    }),
    [],
  );
  const { merge, refresh, state: tasks } = useBackgroundTaskRuntime(adapter);

  const cancelTask = useCallback(async (taskId: string) => {
    const snapshot = await cancelAgentLifecycleTask(taskId);
    merge({ snapshot });
    return snapshot;
  }, [merge]);

  const getTask = useCallback(
    (taskId: string) => tasks.find((task) => task.id === taskId),
    [tasks],
  );

  const value = useMemo<AgentLifecycleTaskContextValue>(() => ({
    tasks,
    cancelTask,
    getTask,
    refresh: async () => {
      await refresh();
    },
    mergeSnapshot: (snapshot) => merge({ snapshot }),
  }), [cancelTask, getTask, merge, refresh, tasks]);

  return (
    <AgentLifecycleTaskContext.Provider value={value}>
      {children}
    </AgentLifecycleTaskContext.Provider>
  );
}

export function useAgentLifecycleTasks() {
  const context = useContext(AgentLifecycleTaskContext);
  if (!context) {
    throw new Error("useAgentLifecycleTasks must be used inside AgentLifecycleTaskProvider");
  }
  return context;
}

export function useOptionalAgentLifecycleTasks() {
  return useContext(AgentLifecycleTaskContext);
}

export function isActiveAgentLifecycleTask(task: AgentLifecycleTaskSnapshot) {
  return task.state === "queued" || task.state === "running";
}

export function mergeAgentLifecycleTaskSnapshots(
  current: AgentLifecycleTaskSnapshot[],
  incoming: AgentLifecycleTaskSnapshot[],
) {
  if (incoming.length === 0) return current;
  const byId = new Map(current.map((task) => [task.id, task]));
  let changed = false;
  for (const snapshot of incoming) {
    const existing = byId.get(snapshot.id);
    if (!existing || shouldReplaceSnapshot(existing, snapshot)) {
      byId.set(snapshot.id, snapshot);
      changed = true;
    }
  }
  if (!changed) return current;
  const active = [...byId.values()].filter(isActiveAgentLifecycleTask);
  const terminal = [...byId.values()]
    .filter((task) => !isActiveAgentLifecycleTask(task))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt) || right.id.localeCompare(left.id))
    .slice(0, TERMINAL_TASK_LIMIT);
  return [...active, ...terminal].sort((left, right) => (
    left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id)
  ));
}

function shouldReplaceSnapshot(
  existing: AgentLifecycleTaskSnapshot,
  incoming: AgentLifecycleTaskSnapshot,
) {
  const timestampOrder = incoming.updatedAt.localeCompare(existing.updatedAt);
  if (timestampOrder !== 0) return timestampOrder > 0;
  if (isActiveAgentLifecycleTask(existing) !== isActiveAgentLifecycleTask(incoming)) {
    return !isActiveAgentLifecycleTask(incoming);
  }
  return lifecycleProgress(incoming) >= lifecycleProgress(existing);
}

function lifecycleProgress(task: AgentLifecycleTaskSnapshot) {
  const phases: Record<AgentLifecycleTaskSnapshot["phase"], number> = {
    queued: 0,
    preparing: 10,
    probing_runtime: 20,
    downloading: 35,
    installing: 50,
    validating_integrity: 60,
    validating_layout: 65,
    probing_protocol: 75,
    activating_database: 85,
    reloading_registry: 90,
    cleaning_up: 95,
    succeeded: 100,
    failed: 100,
    cancelled: 100,
  };
  return phases[task.phase];
}
