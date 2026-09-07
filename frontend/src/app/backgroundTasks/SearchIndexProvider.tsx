import { useCallback, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  startConversationSearchIndexRebuild,
  subscribeConversationSearchIndexTasks,
  type ConversationSearchIndexStatus,
  type ConversationSearchIndexTaskSnapshot,
} from "../../services/conversations";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import {
  mergeSearchIndexQueryState,
  searchIndexQueryOptions,
  type SearchIndexQueryState,
} from "./searchIndexQueries";

export interface SearchIndexContextValue {
  rebuild: () => Promise<ConversationSearchIndexTaskSnapshot>;
  refresh: () => Promise<void>;
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
}

export function SearchIndexProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "search-index");

  // Root task query: single owner of polling
  useQuery({
    ...searchIndexQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) =>
      query.state.data?.task?.status === "running" ? 1000 : 10000,
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<
        SearchIndexQueryState,
        ConversationSearchIndexTaskSnapshot
      >
        merge={(current, snapshot) => {
          return mergeSearchIndexQueryState(current, {
            status: current?.status ?? null,
            task: snapshot,
          });
        }}
        queryKey={queryKey}
        subscribe={(listener) =>
          subscribeConversationSearchIndexTasks((snapshot) => {
            listener(snapshot);
            if (snapshot.status !== "running") {
              void queryClient.invalidateQueries({
                exact: true,
                queryKey,
              });
            }
          })
        }
      />
      {children}
    </>
  );
}

export function useSearchIndex(): SearchIndexContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "search-index");

  const query = useQuery({
    ...searchIndexQueryOptions(activeScope),
  });

  const rebuild = useCallback(async () => {
    const snapshot = await startConversationSearchIndexRebuild();
    queryClient.setQueryData<SearchIndexQueryState>(queryKey, (current) =>
      mergeSearchIndexQueryState(current, {
        status: current?.status ?? null,
        task: snapshot,
      }),
    );
    return snapshot;
  }, [queryClient, queryKey]);

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  return {
    rebuild,
    refresh,
    status: query.data?.status ?? null,
    task: query.data?.task ?? null,
  };
}
