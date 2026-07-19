import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  listConversationSyncTasks,
  syncConversations,
  type ConversationSyncTaskSnapshot,
} from "../../services/conversations";
import type { ConversationRecordKind } from "../../types";

const CONVERSATION_SYNC_TASK_UPDATED_EVENT = "conversation-sync-task-updated";
const SYNC_STATUS_POLL_INTERVAL_MS = 1000;

interface ConversationSyncContextValue {
  startSync: (params: {
    source_id?: string | null;
    adapter_id?: string | null;
    record_kind?: ConversationRecordKind | null;
    dry_run?: boolean;
  }) => Promise<ConversationSyncTaskSnapshot>;
  task: ConversationSyncTaskSnapshot | null;
  taskFor: (recordKind: ConversationRecordKind) => ConversationSyncTaskSnapshot | null;
  tasks: ConversationSyncTaskSnapshot[];
}

type ConversationSyncTaskMap = Record<ConversationRecordKind, ConversationSyncTaskSnapshot | null>;

const EMPTY_TASKS: ConversationSyncTaskMap = { session: null, web: null };

const ConversationSyncContext = createContext<ConversationSyncContextValue | null>(null);

export function ConversationSyncProvider({ children }: { children: ReactNode }) {
  const [taskMap, setTaskMap] = useState<ConversationSyncTaskMap>(EMPTY_TASKS);

  const refreshTasks = useCallback(async () => {
    const snapshots = await listConversationSyncTasks();
    setTaskMap((current) => mergeConversationTaskSnapshots(snapshots, current));
    return snapshots;
  }, []);

  useEffect(() => {
    let cancelled = false;
    void listConversationSyncTasks()
      .then((snapshots) => {
        if (!cancelled) {
          setTaskMap((current) => mergeConversationTaskSnapshots(snapshots, current));
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<ConversationSyncTaskSnapshot>(
      CONVERSATION_SYNC_TASK_UPDATED_EVENT,
      (event) => {
        if (!cancelled) {
          setTaskMap((current) => mergeConversationTaskIntoMap(event.payload, current));
        }
      },
    )
      .then((removeListener) => {
        if (cancelled) {
          removeListener();
        } else {
          unlisten = removeListener;
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const runningTaskKey = Object.values(taskMap)
      .filter((task) => task?.status === "running")
      .map((task) => task?.id)
      .join(":");
    if (!runningTaskKey) {
      return;
    }

    let polling = false;
    const intervalId = window.setInterval(() => {
      if (polling) {
        return;
      }
      polling = true;
      void refreshTasks()
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, SYNC_STATUS_POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [refreshTasks, taskMap.session?.id, taskMap.session?.status, taskMap.web?.id, taskMap.web?.status]);

  const startSync = useCallback(
    async (params: {
      source_id?: string | null;
      adapter_id?: string | null;
      record_kind?: ConversationRecordKind | null;
      dry_run?: boolean;
    }) => {
      const snapshot = await syncConversations(params);
      const nextSnapshot = mergeConversationTaskSnapshot(
        snapshot,
        null,
        params.record_kind ?? "session",
      ) ?? snapshot;
      setTaskMap((current) => mergeConversationTaskIntoMap(nextSnapshot, current, params.record_kind ?? "session"));
      return nextSnapshot;
    },
    [],
  );

  const taskFor = useCallback(
    (recordKind: ConversationRecordKind) => taskMap[recordKind],
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

function mergeConversationTaskIntoMap(
  snapshot: ConversationSyncTaskSnapshot,
  current: ConversationSyncTaskMap,
  fallbackRecordKind: ConversationRecordKind | null = null,
): ConversationSyncTaskMap {
  const currentSnapshot = Object.values(current).find((task) => task?.id === snapshot.id) ?? null;
  const merged = mergeConversationTaskSnapshot(snapshot, currentSnapshot, fallbackRecordKind);
  const recordKind = normalizeConversationRecordKind(merged?.record_kind);
  if (!merged || !recordKind) {
    return current;
  }
  return { ...current, [recordKind]: merged };
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
  fallbackRecordKind: ConversationRecordKind | null = null,
): ConversationSyncTaskSnapshot | null {
  if (!snapshot) {
    return null;
  }

  const recordKind =
    normalizeConversationRecordKind(snapshot.record_kind) ??
    inferConversationRecordKindFromResult(snapshot.result) ??
    (current?.id === snapshot.id ? normalizeConversationRecordKind(current.record_kind) : null) ??
    fallbackRecordKind;

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
