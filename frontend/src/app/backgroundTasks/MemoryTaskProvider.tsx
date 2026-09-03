import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  cancelMemoryPublicTask,
  listMemoryPublicTasks,
  retryMemoryPublicTask,
  subscribeMemoryTasks,
} from "../../services/memory";
import type { MemoryTaskView } from "../../types/memory";

interface MemoryTaskContextValue {
  cancelTask: (taskId: string) => Promise<MemoryTaskView>;
  refresh: () => Promise<void>;
  retryTask: (taskId: string) => Promise<MemoryTaskView>;
  task: MemoryTaskView | null;
  tasks: MemoryTaskView[];
  publicTasks: MemoryTaskView[];
}

const MemoryTaskContext = createContext<MemoryTaskContextValue | null>(null);

export function MemoryTaskProvider({ children }: { children: ReactNode }) {
  const [tasks, setTasks] = useState<MemoryTaskView[]>([]);

  const refresh = useCallback(async () => {
    setTasks(await listMemoryPublicTasks(true));
  }, []);

  useEffect(() => {
    let cancelled = false;
    const refreshTasks = () => void listMemoryPublicTasks(true).then((nextTasks) => {
      if (!cancelled) setTasks(nextTasks);
    }).catch(() => undefined);
    void refreshTasks();
    const interval = window.setInterval(refreshTasks, 1000);
    let unlisten: (() => void) | undefined;
    void subscribeMemoryTasks(refreshTasks).then((cleanup) => {
      if (cancelled) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      cancelled = true;
      window.clearInterval(interval);
      unlisten?.();
    };
  }, []);

  const cancelTask = useCallback(async (taskId: string) => {
    const task = await cancelMemoryPublicTask(taskId);
    setTasks((current) => upsertTask(current, task));
    return task;
  }, []);

  const retryTask = useCallback(async (taskId: string) => {
    const task = await retryMemoryPublicTask(taskId);
    setTasks((current) => upsertTask(current, task));
    return task;
  }, []);

  const value = useMemo(() => ({
    cancelTask,
    refresh,
    retryTask,
    task: tasks[tasks.length - 1] ?? null,
    tasks,
    publicTasks: tasks,
  }), [cancelTask, refresh, retryTask, tasks]);

  return <MemoryTaskContext.Provider value={value}>{children}</MemoryTaskContext.Provider>;
}

export function useMemoryTasks() {
  const context = useContext(MemoryTaskContext);
  if (!context) throw new Error("useMemoryTasks must be used inside MemoryTaskProvider");
  return context;
}

function upsertTask(tasks: MemoryTaskView[], nextTask: MemoryTaskView) {
  return [...tasks.filter((task) => task.id !== nextTask.id), nextTask].sort((left, right) => left.started_at.localeCompare(right.started_at));
}
