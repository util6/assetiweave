import { createContext, useCallback, useContext, useMemo, type ReactNode } from "react";
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
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

interface CatalogTaskContextValue {
  sourceScan: SourceScanTaskSnapshot | null;
  batchMount: BatchMountTaskSnapshot | null;
  startSourceScan: (kind?: "skill" | "prompt" | "rule", scope?: SourceScanScope) => Promise<SourceScanTaskSnapshot>;
  cancelSourceScan: (taskId: string) => Promise<SourceScanTaskSnapshot>;
  startBatchMount: (params: Parameters<typeof startBatchMount>[0]) => Promise<BatchMountTaskSnapshot>;
  cancelBatchMount: (taskId: string) => Promise<BatchMountTaskSnapshot>;
}

const CatalogTaskContext = createContext<CatalogTaskContextValue | null>(null);

export function CatalogTaskProvider({ children }: { children: ReactNode }) {
  const sourceAdapter = useMemo<BackgroundTaskRuntimeAdapter<SourceScanTaskSnapshot | null, SourceScanTaskSnapshot>>(
    () => ({
      initialState: null,
      isRunning: (state) => state?.status === "running" || state?.status === "cancelling",
      merge: (_, incoming) => (Array.isArray(incoming) ? selectLatest(incoming) : incoming),
      refresh: async () => selectLatest(await listSourceScanTasks()),
      subscribe: (listener) => subscribeSourceScanTasks(listener),
      pollIntervalMs: 750,
    }),
    [],
  );
  const batchAdapter = useMemo<BackgroundTaskRuntimeAdapter<BatchMountTaskSnapshot | null, BatchMountTaskSnapshot>>(
    () => ({
      initialState: null,
      isRunning: (state) => state?.status === "running" || state?.status === "cancelling",
      merge: (_, incoming) => (Array.isArray(incoming) ? selectLatest(incoming) : incoming),
      refresh: async () => selectLatest(await listBatchMountTasks()),
      subscribe: (listener) => subscribeBatchMountTasks(listener),
      pollIntervalMs: 750,
    }),
    [],
  );
  const sourceRuntime = useBackgroundTaskRuntime(sourceAdapter);
  const batchRuntime = useBackgroundTaskRuntime(batchAdapter);

  const startScan = useCallback(async (kind?: "skill" | "prompt" | "rule", scope: SourceScanScope = "all") => {
    const snapshot = await startSourceScan(kind, scope);
    sourceRuntime.merge(snapshot);
    return snapshot;
  }, [sourceRuntime.merge]);
  const cancelScan = useCallback(async (taskId: string) => {
    const snapshot = await cancelSourceScan(taskId);
    sourceRuntime.merge(snapshot);
    return snapshot;
  }, [sourceRuntime.merge]);
  const startMount = useCallback(async (params: Parameters<typeof startBatchMount>[0]) => {
    const snapshot = await startBatchMount(params);
    batchRuntime.merge(snapshot);
    return snapshot;
  }, [batchRuntime.merge]);
  const cancelMount = useCallback(async (taskId: string) => {
    const snapshot = await cancelBatchMount(taskId);
    batchRuntime.merge(snapshot);
    return snapshot;
  }, [batchRuntime.merge]);

  const value = useMemo<CatalogTaskContextValue>(() => ({
    sourceScan: sourceRuntime.state,
    batchMount: batchRuntime.state,
    startSourceScan: startScan,
    cancelSourceScan: cancelScan,
    startBatchMount: startMount,
    cancelBatchMount: cancelMount,
  }), [batchRuntime.state, cancelMount, cancelScan, sourceRuntime.state, startMount, startScan]);

  return <CatalogTaskContext.Provider value={value}>{children}</CatalogTaskContext.Provider>;
}

export function useCatalogTasks() {
  const context = useContext(CatalogTaskContext);
  if (!context) {
    throw new Error("useCatalogTasks must be used inside CatalogTaskProvider");
  }
  return context;
}

function selectLatest<T extends { started_at: string }>(tasks: T[]): T | null {
  const sorted = [...tasks].sort((left, right) => left.started_at.localeCompare(right.started_at));
  return sorted[sorted.length - 1] ?? null;
}
