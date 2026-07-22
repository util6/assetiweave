import { listen } from "@tauri-apps/api/event";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  getConversationSearchIndexStatus,
  getConversationSearchIndexTask,
  startConversationSearchIndexRebuild,
  type ConversationSearchIndexStatus,
  type ConversationSearchIndexTaskSnapshot,
} from "../../services/conversations";

const SEARCH_INDEX_TASK_UPDATED_EVENT = "conversation-search-index-task-updated";
const POLL_INTERVAL_MS = 1000;

interface SearchIndexContextValue {
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
  rebuild: () => Promise<ConversationSearchIndexTaskSnapshot>;
  refresh: () => Promise<void>;
}

const SearchIndexContext = createContext<SearchIndexContextValue | null>(null);

export function SearchIndexProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<ConversationSearchIndexStatus | null>(null);
  const [task, setTask] = useState<ConversationSearchIndexTaskSnapshot | null>(null);

  const refresh = useCallback(async () => {
    const [nextStatus, nextTask] = await Promise.all([
      getConversationSearchIndexStatus(),
      getConversationSearchIndexTask(),
    ]);
    setStatus(nextStatus);
    setTask((current) => (current?.status === "running" && !nextTask ? current : nextTask));
  }, []);

  useEffect(() => {
    void refresh().catch(() => {});
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<ConversationSearchIndexTaskSnapshot>(SEARCH_INDEX_TASK_UPDATED_EVENT, (event) => {
      if (cancelled) return;
      setTask(event.payload);
      if (event.payload.status !== "running") {
        void getConversationSearchIndexStatus().then(setStatus).catch(() => {});
      }
    })
      .then((removeListener) => {
        if (cancelled) removeListener();
        else unlisten = removeListener;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (task?.status !== "running") return;
    let polling = false;
    const interval = window.setInterval(() => {
      if (polling) return;
      polling = true;
      void refresh()
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, POLL_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [refresh, task?.id, task?.status]);

  const rebuild = useCallback(async () => {
    const snapshot = await startConversationSearchIndexRebuild();
    setTask(snapshot);
    return snapshot;
  }, []);

  const value = useMemo<SearchIndexContextValue>(
    () => ({ status, task, rebuild, refresh }),
    [status, task, rebuild, refresh],
  );
  return <SearchIndexContext.Provider value={value}>{children}</SearchIndexContext.Provider>;
}

export function useSearchIndex() {
  const context = useContext(SearchIndexContext);
  if (!context) throw new Error("useSearchIndex must be used inside SearchIndexProvider");
  return context;
}
