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
  getConversationSyncTask,
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
}

const ConversationSyncContext = createContext<ConversationSyncContextValue | null>(null);

export function ConversationSyncProvider({ children }: { children: ReactNode }) {
  const [task, setTask] = useState<ConversationSyncTaskSnapshot | null>(null);

  const refreshTask = useCallback(async () => {
    const snapshot = await getConversationSyncTask();
    setTask((current) => mergeConversationTaskSnapshot(snapshot, current));
    return snapshot;
  }, []);

  useEffect(() => {
    let cancelled = false;
    void getConversationSyncTask()
      .then((snapshot) => {
        if (!cancelled) {
          setTask((current) => current ?? mergeConversationTaskSnapshot(snapshot, current));
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
          setTask((current) => mergeConversationTaskSnapshot(event.payload, current));
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
    if (task?.status !== "running") {
      return;
    }

    let polling = false;
    const intervalId = window.setInterval(() => {
      if (polling) {
        return;
      }
      polling = true;
      void refreshTask()
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, SYNC_STATUS_POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [refreshTask, task?.id, task?.status]);

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
      setTask(nextSnapshot);
      return nextSnapshot;
    },
    [],
  );

  const value = useMemo<ConversationSyncContextValue>(
    () => ({
      startSync,
      task,
    }),
    [startSync, task],
  );

  return (
    <ConversationSyncContext.Provider value={value}>
      {children}
    </ConversationSyncContext.Provider>
  );
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
