import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listTeamRunTasks,
  subscribeTeamRunTasks,
} from "../../services/teamWorkflow";
import type { TeamRuntimeTaskSnapshot } from "../../types/team";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";
import { isTerminalStatus } from "./taskMergeUtils";

export interface TeamTaskContextValue {
  tasks: TeamRuntimeTaskSnapshot[];
  refresh: () => Promise<void>;
  getTask: (taskId: string) => TeamRuntimeTaskSnapshot | undefined;
}

export function teamRunQueryOptions(scope: QueryScope) {
  return queryOptions<TeamRuntimeTaskSnapshot[]>({
    queryKey: taskKeys.resource(scope, "team-run"),
    queryFn: listTeamRunTasks,
    networkMode: "always",
    structuralSharing: (oldData, newData) => {
      const current = (oldData as TeamRuntimeTaskSnapshot[] | undefined) ?? [];
      const incoming = (newData as TeamRuntimeTaskSnapshot[] | undefined) ?? [];
      return mergeTeamRunTasks(current, incoming);
    },
    staleTime: 1000,
  });
}

export function TeamTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "team-run");

  useQuery({
    ...teamRunQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const tasks = query.state.data;
      const isRunning =
        tasks &&
        tasks.some(
          (task) =>
            task.state === "Pending" ||
            task.state === "Running" ||
            task.state === "Cancelling",
        );
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<TeamRuntimeTaskSnapshot[], TeamRuntimeTaskSnapshot>
        merge={(current, task) => mergeTeamRunTasks(current ?? [], task)}
        queryKey={queryKey}
        subscribe={subscribeTeamRunTasks}
      />
      {children ?? null}
    </>
  );
}

export function useTeamTasks(): TeamTaskContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "team-run");

  const query = useQuery({
    ...teamRunQueryOptions(activeScope),
  });

  const tasks = query.data ?? [];

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  const getTask = useCallback(
    (taskId: string) => tasks.find((task) => task.task_id === taskId),
    [tasks],
  );

  return {
    getTask,
    refresh,
    tasks,
  };
}

export function useOptionalTeamTasks(): TeamTaskContextValue | null {
  try {
    return useTeamTasks();
  } catch {
    return null;
  }
}

export function mergeTeamRunTasks(
  current: TeamRuntimeTaskSnapshot[],
  incoming: TeamRuntimeTaskSnapshot | TeamRuntimeTaskSnapshot[],
): TeamRuntimeTaskSnapshot[] {
  const snapshots = Array.isArray(incoming) ? incoming : [incoming];
  const byId = new Map(current.map((task) => [task.task_id, task]));
  for (const task of snapshots) {
    const existing = byId.get(task.task_id);
    if (existing) {
      const existingIsTerminal = isTerminalStatus(existing.state);
      const incomingIsTerminal = isTerminalStatus(task.state);
      if (existingIsTerminal && !incomingIsTerminal) {
        // 保留终态状态，防止旧轮询将完成状态覆写回 running
        byId.set(task.task_id, {
          ...task,
          state: existing.state,
          finished_at: existing.finished_at ?? task.finished_at,
          error: existing.error ?? task.error,
        });
        continue;
      }
    }
    byId.set(task.task_id, task);
  }
  return [...byId.values()];
}
