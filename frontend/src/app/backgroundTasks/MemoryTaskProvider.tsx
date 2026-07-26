import { listen } from "@tauri-apps/api/event";
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
import {
  cancelMemoryTask,
  getMemoryDreamStatus,
  listMemoryTasks,
  startMemoryTask,
} from "../../services/memory";
import type {
  MemoryDreamPreview,
  MemoryRecallPreviewParams,
  MemoryScope,
  MemoryTaskSnapshot,
} from "../../types/memory";

const MEMORY_TASK_UPDATED_EVENT = "memory-task-updated";
const CONVERSATION_SYNC_TASK_UPDATED_EVENT = "conversation-sync-task-updated";
const POLL_INTERVAL_MS = 1000;
const STARTUP_CHECK_DELAY_MS = 5000;
const AUTO_CHECK_INTERVAL_MS = 15 * 60 * 1000;

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
  const [tasks, setTasks] = useState<MemoryTaskSnapshot[]>([]);
  const [autoDreamStatus, setAutoDreamStatus] = useState<MemoryDreamPreview | null>(null);
  const autoStartAttemptRef = useRef<string | null>(null);

  const mergeTask = useCallback((snapshot: MemoryTaskSnapshot) => {
    setTasks((current) => {
      const next = current.filter((task) => task.id !== snapshot.id);
      next.push(snapshot);
      next.sort((left, right) => left.started_at.localeCompare(right.started_at));
      return next;
    });
  }, []);

  const refresh = useCallback(async () => {
    const snapshots = await listMemoryTasks();
    setTasks(snapshots);
  }, []);

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
    mergeTask(snapshot);
    return snapshot;
  }, [mergeTask, refreshAutoDreamStatus]);

  useEffect(() => {
    void Promise.all([refresh(), refreshAutoDreamStatus()]).catch(() => {});
  }, [refresh, refreshAutoDreamStatus]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<MemoryTaskSnapshot>(MEMORY_TASK_UPDATED_EVENT, (event) => {
      if (cancelled) return;
      mergeTask(event.payload);
      if (event.payload.status !== "running") {
        void refreshAutoDreamStatus(event.payload.scope).catch(() => {});
      }
    })
      .then((removeListener) => {
        if (cancelled) removeListener();
        else unlisten = removeListener;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [mergeTask, refreshAutoDreamStatus]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<{ status?: string }>(CONVERSATION_SYNC_TASK_UPDATED_EVENT, (event) => {
      if (!cancelled && event.payload.status === "completed") {
        void maybeStartAutoDream().catch(() => {});
      }
    })
      .then((removeListener) => {
        if (cancelled) removeListener();
        else unlisten = removeListener;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [maybeStartAutoDream]);

  const runningKey = tasks
    .filter((task) => task.status === "running")
    .map((task) => task.id)
    .join(":");
  useEffect(() => {
    if (!runningKey) return;
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
  }, [refresh, runningKey]);

  useEffect(() => {
    const startupTimer = window.setTimeout(() => {
      void maybeStartAutoDream().catch(() => {});
    }, STARTUP_CHECK_DELAY_MS);
    const interval = window.setInterval(() => {
      void maybeStartAutoDream().catch(() => {});
    }, AUTO_CHECK_INTERVAL_MS);
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
    mergeTask(snapshot);
    return snapshot;
  }, [mergeTask]);

  const cancelTask = useCallback(async (taskId: string) => {
    const snapshot = await cancelMemoryTask(taskId);
    mergeTask(snapshot);
    return snapshot;
  }, [mergeTask]);

  const startRecall = useCallback(async (params: MemoryRecallPreviewParams & { synthesize?: boolean; dryRun?: boolean }) => {
    const snapshot = await startMemoryTask({
      kind: params.mode === "full" ? "full_organize" : "deep_recall",
      scope: params.scope,
      trigger: "manual",
      dry_run: params.dryRun,
      recall: params,
      synthesize: params.synthesize ?? false,
    });
    mergeTask(snapshot);
    return snapshot;
  }, [mergeTask]);

  const task = tasks[tasks.length - 1] ?? null;
  const value = useMemo<MemoryTaskContextValue>(() => ({
    autoDreamStatus,
    cancelTask,
    refresh,
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
