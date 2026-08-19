import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { subscribeConversationSyncTasks } from "../../services/conversations";
import {
  cancelMemoryTask,
  getMemoryDreamStatus,
  listMemoryTasks,
  startMemoryTask,
  subscribeMemoryTasks,
} from "../../services/memory";
import type {
  MemoryDreamPreview,
  MemoryRecallPreviewParams,
  MemoryScope,
  MemoryTaskSnapshot,
} from "../../types/memory";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

interface MemoryTaskRuntimeEvent {
  snapshot: MemoryTaskSnapshot;
}

interface MemoryTaskContextValue {
  autoDreamStatus: MemoryDreamPreview | null;
  cancelTask: (taskId: string) => Promise<MemoryTaskSnapshot>;
  refresh: () => Promise<void>;
  refreshAutoDreamStatus: (scope?: MemoryScope) => Promise<MemoryDreamPreview | null>;
  startDream: (params?: { scope?: MemoryScope; dryRun?: boolean }) => Promise<MemoryTaskSnapshot>;
  startRecall: (params: MemoryRecallPreviewParams & { synthesize?: boolean; dryRun?: boolean }) => Promise<MemoryTaskSnapshot>;
  task: MemoryTaskSnapshot | null;
  tasks: MemoryTaskSnapshot[];
}

const MemoryTaskContext = createContext<MemoryTaskContextValue | null>(null);

export function MemoryTaskProvider({ children }: { children: ReactNode }) {
  const [autoDreamStatus, setAutoDreamStatus] = useState<MemoryDreamPreview | null>(null);
  const autoStartAttemptRef = useRef<string | null>(null);
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<MemoryTaskSnapshot[], MemoryTaskRuntimeEvent>>(
    () => ({
      initialState: [],
      isRunning: (state) => state.some((task) => task.status === "running"),
      merge: (current, incoming) => {
        if (isMemoryTaskRuntimeEvent(incoming)) {
          return upsertMemoryTask(current, incoming.snapshot);
        }
        return incoming.reduce(upsertMemoryTask, current);
      },
      refresh: listMemoryTasks,
      subscribe: (listener) => subscribeMemoryTasks((snapshot) => listener({ snapshot })),
    }),
    [],
  );
  const { merge, refresh, state: tasks } = useBackgroundTaskRuntime(adapter);

  const refreshAutoDreamStatus = useCallback(async (scope?: MemoryScope) => {
    const status = await getMemoryDreamStatus(scope);
    setAutoDreamStatus(status);
    return status;
  }, []);

  const maybeStartAutoDream = useCallback(async () => {
    const status = await refreshAutoDreamStatus();
    if (!status?.ready) return null;
    const attemptKey = [
      status.scope_fingerprint,
      status.source_revision_end,
      status.cursor_end?.session_sort_key ?? "none",
      status.cursor_end?.question_offset ?? 0,
    ].join(":");
    if (autoStartAttemptRef.current === attemptKey) return null;
    autoStartAttemptRef.current = attemptKey;
    const snapshot = await startMemoryTask({
      kind: "auto_dream",
      scope: status.scope,
      trigger: "automatic",
    });
    merge({ snapshot });
    return snapshot;
  }, [merge, refreshAutoDreamStatus]);

  useEffect(() => {
    void refreshAutoDreamStatus().catch(() => {});
  }, [refreshAutoDreamStatus]);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    void subscribeConversationSyncTasks((snapshot) => {
      if (!cancelled && snapshot.status === "completed") {
        void maybeStartAutoDream().catch(() => {});
      }
    })
      .then((removeListener) => {
        if (cancelled) removeListener();
        else unsubscribe = removeListener;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [maybeStartAutoDream]);

  useEffect(() => {
    const startupTimer = window.setTimeout(() => {
      void maybeStartAutoDream().catch(() => {});
    }, 5000);
    const interval = window.setInterval(() => {
      void maybeStartAutoDream().catch(() => {});
    }, 15 * 60 * 1000);
    return () => {
      window.clearTimeout(startupTimer);
      window.clearInterval(interval);
    };
  }, [maybeStartAutoDream]);

  const startDream = useCallback(async (params: { scope?: MemoryScope; dryRun?: boolean } = {}) => {
    const snapshot = await startMemoryTask({
      kind: "auto_dream",
      scope: params.scope,
      trigger: "manual",
      dry_run: params.dryRun,
    });
    merge({ snapshot });
    return snapshot;
  }, [merge]);

  const cancelTask = useCallback(async (taskId: string) => {
    const snapshot = await cancelMemoryTask(taskId);
    merge({ snapshot });
    return snapshot;
  }, [merge]);

  const startRecall = useCallback(async (params: MemoryRecallPreviewParams & { synthesize?: boolean; dryRun?: boolean }) => {
    const snapshot = await startMemoryTask({
      kind: params.mode === "full" ? "full_organize" : "deep_recall",
      scope: params.scope,
      trigger: "manual",
      dry_run: params.dryRun,
      recall: params,
      synthesize: params.synthesize ?? false,
    });
    merge({ snapshot });
    return snapshot;
  }, [merge]);

  const task = tasks[tasks.length - 1] ?? null;
  const value = useMemo<MemoryTaskContextValue>(() => ({
    autoDreamStatus,
    cancelTask,
    refresh: async () => {
      await refresh();
    },
    refreshAutoDreamStatus,
    startDream,
    startRecall,
    task,
    tasks,
  }), [autoDreamStatus, cancelTask, refresh, refreshAutoDreamStatus, startDream, startRecall, task, tasks]);

  return <MemoryTaskContext.Provider value={value}>{children}</MemoryTaskContext.Provider>;
}

export function useMemoryTasks() {
  const context = useContext(MemoryTaskContext);
  if (!context) throw new Error("useMemoryTasks must be used inside MemoryTaskProvider");
  return context;
}

function isMemoryTaskRuntimeEvent(
  incoming: MemoryTaskSnapshot[] | MemoryTaskRuntimeEvent,
): incoming is MemoryTaskRuntimeEvent {
  return "snapshot" in incoming;
}

function upsertMemoryTask(tasks: MemoryTaskSnapshot[], snapshot: MemoryTaskSnapshot) {
  return [...tasks.filter((task) => task.id !== snapshot.id), snapshot].sort((left, right) => (
    left.started_at.localeCompare(right.started_at)
  ));
}
