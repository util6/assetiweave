import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelAiExecutionTask,
  listAiExecutionTasks,
  startConversationCardTranslation,
  subscribeAiExecutionTasks,
  type AiExecutionTaskSnapshot,
  type ConversationCardTranslationRequest,
} from "../../services/cardTranslation";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

const TERMINAL_TASK_LIMIT = 100;

export interface AiExecutionTaskContextValue {
  tasks: AiExecutionTaskSnapshot[];
  startTranslation: (
    request: ConversationCardTranslationRequest,
  ) => Promise<AiExecutionTaskSnapshot>;
  cancelTask: (taskId: string) => Promise<AiExecutionTaskSnapshot>;
  getTask: (taskId: string) => AiExecutionTaskSnapshot | undefined;
  refresh: () => Promise<void>;
}

export function aiExecutionQueryOptions(scope: QueryScope) {
  return queryOptions<AiExecutionTaskSnapshot[]>({
    queryKey: taskKeys.resource(scope, "ai-execution"),
    queryFn: listAiExecutionTasks,
    structuralSharing: (oldData, newData) => {
      const current =
        (oldData as AiExecutionTaskSnapshot[] | undefined) ?? [];
      const incoming =
        (newData as AiExecutionTaskSnapshot[] | undefined) ?? [];
      return mergeAiExecutionTaskSnapshots(current, incoming);
    },
    staleTime: 1000,
  });
}

export function AiExecutionTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "ai-execution");

  useQuery({
    ...aiExecutionQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const tasks = query.state.data;
      const isRunning = tasks && tasks.some(isActiveAiExecutionTask);
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<AiExecutionTaskSnapshot[], AiExecutionTaskSnapshot>
        merge={(current, snapshot) =>
          mergeAiExecutionTaskSnapshots(current ?? [], [snapshot])
        }
        queryKey={queryKey}
        subscribe={subscribeAiExecutionTasks}
      />
      {children ?? null}
    </>
  );
}

export function useAiExecutionTasks(): AiExecutionTaskContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "ai-execution");

  const query = useQuery({
    ...aiExecutionQueryOptions(activeScope),
  });

  const tasks = query.data ?? [];

  const startTranslation = useCallback(
    async (request: ConversationCardTranslationRequest) => {
      const snapshot = await startConversationCardTranslation(request);
      queryClient.setQueryData<AiExecutionTaskSnapshot[]>(queryKey, (current) =>
        mergeAiExecutionTaskSnapshots(current ?? [], [snapshot]),
      );
      return snapshot;
    },
    [queryClient, queryKey],
  );

  const cancelTask = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelAiExecutionTask(taskId);
      queryClient.setQueryData<AiExecutionTaskSnapshot[]>(queryKey, (current) =>
        mergeAiExecutionTaskSnapshots(current ?? [], [snapshot]),
      );
      return snapshot;
    },
    [queryClient, queryKey],
  );

  const getTask = useCallback(
    (taskId: string) => tasks.find((task) => task.id === taskId),
    [tasks],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  return {
    cancelTask,
    getTask,
    refresh,
    startTranslation,
    tasks,
  };
}

export function useOptionalAiExecutionTasks(): AiExecutionTaskContextValue | null {
  try {
    return useAiExecutionTasks();
  } catch {
    return null;
  }
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
  return retainRecentAiExecutionTasks([...byId.values()]).sort(
    (left, right) =>
      left.created_at.localeCompare(right.created_at) ||
      left.id.localeCompare(right.id),
  );
}

function retainRecentAiExecutionTasks(tasks: AiExecutionTaskSnapshot[]) {
  const active = tasks.filter(isActiveAiExecutionTask);
  const terminal = tasks
    .filter((task) => !isActiveAiExecutionTask(task))
    .sort(
      (left, right) =>
        right.updated_at.localeCompare(left.updated_at) ||
        right.id.localeCompare(left.id),
    )
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
