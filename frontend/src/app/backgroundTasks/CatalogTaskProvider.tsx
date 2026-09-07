import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  cancelBatchMount,
  cancelSourceScan,
  listBatchMountTasks,
  listSourceScanTasks,
  startBatchMount,
  startSourceScan,
  subscribeBatchMountTasks,
  subscribeSourceScanTasks,
  type BatchMountTaskSnapshot,
  type SourceScanScope,
  type SourceScanTaskSnapshot,
} from "../../services/catalog";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";
import { checkEventTenantMatch, mergeTaskSnapshot } from "./taskMergeUtils";

export interface CatalogTaskContextValue {
  sourceScan: SourceScanTaskSnapshot | null;
  batchMount: BatchMountTaskSnapshot | null;
  startSourceScan: (
    kind?: "skill" | "prompt" | "rule",
    scope?: SourceScanScope,
  ) => Promise<SourceScanTaskSnapshot>;
  cancelSourceScan: (taskId: string) => Promise<SourceScanTaskSnapshot>;
  startBatchMount: (
    params: Parameters<typeof startBatchMount>[0],
  ) => Promise<BatchMountTaskSnapshot>;
  cancelBatchMount: (taskId: string) => Promise<BatchMountTaskSnapshot>;
}

export function sourceScanQueryOptions(scope: QueryScope) {
  return queryOptions<SourceScanTaskSnapshot | null>({
    queryKey: taskKeys.resource(scope, "catalog-source-scan"),
    queryFn: async () => selectLatest(await listSourceScanTasks()),
    structuralSharing: (oldData, newData) =>
      mergeTaskSnapshot(
        oldData as SourceScanTaskSnapshot | null,
        newData as SourceScanTaskSnapshot | null,
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

export function CatalogTaskProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const scanQueryKey = taskKeys.resource(activeScope, "catalog-source-scan");
  const mountQueryKey = taskKeys.resource(activeScope, "catalog-batch-mount");

  useQuery({
    ...sourceScanQueryOptions(activeScope),
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

  return (
    <>
      <TaskEventBridge<SourceScanTaskSnapshot | null, SourceScanTaskSnapshot>
        merge={mergeTaskSnapshot}
        validateTenant={(event) =>
          checkEventTenantMatch(event, activeScope.tenantId)
        }
        queryKey={scanQueryKey}
        subscribe={subscribeSourceScanTasks}
      />
      <TaskEventBridge<BatchMountTaskSnapshot | null, BatchMountTaskSnapshot>
        merge={mergeTaskSnapshot}
        validateTenant={(event) =>
          checkEventTenantMatch(event, activeScope.tenantId)
        }
        queryKey={mountQueryKey}
        subscribe={subscribeBatchMountTasks}
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

  const scanQuery = useQuery({
    ...sourceScanQueryOptions(activeScope),
  });
  const mountQuery = useQuery({
    ...batchMountQueryOptions(activeScope),
  });

  const startScan = useCallback(
    async (
      kind?: "skill" | "prompt" | "rule",
      scanScope: SourceScanScope = "all",
    ) => {
      const snapshot = await startSourceScan(kind, scanScope);
      queryClient.setQueryData<SourceScanTaskSnapshot | null>(
        scanQueryKey,
        snapshot,
      );
      return snapshot;
    },
    [queryClient, scanQueryKey],
  );

  const cancelScan = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelSourceScan(taskId);
      queryClient.setQueryData<SourceScanTaskSnapshot | null>(
        scanQueryKey,
        snapshot,
      );
      return snapshot;
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

  return {
    batchMount: mountQuery.data ?? null,
    cancelBatchMount: cancelMount,
    cancelSourceScan: cancelScan,
    sourceScan: scanQuery.data ?? null,
    startBatchMount: startMount,
    startSourceScan: startScan,
  };
}

function selectLatest<T extends { started_at: string }>(tasks: T[]): T | null {
  const sorted = [...tasks].sort((left, right) =>
    left.started_at.localeCompare(right.started_at),
  );
  return sorted[sorted.length - 1] ?? null;
}
