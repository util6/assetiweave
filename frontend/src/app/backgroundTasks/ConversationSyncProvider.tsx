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

const CONVERSATION_SYNC_TASK_UPDATED_EVENT = "conversation-sync-task-updated";
const SYNC_STATUS_POLL_INTERVAL_MS = 1000;

interface ConversationSyncContextValue {
  startSync: (params: {
    source_id?: string | null;
    adapter_id?: string | null;
    dry_run?: boolean;
  }) => Promise<ConversationSyncTaskSnapshot>;
  task: ConversationSyncTaskSnapshot | null;
}

const ConversationSyncContext = createContext<ConversationSyncContextValue | null>(null);

export function ConversationSyncProvider({ children }: { children: ReactNode }) {
  const [task, setTask] = useState<ConversationSyncTaskSnapshot | null>(null);

  const refreshTask = useCallback(async () => {
    const snapshot = await getConversationSyncTask();
    setTask(snapshot);
    return snapshot;
  }, []);

  useEffect(() => {
    let cancelled = false;
    void getConversationSyncTask()
      .then((snapshot) => {
        if (!cancelled) {
          setTask((current) => current ?? snapshot);
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
          setTask(event.payload);
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
      dry_run?: boolean;
    }) => {
      const snapshot = await syncConversations(params);
      setTask(snapshot);
      return snapshot;
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
