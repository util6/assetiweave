import { useCallback, useEffect, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelBatchMount,
  cancelGitSourcePull,
  cancelSourceScan,
  listBatchMountTasks,
  listGitSourcePullTasks,
  listSourceScanTasks,
  startBatchMount,
  startGitSourcePull,
  startSourceScan,
  subscribeBatchMountTasks,
  subscribeGitSourcePullTasks,
  type BatchMountTaskSnapshot,
  type GitSourcePullTaskSnapshot,
  type SourceScanScope,
} from "../../services/catalog";
import type { TaskView } from "../../types/taskCenter";
import { subscribeTaskUpdated } from "../../services/taskCenterService";
import { taskInvalidationRegistry } from "./taskInvalidationRegistry";
import { catalogKeys } from "../query/catalogQueries";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";
import { checkEventTenantMatch, mergeTaskSnapshot } from "./taskMergeUtils";

export interface CatalogTaskContextValue {
  sourceScan: TaskView | null;
  batchMount: BatchMountTaskSnapshot | null;
  gitSourcePull: GitSourcePullTaskSnapshot | null;
  startSourceScan: (
    kind?: "skill" | "prompt" | "rule",
    scope?: SourceScanScope,
  ) => Promise<TaskView>;
  cancelSourceScan: (taskId: string) => Promise<TaskView>;
  startBatchMount: (
    params: Parameters<typeof startBatchMount>[0],
  ) => Promise<BatchMountTaskSnapshot>;
  cancelBatchMount: (taskId: string) => Promise<BatchMountTaskSnapshot>;
  startGitSourcePull: (
    sourceIds: string[],
  ) => Promise<GitSourcePullTaskSnapshot>;
  cancelGitSourcePull: (taskId: string) => Promise<GitSourcePullTaskSnapshot>;
}

export function sourceScanQueryOptions(scope: QueryScope) {
  return queryOptions<TaskView | null>({
    queryKey: taskKeys.resource(scope, "catalog-source-scan"),
    queryFn: async (): Promise<TaskView | null> => {
      const tasks = await listSourceScanTasks();
      return selectLatest(tasks);
    },
    structuralSharing: (oldData, newData) =>
      mergeTaskSnapshot(
        oldData as TaskView | null,
        newData as TaskView | null,
      ),
    staleTime: 1000,
    networkMode: "always",
  });
}

export function batchMountQueryOptions(scope: QueryScope) {
  return queryOptions<BatchMountTaskSnapshot | null>({
    queryKey: taskKeys.resource(scope, "catalog-batch-mount"),
    queryFn: async () => selectLatest(await listBatchMountTasks()),
    structuralSharing: (oldData, newData) =>
      mergeTaskSnapshot(
        oldData as BatchMountTaskSnapshot | null,
        newData as BatchMountTaskSnapshot | null,
      ),
    staleTime: 1000,
    networkMode: "always",
  });
}

export function gitSourcePullQueryOptions(scope: QueryScope) {
  return queryOptions<GitSourcePullTaskSnapshot | null>({
    queryKey: taskKeys.resource(scope, "catalog-git-source-pull"),
    queryFn: async () => selectLatest(await listGitSourcePullTasks()),
    structuralSharing: (oldData, newData) =>
      mergeTaskSnapshot(
        oldData as GitSourcePullTaskSnapshot | null,
        newData as GitSourcePullTaskSnapshot | null,
      ),
    staleTime: 1000,
    networkMode: "always",
  });
}

export function CatalogTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const scanQueryKey = taskKeys.resource(activeScope, "catalog-source-scan");
  const mountQueryKey = taskKeys.resource(activeScope, "catalog-batch-mount");
  const pullQueryKey = taskKeys.resource(
    activeScope,
    "catalog-git-source-pull",
  );

  useEffect(() => {
    return taskInvalidationRegistry.register("scan", async (_task, qc) => {
      await qc.invalidateQueries({ queryKey: catalogKeys.root(activeScope) });
    });
  }, [activeScope]);

  useQuery({
    ...sourceScanQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const state = query.state.data;
      return state?.state === "running" ||
        state?.state === "cancelling" ||
        state?.state === "pending"
        ? 1000
        : 10000;
    },
    refetchIntervalInBackground: true,
  });

  useQuery({
    ...batchMountQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const state = query.state.data;
      return state?.status === "running" || state?.status === "cancelling"
        ? 1000
        : 10000;
    },
    refetchIntervalInBackground: true,
  });

  useQuery({
    ...gitSourcePullQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const state = query.state.data;
      return state?.status === "running" || state?.status === "cancelling"
        ? 1000
        : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<TaskView | null, TaskView>
        merge={mergeTaskSnapshot}
        validateTenant={(event) =>
          checkEventTenantMatch(event, activeScope.tenantId)
        }
        queryKey={scanQueryKey}
        subscribe={(listener) =>
          subscribeTaskUpdated((task) => {
            if (task.kind.toLowerCase() === "scan") {
              listener(task);
            }
          })
        }
      />
      <TaskEventBridge<BatchMountTaskSnapshot | null, BatchMountTaskSnapshot>
        merge={mergeTaskSnapshot}
        validateTenant={(event) =>
          checkEventTenantMatch(event, activeScope.tenantId)
        }
        queryKey={mountQueryKey}
        subscribe={subscribeBatchMountTasks}
      />
      <TaskEventBridge<
        GitSourcePullTaskSnapshot | null,
        GitSourcePullTaskSnapshot
      >
        merge={mergeTaskSnapshot}
        validateTenant={(event) =>
          checkEventTenantMatch(event, activeScope.tenantId)
        }
        queryKey={pullQueryKey}
        subscribe={subscribeGitSourcePullTasks}
      />
      {children ?? null}
    </>
  );
}

export function useCatalogTasks(): CatalogTaskContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const scanQueryKey = taskKeys.resource(activeScope, "catalog-source-scan");
  const mountQueryKey = taskKeys.resource(activeScope, "catalog-batch-mount");
  const pullQueryKey = taskKeys.resource(
    activeScope,
    "catalog-git-source-pull",
  );

  const scanQuery = useQuery({
    ...sourceScanQueryOptions(activeScope),
  });
  const mountQuery = useQuery({
    ...batchMountQueryOptions(activeScope),
  });
  const pullQuery = useQuery({
    ...gitSourcePullQueryOptions(activeScope),
  });

  const startScan = useCallback(
    async (
      kind?: "skill" | "prompt" | "rule",
      scanScope: SourceScanScope = "all",
    ) => {
      const task = await startSourceScan(kind, scanScope);
      queryClient.setQueryData<TaskView | null>(
        scanQueryKey,
        task,
      );
      return task;
    },
    [queryClient, scanQueryKey],
  );

  const cancelScan = useCallback(
    async (taskId: string) => {
      const task = await cancelSourceScan(taskId);
      queryClient.setQueryData<TaskView | null>(
        scanQueryKey,
        task,
      );
      return task;
    },
    [queryClient, scanQueryKey],
  );

  const startMount = useCallback(
    async (params: Parameters<typeof startBatchMount>[0]) => {
      const snapshot = await startBatchMount(params);
      queryClient.setQueryData<BatchMountTaskSnapshot | null>(
        mountQueryKey,
        snapshot,
      );
      return snapshot;
    },
    [mountQueryKey, queryClient],
  );

  const cancelMount = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelBatchMount(taskId);
      queryClient.setQueryData<BatchMountTaskSnapshot | null>(
        mountQueryKey,
        snapshot,
      );
      return snapshot;
    },
    [mountQueryKey, queryClient],
  );

  const startPull = useCallback(
    async (sourceIds: string[]) => {
      const snapshot = await startGitSourcePull(sourceIds);
      queryClient.setQueryData<GitSourcePullTaskSnapshot | null>(
        pullQueryKey,
        snapshot,
      );
      return snapshot;
    },
    [pullQueryKey, queryClient],
  );

  const cancelPull = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelGitSourcePull(taskId);
      queryClient.setQueryData<GitSourcePullTaskSnapshot | null>(
        pullQueryKey,
        snapshot,
      );
      return snapshot;
    },
    [pullQueryKey, queryClient],
  );

  return {
    batchMount: mountQuery.data ?? null,
    cancelBatchMount: cancelMount,
    cancelGitSourcePull: cancelPull,
    cancelSourceScan: cancelScan,
    gitSourcePull: pullQuery.data ?? null,
    sourceScan: scanQuery.data ?? null,
    startBatchMount: startMount,
    startGitSourcePull: startPull,
    startSourceScan: startScan,
  };
}

function selectLatest<
  T extends { started_at?: string | null; startedAt?: string | null },
>(tasks: T[]): T | null {
  const sorted = [...tasks].sort((left, right) => {
    const l = left.started_at ?? left.startedAt ?? "";
    const r = right.started_at ?? right.startedAt ?? "";
    return l.localeCompare(r);
  });
  return sorted[sorted.length - 1] ?? null;
}
