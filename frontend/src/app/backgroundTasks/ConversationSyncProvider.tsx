import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";
import {
  listConversationSyncTasks,
  subscribeConversationSyncTasks,
  syncConversations,
  type ConversationSyncTaskSnapshot,
  type ConversationSyncMode,
} from "../../services/conversations";
import type { ConversationRecordKind } from "../../types";

interface ConversationSyncContextValue {
  startSync: (params: {
    source_id?: string | null;
    adapter_id?: string | null;
    record_kind?: ConversationRecordKind | null;
    mode?: ConversationSyncMode;
    dry_run?: boolean;
  }) => Promise<ConversationSyncTaskSnapshot>;
  task: ConversationSyncTaskSnapshot | null;
  taskFor: (recordKind: ConversationRecordKind) => ConversationSyncTaskSnapshot | null;
  tasks: ConversationSyncTaskSnapshot[];
}

type ConversationSyncTaskScope = ConversationRecordKind | "all";
type ConversationSyncTaskMap = Record<ConversationSyncTaskScope, ConversationSyncTaskSnapshot | null>;
type ConversationSyncRuntimeEvent = {
  fallbackScope?: ConversationSyncTaskScope;
  snapshot?: ConversationSyncTaskSnapshot;
  snapshots?: ConversationSyncTaskSnapshot[];
};

const EMPTY_TASKS: ConversationSyncTaskMap = { all: null, session: null, web: null };

const ConversationSyncContext = createContext<ConversationSyncContextValue | null>(null);

export function ConversationSyncProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<ConversationSyncTaskMap, ConversationSyncRuntimeEvent>>(
    () => ({
      initialState: EMPTY_TASKS,
      isRunning: (state) => Object.values(state).some((task) => task?.status === "running"),
      merge: (current, incoming) => {
        if (isConversationSyncRuntimeEvent(incoming)) {
          if (incoming.snapshots) {
            return mergeConversationTaskSnapshots(incoming.snapshots, current);
          }
          return incoming.snapshot
            ? mergeConversationTaskIntoMap(incoming.snapshot, current, incoming.fallbackScope)
            : current;
        }

        return Object.values(incoming).reduce(
          (next, snapshot) => snapshot ? mergeConversationTaskIntoMap(snapshot, next) : next,
          current,
        );
      },
      refresh: async () => mergeConversationTaskSnapshots(await listConversationSyncTasks(), EMPTY_TASKS),
      subscribe: (listener) => subscribeConversationSyncTasks((snapshot) => listener({ snapshot })),
    }),
    [],
  );
  const { merge, state: taskMap } = useBackgroundTaskRuntime(adapter);

  const startSync = useCallback(
    async (params: {
      source_id?: string | null;
      adapter_id?: string | null;
      record_kind?: ConversationRecordKind | null;
      mode?: ConversationSyncMode;
      dry_run?: boolean;
    }) => {
      const snapshot = await syncConversations(params);
      const nextSnapshot = mergeConversationTaskSnapshot(
        snapshot,
        null,
        params.record_kind ?? (params.mode === "full" ? "all" : "session"),
      ) ?? snapshot;
      merge({
        snapshot: nextSnapshot,
        fallbackScope: params.record_kind ?? (params.mode === "full" ? "all" : "session"),
      });
      return nextSnapshot;
    },
    [merge],
  );

  const taskFor = useCallback(
    (recordKind: ConversationRecordKind) => latestConversationTask(
      taskMap[recordKind],
      taskMap.all,
    ),
    [taskMap],
  );
  const tasks = useMemo(
    () => Object.values(taskMap).filter((task): task is ConversationSyncTaskSnapshot => Boolean(task)),
    [taskMap],
  );
  const task = tasks[tasks.length - 1] ?? null;
  const value = useMemo<ConversationSyncContextValue>(
    () => ({
      startSync,
      task,
      taskFor,
      tasks,
    }),
    [startSync, task, taskFor, tasks],
  );

  return (
    <ConversationSyncContext.Provider value={value}>
      {children}
    </ConversationSyncContext.Provider>
  );
}

function mergeConversationTaskSnapshots(
  snapshots: ConversationSyncTaskSnapshot[],
  current: ConversationSyncTaskMap,
): ConversationSyncTaskMap {
  return snapshots.reduce(
    (next, snapshot) => mergeConversationTaskIntoMap(snapshot, next),
    current,
  );
}

function isConversationSyncRuntimeEvent(
  incoming: ConversationSyncTaskMap | ConversationSyncRuntimeEvent,
): incoming is ConversationSyncRuntimeEvent {
  return "snapshot" in incoming || "snapshots" in incoming;
}

function mergeConversationTaskIntoMap(
  snapshot: ConversationSyncTaskSnapshot,
  current: ConversationSyncTaskMap,
  fallbackScope: ConversationSyncTaskScope | null = null,
): ConversationSyncTaskMap {
  const currentSnapshot = Object.values(current).find((task) => task?.id === snapshot.id) ?? null;
  const merged = mergeConversationTaskSnapshot(snapshot, currentSnapshot, fallbackScope);
  const recordKind = normalizeConversationRecordKind(merged?.record_kind);
  const scope = recordKind ?? (merged?.record_kind === null ? "all" : fallbackScope);
  if (!merged || !scope) {
    return current;
  }
  return { ...current, [scope]: merged };
}

export function useConversationSync() {
  const context = useContext(ConversationSyncContext);
  if (!context) {
    throw new Error("useConversationSync must be used inside ConversationSyncProvider");
  }
  return context;
}

function mergeConversationTaskSnapshot(
  snapshot: ConversationSyncTaskSnapshot | null,
  current: ConversationSyncTaskSnapshot | null,
  fallbackScope: ConversationSyncTaskScope | null = null,
): ConversationSyncTaskSnapshot | null {
  if (!snapshot) {
    return null;
  }

  const recordKind =
    normalizeConversationRecordKind(snapshot.record_kind) ??
    inferConversationRecordKindFromResult(snapshot.result) ??
    (current?.id === snapshot.id ? normalizeConversationRecordKind(current.record_kind) : null) ??
    (fallbackScope === "all" ? null : fallbackScope);

  return recordKind ? { ...snapshot, record_kind: recordKind } : snapshot;
}

function inferConversationRecordKindFromResult(result: unknown): ConversationRecordKind | null {
  if (!isRecord(result) || !Array.isArray(result.results)) {
    return null;
  }

  for (const item of result.results) {
    if (!isRecord(item)) {
      continue;
    }
    const recordKind = normalizeConversationRecordKind(item.record_kind);
    if (recordKind) {
      return recordKind;
    }
  }

  return null;
}

function normalizeConversationRecordKind(value: unknown): ConversationRecordKind | null {
  return value === "session" || value === "web" ? value : null;
}

function latestConversationTask(
  scopedTask: ConversationSyncTaskSnapshot | null,
  allTask: ConversationSyncTaskSnapshot | null,
) {
  if (!scopedTask) return allTask;
  if (!allTask) return scopedTask;
  return allTask.started_at > scopedTask.started_at ? allTask : scopedTask;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
