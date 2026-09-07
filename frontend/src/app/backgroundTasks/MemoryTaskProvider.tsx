import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelMemoryPublicTask,
  listMemoryPublicTasks,
  retryMemoryPublicTask,
  subscribeMemoryTasks,
} from "../../services/memory";
import type { MemoryTaskView } from "../../types/memory";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

export function memoryTasksQueryOptions(scope: QueryScope) {
  return queryOptions<MemoryTaskView[]>({
    queryKey: taskKeys.resource(scope, "memory"),
    queryFn: () => listMemoryPublicTasks(true),
    staleTime: 1000,
  });
}

export interface MemoryTaskContextValue {
  cancelTask: (taskId: string) => Promise<MemoryTaskView>;
  refresh: () => Promise<void>;
  retryTask: (taskId: string) => Promise<MemoryTaskView>;
  task: MemoryTaskView | null;
  tasks: MemoryTaskView[];
  publicTasks: MemoryTaskView[];
}

export function MemoryTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "memory");

  useQuery({
    ...memoryTasksQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const data = query.state.data;
      const isRunning = data?.some(
        (t) =>
          t.status === "running" ||
          t.status === "pending" ||
          t.status === "cancelling",
      );
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<MemoryTaskView[], unknown>
        queryKey={queryKey}
        subscribe={(listener) => subscribeMemoryTasks(() => listener({}))}
      />
      {children ?? null}
    </>
  );
}

export function useMemoryTasks(): MemoryTaskContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "memory");

  const query = useQuery({
    ...memoryTasksQueryOptions(activeScope),
  });

  const tasks = query.data ?? [];

  const cancelTask = useCallback(
    async (taskId: string) => {
      const updated = await cancelMemoryPublicTask(taskId);
      queryClient.setQueryData<MemoryTaskView[]>(queryKey, (current) =>
        upsertTask(current ?? [], updated),
      );
      return updated;
    },
    [queryClient, queryKey],
  );

  const retryTask = useCallback(
    async (taskId: string) => {
      const updated = await retryMemoryPublicTask(taskId);
      queryClient.setQueryData<MemoryTaskView[]>(queryKey, (current) =>
        upsertTask(current ?? [], updated),
      );
      return updated;
    },
    [queryClient, queryKey],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  return {
    cancelTask,
    refresh,
    retryTask,
    task: tasks[tasks.length - 1] ?? null,
    tasks,
    publicTasks: tasks,
  };
}

function upsertTask(tasks: MemoryTaskView[], nextTask: MemoryTaskView) {
  return [...tasks.filter((task) => task.id !== nextTask.id), nextTask].sort(
    (left, right) => left.started_at.localeCompare(right.started_at),
  );
}
