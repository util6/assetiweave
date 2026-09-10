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
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelPublicTask,
  clearTerminalTasks,
  listPublicTasks,
  retryPublicTask,
  subscribeTaskUpdated,
} from "../../services/taskCenterService";
import type { TaskView } from "../../types/taskCenter";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { useAppSettings } from "../../store/settings/useAppSettings";

export interface TaskNotificationItem {
  id: string;
  task: TaskView;
  title: string;
  message: string;
  isFailure: boolean;
  durationMs: number;
  createdAt: number;
}

export interface TaskCenterContextValue {
  tasks: TaskView[];
  activeTasks: TaskView[];
  terminalTasks: TaskView[];
  activeCount: number;
  failureCount: number;
  selectedTaskId: string | null;
  setSelectedTaskId: (id: string | null) => void;
  selectedTask: TaskView | null;
  cancelTask: (taskId: string) => Promise<TaskView>;
  retryTask: (taskId: string) => Promise<TaskView>;
  clearTerminal: (taskId?: string) => Promise<number>;
  refresh: () => Promise<void>;
  currentNotification: TaskNotificationItem | null;
  dismissNotification: (id: string) => void;
}

const TaskCenterContext = createContext<TaskCenterContextValue | null>(null);

function isActiveState(state: string) {
  return state === "pending" || state === "running" || state === "cancelling";
}

function isTerminalState(state: string) {
  return state === "succeeded" || state === "failed" || state === "canceled";
}

export function TaskCenterProvider({ children }: { children: ReactNode }) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "task_center_all");
  const { settings } = useAppSettings();

  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [notificationQueue, setNotificationQueue] = useState<
    TaskNotificationItem[]
  >([]);

  // Track previous task states to detect transitions into terminal
  const prevTasksRef = useRef<Map<string, TaskView>>(new Map());

  const query = useQuery<TaskView[]>({
    queryKey,
    queryFn: () => listPublicTasks(),
    refetchInterval: (q) => {
      const data = q.state.data;
      const hasActive = data?.some((t) => isActiveState(t.state));
      return hasActive ? 1500 : 8000;
    },
    refetchIntervalInBackground: true,
  });

  const rawTasks = query.data ?? [];

  // Sort tasks: active tasks first, then by started_at descending
  const tasks = useMemo(() => {
    return [...rawTasks].sort((a, b) => {
      const aActive = isActiveState(a.state);
      const bActive = isActiveState(b.state);
      if (aActive && !bActive) return -1;
      if (!aActive && bActive) return 1;
      const aTime = a.started_at ?? a.updated_at ?? "";
      const bTime = b.started_at ?? b.updated_at ?? "";
      return bTime.localeCompare(aTime);
    });
  }, [rawTasks]);

  const activeTasks = useMemo(
    () => tasks.filter((t) => isActiveState(t.state)),
    [tasks],
  );

  const terminalTasks = useMemo(
    () => tasks.filter((t) => isTerminalState(t.state)),
    [tasks],
  );

  const activeCount = activeTasks.length;

  const failureCount = useMemo(() => {
    return terminalTasks.filter(
      (t) =>
        t.state === "failed" ||
        t.outcome === "failure" ||
        t.outcome === "partial_success",
    ).length;
  }, [terminalTasks]);

  // Keep selected task valid
  const selectedTask = useMemo(() => {
    if (selectedTaskId) {
      const found = tasks.find((t) => t.id === selectedTaskId);
      if (found) return found;
    }
    return tasks[0] ?? null;
  }, [tasks, selectedTaskId]);

  const enqueueNotification = useCallback(
    (task: TaskView) => {
      if (settings?.showTaskNotifications === false) {
        return;
      }
      const isPartial = task.outcome === "partial_success";
      const isFailed =
        task.state === "failed" ||
        task.outcome === "failure" ||
        task.failures.length > 0;
      const isFailure = isFailed || isPartial;
      const durationMs = isFailure ? 8000 : 4000;

      let title = task.title;
      if (isPartial) {
        title += " (部分成功)";
      } else if (isFailed) {
        title += " (失败)";
      } else {
        title += " (已完成)";
      }

      const message =
        task.error_summary ||
        task.result_summary ||
        task.failures[0]?.message ||
        (isFailure ? "任务执行遇到异常" : "任务已顺利完成");

      const item: TaskNotificationItem = {
        id: `${task.id}-${task.revision}`,
        task,
        title,
        message,
        isFailure,
        durationMs,
        createdAt: Date.now(),
      };

      setNotificationQueue((prev) => {
        // Drop duplicates
        const filtered = prev.filter((n) => n.task.id !== task.id);
        // Max 3 items in queue
        const next = [...filtered, item];
        if (next.length > 3) {
          return next.slice(next.length - 3);
        }
        return next;
      });
    },
    [settings?.showTaskNotifications],
  );

  const dismissNotification = useCallback((id: string) => {
    setNotificationQueue((prev) => prev.filter((n) => n.id !== id));
  }, []);

  // Clear notifications when user disables setting
  useEffect(() => {
    if (settings?.showTaskNotifications === false) {
      setNotificationQueue([]);
    }
  }, [settings?.showTaskNotifications]);

  // Handle task snapshot updates
  const handleTaskSnapshot = useCallback(
    (snapshot: TaskView) => {
      const prev = prevTasksRef.current.get(snapshot.id);
      const wasActive = prev ? isActiveState(prev.state) : false;
      const nowTerminal = isTerminalState(snapshot.state);

      if (wasActive && nowTerminal) {
        enqueueNotification(snapshot);
      }

      prevTasksRef.current.set(snapshot.id, snapshot);

      queryClient.setQueryData<TaskView[]>(queryKey, (current) => {
        const existing = current ?? [];
        const index = existing.findIndex((t) => t.id === snapshot.id);
        if (index >= 0) {
          const updated = [...existing];
          updated[index] = snapshot;
          return updated;
        }
        return [snapshot, ...existing];
      });
    },
    [enqueueNotification, queryClient, queryKey],
  );

  // Subscribe to Tauri events
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;

    subscribeTaskUpdated((task) => {
      if (active) {
        handleTaskSnapshot(task);
      }
    }).then((fn) => {
      if (active) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  }, [handleTaskSnapshot]);

  // Sync prevTasksRef on query data updates
  useEffect(() => {
    for (const t of rawTasks) {
      if (!prevTasksRef.current.has(t.id)) {
        prevTasksRef.current.set(t.id, t);
      }
    }
  }, [rawTasks]);

  const cancelTask = useCallback(
    async (taskId: string) => {
      const updated = await cancelPublicTask(taskId);
      handleTaskSnapshot(updated);
      return updated;
    },
    [handleTaskSnapshot],
  );

  const retryTask = useCallback(
    async (taskId: string) => {
      const updated = await retryPublicTask(taskId);
      handleTaskSnapshot(updated);
      setSelectedTaskId(updated.id);
      return updated;
    },
    [handleTaskSnapshot],
  );

  const clearTerminal = useCallback(
    async (taskId?: string) => {
      const count = await clearTerminalTasks({
        tenant_id: activeScope.tenantId,
        task_id: taskId,
      });
      await queryClient.invalidateQueries({ queryKey });
      return count;
    },
    [activeScope.tenantId, queryClient, queryKey],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  const currentNotification = notificationQueue[0] ?? null;

  const value = useMemo<TaskCenterContextValue>(
    () => ({
      tasks,
      activeTasks,
      terminalTasks,
      activeCount,
      failureCount,
      selectedTaskId,
      setSelectedTaskId,
      selectedTask,
      cancelTask,
      retryTask,
      clearTerminal,
      refresh,
      currentNotification,
      dismissNotification,
    }),
    [
      tasks,
      activeTasks,
      terminalTasks,
      activeCount,
      failureCount,
      selectedTaskId,
      selectedTask,
      cancelTask,
      retryTask,
      clearTerminal,
      refresh,
      currentNotification,
      dismissNotification,
    ],
  );

  return (
    <TaskCenterContext.Provider value={value}>
      {children}
    </TaskCenterContext.Provider>
  );
}

export function useTaskCenter(): TaskCenterContextValue {
  const context = useContext(TaskCenterContext);
  if (!context) {
    throw new Error("useTaskCenter must be used within TaskCenterProvider");
  }
  return context;
}

export function useOptionalTaskCenter(): TaskCenterContextValue | null {
  return useContext(TaskCenterContext);
}
