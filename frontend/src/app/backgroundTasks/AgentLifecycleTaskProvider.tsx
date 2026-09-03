import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelAgentLifecycleTask,
  listAgentLifecycleTasks,
  subscribeAgentLifecycleTasks,
  type AgentLifecycleTaskSnapshot,
} from "../../services/agentRuntime";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

const TERMINAL_TASK_LIMIT = 100;

export interface AgentLifecycleTaskContextValue {
  tasks: AgentLifecycleTaskSnapshot[];
  cancelTask: (taskId: string) => Promise<AgentLifecycleTaskSnapshot>;
  getTask: (taskId: string) => AgentLifecycleTaskSnapshot | undefined;
  refresh: () => Promise<void>;
  mergeSnapshot: (snapshot: AgentLifecycleTaskSnapshot) => void;
}

export function agentLifecycleQueryOptions(scope: QueryScope) {
  return queryOptions<AgentLifecycleTaskSnapshot[]>({
    queryKey: taskKeys.resource(scope, "agent-lifecycle"),
    queryFn: listAgentLifecycleTasks,
    structuralSharing: (oldData, newData) => {
      const current =
        (oldData as AgentLifecycleTaskSnapshot[] | undefined) ?? [];
      const incoming =
        (newData as AgentLifecycleTaskSnapshot[] | undefined) ?? [];
      return mergeAgentLifecycleTaskSnapshots(current, incoming);
    },
    staleTime: 1000,
  });
}

export function AgentLifecycleTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "agent-lifecycle");

  useQuery({
    ...agentLifecycleQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const tasks = query.state.data;
      const isRunning = tasks && tasks.some(isActiveAgentLifecycleTask);
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<AgentLifecycleTaskSnapshot[], AgentLifecycleTaskSnapshot>
        merge={(current, snapshot) =>
          mergeAgentLifecycleTaskSnapshots(current ?? [], [snapshot])
        }
        queryKey={queryKey}
        subscribe={subscribeAgentLifecycleTasks}
      />
      {children ?? null}
    </>
  );
}

export function useAgentLifecycleTasks(): AgentLifecycleTaskContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "agent-lifecycle");

  const query = useQuery({
    ...agentLifecycleQueryOptions(activeScope),
  });

  const tasks = query.data ?? [];

  const cancelTask = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelAgentLifecycleTask(taskId);
      queryClient.setQueryData<AgentLifecycleTaskSnapshot[]>(
        queryKey,
        (current) =>
          mergeAgentLifecycleTaskSnapshots(current ?? [], [snapshot]),
      );
      return snapshot;
    },
    [queryClient, queryKey],
  );

  const getTask = useCallback(
    (taskId: string) => tasks.find((task) => task.id === taskId),
    [tasks],
  );

  const mergeSnapshot = useCallback(
    (snapshot: AgentLifecycleTaskSnapshot) => {
      queryClient.setQueryData<AgentLifecycleTaskSnapshot[]>(
        queryKey,
        (current) =>
          mergeAgentLifecycleTaskSnapshots(current ?? [], [snapshot]),
      );
    },
    [queryClient, queryKey],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  return {
    cancelTask,
    getTask,
    mergeSnapshot,
    refresh,
    tasks,
  };
}

export function useOptionalAgentLifecycleTasks(): AgentLifecycleTaskContextValue | null {
  try {
    return useAgentLifecycleTasks();
  } catch {
    return null;
  }
}

export function isActiveAgentLifecycleTask(task: AgentLifecycleTaskSnapshot) {
  return (
    task.state === "queued" ||
    task.state === "running" ||
    task.state === "cancelling"
  );
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
    .sort(
      (left, right) =>
        right.updatedAt.localeCompare(left.updatedAt) ||
        right.id.localeCompare(left.id),
    )
    .slice(0, TERMINAL_TASK_LIMIT);
  return [...active, ...terminal].sort(
    (left, right) =>
      left.createdAt.localeCompare(right.createdAt) ||
      left.id.localeCompare(right.id),
  );
}

function shouldReplaceSnapshot(
  existing: AgentLifecycleTaskSnapshot,
  incoming: AgentLifecycleTaskSnapshot,
) {
  const timestampOrder = incoming.updatedAt.localeCompare(existing.updatedAt);
  if (timestampOrder !== 0) return timestampOrder > 0;
  if (
    isActiveAgentLifecycleTask(existing) !==
    isActiveAgentLifecycleTask(incoming)
  ) {
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
    cancelling: 97,
    succeeded: 100,
    failed: 100,
    cancelled: 100,
  };
  return phases[task.phase];
}
