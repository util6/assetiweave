import { listen } from "@tauri-apps/api/event";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  cancelAiExecutionTask,
  listAiExecutionTasks,
  startConversationCardTranslation,
  type AiExecutionTaskSnapshot,
  type ConversationCardTranslationRequest,
} from "../../services/cardTranslation";

export const AI_EXECUTION_TASK_UPDATED_EVENT = "ai-execution://task-updated";
const POLL_INTERVAL_MS = 1000;
const TERMINAL_TASK_LIMIT = 100;

interface AiExecutionTaskContextValue {
  tasks: AiExecutionTaskSnapshot[];
  startTranslation: (
    request: ConversationCardTranslationRequest,
  ) => Promise<AiExecutionTaskSnapshot>;
  cancelTask: (taskId: string) => Promise<AiExecutionTaskSnapshot>;
  getTask: (taskId: string) => AiExecutionTaskSnapshot | undefined;
  refresh: () => Promise<void>;
}

const AiExecutionTaskContext = createContext<AiExecutionTaskContextValue | null>(null);

export function AiExecutionTaskProvider({ children }: { children: ReactNode }) {
  const [tasks, setTasks] = useState<AiExecutionTaskSnapshot[]>([]);

  const mergeSnapshots = useCallback((snapshots: AiExecutionTaskSnapshot[]) => {
    setTasks((current) => mergeAiExecutionTaskSnapshots(current, snapshots));
  }, []);

  const mergeSnapshot = useCallback((snapshot: AiExecutionTaskSnapshot) => {
    mergeSnapshots([snapshot]);
  }, [mergeSnapshots]);

  const refresh = useCallback(async () => {
    mergeSnapshots(await listAiExecutionTasks());
  }, [mergeSnapshots]);

  useEffect(() => {
    void refresh().catch(() => {});
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<AiExecutionTaskSnapshot>(AI_EXECUTION_TASK_UPDATED_EVENT, (event) => {
      if (!disposed) mergeSnapshot(event.payload);
    })
      .then((removeListener) => {
        if (disposed) removeListener();
        else unlisten = removeListener;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [mergeSnapshot]);

  const activeTaskKey = tasks
    .filter(isActiveAiExecutionTask)
    .map((task) => `${task.id}:${task.updated_at}`)
    .join("|");
  useEffect(() => {
    if (!activeTaskKey) return;
    let polling = false;
    const interval = window.setInterval(() => {
      if (polling) return;
      polling = true;
      void refresh()
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, POLL_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [activeTaskKey, refresh]);

  const startTranslation = useCallback(async (request: ConversationCardTranslationRequest) => {
    const snapshot = await startConversationCardTranslation(request);
    mergeSnapshot(snapshot);
    return snapshot;
  }, [mergeSnapshot]);

  const cancelTask = useCallback(async (taskId: string) => {
    const snapshot = await cancelAiExecutionTask(taskId);
    mergeSnapshot(snapshot);
    return snapshot;
  }, [mergeSnapshot]);

  const getTask = useCallback(
    (taskId: string) => tasks.find((task) => task.id === taskId),
    [tasks],
  );

  const value = useMemo<AiExecutionTaskContextValue>(() => ({
    tasks,
    startTranslation,
    cancelTask,
    getTask,
    refresh,
  }), [cancelTask, getTask, refresh, startTranslation, tasks]);

  return (
    <AiExecutionTaskContext.Provider value={value}>
      {children}
    </AiExecutionTaskContext.Provider>
  );
}

export function useAiExecutionTasks() {
  const context = useContext(AiExecutionTaskContext);
  if (!context) {
    throw new Error("useAiExecutionTasks must be used inside AiExecutionTaskProvider");
  }
  return context;
}

export function useOptionalAiExecutionTasks() {
  return useContext(AiExecutionTaskContext);
}

export function isActiveAiExecutionTask(task: AiExecutionTaskSnapshot) {
  return task.state === "queued" || task.state === "running";
}

export function mergeAiExecutionTaskSnapshots(
  current: AiExecutionTaskSnapshot[],
  incoming: AiExecutionTaskSnapshot[],
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
  return retainRecentAiExecutionTasks([...byId.values()]).sort((left, right) => (
    left.created_at.localeCompare(right.created_at) || left.id.localeCompare(right.id)
  ));
}

function retainRecentAiExecutionTasks(tasks: AiExecutionTaskSnapshot[]) {
  const active = tasks.filter(isActiveAiExecutionTask);
  const terminal = tasks
    .filter((task) => !isActiveAiExecutionTask(task))
    .sort((left, right) => (
      right.updated_at.localeCompare(left.updated_at) || right.id.localeCompare(left.id)
    ))
    .slice(0, TERMINAL_TASK_LIMIT);
  return [...active, ...terminal];
}

function shouldReplaceSnapshot(
  existing: AiExecutionTaskSnapshot,
  incoming: AiExecutionTaskSnapshot,
) {
  const timestampOrder = incoming.updated_at.localeCompare(existing.updated_at);
  if (timestampOrder !== 0) return timestampOrder > 0;
  return snapshotProgress(incoming) > snapshotProgress(existing);
}

function snapshotProgress(task: AiExecutionTaskSnapshot) {
  const stateProgress: Record<AiExecutionTaskSnapshot["state"], number> = {
    queued: 0,
    running: 10,
    succeeded: 100,
    failed: 100,
    cancelled: 100,
  };
  const phaseProgress: Record<AiExecutionTaskSnapshot["phase"], number> = {
    queued: 0,
    resolving: 1,
    spawning: 2,
    initializing: 3,
    creating_session: 4,
    configuring: 5,
    prompting: 6,
    cancelling: 7,
    closing: 8,
    cleaning_up: 9,
  };
  return stateProgress[task.state] + phaseProgress[task.phase];
}
