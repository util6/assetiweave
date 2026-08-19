import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import {
  getConversationSearchIndexStatus,
  getConversationSearchIndexTask,
  startConversationSearchIndexRebuild,
  subscribeConversationSearchIndexTasks,
  type ConversationSearchIndexStatus,
  type ConversationSearchIndexTaskSnapshot,
} from "../../services/conversations";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

interface SearchIndexRuntimeState {
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
}

interface SearchIndexStatusRuntimeEvent {
  status: ConversationSearchIndexStatus;
  snapshot: ConversationSearchIndexTaskSnapshot;
}

interface SearchIndexContextValue {
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
  rebuild: () => Promise<ConversationSearchIndexTaskSnapshot>;
  refresh: () => Promise<void>;
}

const SearchIndexContext = createContext<SearchIndexContextValue | null>(null);

export function SearchIndexProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<SearchIndexRuntimeState, ConversationSearchIndexTaskSnapshot | SearchIndexStatusRuntimeEvent>>(
    () => ({
      initialState: { status: null, task: null },
      isRunning: (state) => state.task?.status === "running",
      merge: (current, incoming) => {
        if (isSearchIndexTaskSnapshot(incoming)) {
          return { ...current, task: incoming };
        }

        if (isSearchIndexStatusRuntimeEvent(incoming)) {
          return { status: incoming.status, task: incoming.snapshot };
        }

        return {
          status: incoming.status,
          task: current.task?.status === "running" && !incoming.task ? current.task : incoming.task,
        };
      },
      refresh: async () => {
        const [status, task] = await Promise.all([
          getConversationSearchIndexStatus(),
          getConversationSearchIndexTask(),
        ]);
        return { status, task };
      },
      subscribe: (listener) => subscribeConversationSearchIndexTasks((snapshot) => {
        listener(snapshot);
        if (snapshot.status !== "running") {
          void getConversationSearchIndexStatus()
            .then((status) => listener({ status, snapshot }))
            .catch(() => undefined);
        }
      }),
    }),
    [],
  );
  const { merge, refresh, state } = useBackgroundTaskRuntime(adapter);

  const rebuild = useCallback(async () => {
    const snapshot = await startConversationSearchIndexRebuild();
    merge(snapshot);
    return snapshot;
  }, [merge]);

  const value = useMemo<SearchIndexContextValue>(
    () => ({
      status: state.status,
      task: state.task,
      rebuild,
      refresh: async () => {
        await refresh();
      },
    }),
    [rebuild, refresh, state.status, state.task],
  );
  return <SearchIndexContext.Provider value={value}>{children}</SearchIndexContext.Provider>;
}

export function useSearchIndex() {
  const context = useContext(SearchIndexContext);
  if (!context) throw new Error("useSearchIndex must be used inside SearchIndexProvider");
  return context;
}

function isSearchIndexTaskSnapshot(
  incoming: SearchIndexRuntimeState | ConversationSearchIndexTaskSnapshot | SearchIndexStatusRuntimeEvent,
): incoming is ConversationSearchIndexTaskSnapshot {
  return "id" in incoming;
}

function isSearchIndexStatusRuntimeEvent(
  incoming: ConversationSearchIndexTaskSnapshot | SearchIndexRuntimeState | SearchIndexStatusRuntimeEvent,
): incoming is SearchIndexStatusRuntimeEvent {
  return "snapshot" in incoming;
}
