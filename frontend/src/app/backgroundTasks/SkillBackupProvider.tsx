import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  getSkillBackupTask,
  startSkillBackupTask,
  subscribeSkillBackupTasks,
  type SkillBackupTaskSnapshot,
} from "../../services/catalog";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

export function skillBackupQueryOptions(scope: QueryScope) {
  return queryOptions<SkillBackupTaskSnapshot | null>({
    queryKey: taskKeys.resource(scope, "skill-backup"),
    queryFn: getSkillBackupTask,
    structuralSharing: (oldData, newData) =>
      mergeSkillBackupTask(
        oldData as SkillBackupTaskSnapshot | null | undefined,
        newData as SkillBackupTaskSnapshot | null,
      ),
    staleTime: 1000,
  });
}

export interface SkillBackupContextValue {
  startBackup: (assetIds: string[]) => Promise<SkillBackupTaskSnapshot>;
  task: SkillBackupTaskSnapshot | null;
}

export function SkillBackupProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "skill-backup");

  useQuery({
    ...skillBackupQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) =>
      query.state.data?.status === "running" ? 1000 : 10000,
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<SkillBackupTaskSnapshot | null, SkillBackupTaskSnapshot>
        merge={(current, snapshot) => mergeSkillBackupTask(current, snapshot)}
        queryKey={queryKey}
        subscribe={subscribeSkillBackupTasks}
      />
      {children ?? null}
    </>
  );
}

export function useSkillBackup(): SkillBackupContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "skill-backup");

  const query = useQuery({
    ...skillBackupQueryOptions(activeScope),
  });

  const startBackup = useCallback(
    async (assetIds: string[]) => {
      const snapshot = await startSkillBackupTask(assetIds);
      queryClient.setQueryData<SkillBackupTaskSnapshot | null>(
        queryKey,
        (current) => mergeSkillBackupTask(current, snapshot),
      );
      return snapshot;
    },
    [queryClient, queryKey],
  );

  return {
    startBackup,
    task: query.data ?? null,
  };
}

export function mergeSkillBackupTask(
  current: SkillBackupTaskSnapshot | null | undefined,
  incoming: SkillBackupTaskSnapshot | null,
): SkillBackupTaskSnapshot | null {
  if (!incoming) {
    return current?.status === "running" ? current : null;
  }
  if (!current) {
    return incoming;
  }
  if (current.id === incoming.id) {
    if (current.status !== "running" && incoming.status === "running") {
      return current;
    }
    return incoming;
  }
  return incoming;
}
