import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listConversationSyncTasks,
  cancelConversationSync,
  subscribeConversationSyncTasks,
  syncConversations,
  type ConversationSyncTaskSnapshot,
  type ConversationSyncMode,
} from "../../services/conversations";
import type { ConversationRecordKind } from "../../types";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

export interface ConversationSyncContextValue {
  startSync: (params: {
    source_id?: string | null;
    adapter_id?: string | null;
    record_kind?: ConversationRecordKind | null;
    mode?: ConversationSyncMode;
    dry_run?: boolean;
  }) => Promise<ConversationSyncTaskSnapshot>;
  cancelSync: (taskId: string) => Promise<ConversationSyncTaskSnapshot>;
  task: ConversationSyncTaskSnapshot | null;
  taskFor: (
    recordKind: ConversationRecordKind,
  ) => ConversationSyncTaskSnapshot | null;
  tasks: ConversationSyncTaskSnapshot[];
}

export type ConversationSyncTaskScope = ConversationRecordKind | "all";
export type ConversationSyncTaskMap = Record<
  ConversationSyncTaskScope,
  ConversationSyncTaskSnapshot | null
>;

export const EMPTY_TASKS: ConversationSyncTaskMap = {
  all: null,
  session: null,
  web: null,
};

export function conversationSyncQueryOptions(scope: QueryScope) {
  return queryOptions<ConversationSyncTaskMap>({
    queryKey: taskKeys.resource(scope, "conversation-sync"),
    queryFn: async () => {
      const snapshots = await listConversationSyncTasks();
      return mergeConversationTaskSnapshots(snapshots, EMPTY_TASKS);
    },
    structuralSharing: (oldData, newData) => {
      const current =
        (oldData as ConversationSyncTaskMap | undefined) ?? EMPTY_TASKS;
      const incoming =
        (newData as ConversationSyncTaskMap | undefined) ?? EMPTY_TASKS;
      return Object.values(incoming).reduce(
        (next, snapshot) =>
          snapshot ? mergeConversationTaskIntoMap(snapshot, next) : next,
        current,
      );
    },
    staleTime: 1000,
  });
}

export function ConversationSyncProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "conversation-sync");

  useQuery({
    ...conversationSyncQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const state = query.state.data;
      const isRunning =
        state &&
        Object.values(state).some(
          (task) => task?.status === "running" || task?.status === "cancelling",
        );
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<ConversationSyncTaskMap, ConversationSyncTaskSnapshot>
        merge={(current, snapshot) =>
          mergeConversationTaskIntoMap(snapshot, current ?? EMPTY_TASKS)
        }
        queryKey={queryKey}
        subscribe={subscribeConversationSyncTasks}
      />
      {children ?? null}
    </>
  );
}

export function useConversationSync(): ConversationSyncContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "conversation-sync");

  const query = useQuery({
    ...conversationSyncQueryOptions(activeScope),
  });

  const taskMap = query.data ?? EMPTY_TASKS;

  const startSync = useCallback(
    async (params: {
      source_id?: string | null;
      adapter_id?: string | null;
      record_kind?: ConversationRecordKind | null;
      mode?: ConversationSyncMode;
      dry_run?: boolean;
    }) => {
      const snapshot = await syncConversations(params);
      const nextSnapshot =
        mergeConversationTaskSnapshot(
          snapshot,
          null,
          params.record_kind ?? (params.mode === "full" ? "all" : "session"),
        ) ?? snapshot;
      queryClient.setQueryData<ConversationSyncTaskMap>(queryKey, (current) =>
        mergeConversationTaskIntoMap(
          nextSnapshot,
          current ?? EMPTY_TASKS,
          params.record_kind ?? (params.mode === "full" ? "all" : "session"),
        ),
      );
      return nextSnapshot;
    },
    [queryClient, queryKey],
  );

  const cancelSync = useCallback(
    async (taskId: string) => {
      const snapshot = await cancelConversationSync(taskId);
      queryClient.setQueryData<ConversationSyncTaskMap>(queryKey, (current) =>
        mergeConversationTaskIntoMap(snapshot, current ?? EMPTY_TASKS),
      );
      return snapshot;
    },
    [queryClient, queryKey],
  );

  const taskFor = useCallback(
    (recordKind: ConversationRecordKind) =>
      latestConversationTask(taskMap[recordKind], taskMap.all),
    [taskMap],
  );

  const tasks = Object.values(taskMap).filter(
    (task): task is ConversationSyncTaskSnapshot => Boolean(task),
  );
  const task = tasks[tasks.length - 1] ?? null;

  return {
    startSync,
    cancelSync,
    task,
    taskFor,
    tasks,
  };
}

export function mergeConversationTaskSnapshots(
  snapshots: ConversationSyncTaskSnapshot[],
  current: ConversationSyncTaskMap,
): ConversationSyncTaskMap {
  return snapshots.reduce(
    (next, snapshot) => mergeConversationTaskIntoMap(snapshot, next),
    current,
  );
}

export function mergeConversationTaskIntoMap(
  snapshot: ConversationSyncTaskSnapshot,
  current: ConversationSyncTaskMap,
  fallbackScope: ConversationSyncTaskScope | null = null,
): ConversationSyncTaskMap {
  const currentSnapshot =
    Object.values(current).find((task) => task?.id === snapshot.id) ?? null;
  const merged = mergeConversationTaskSnapshot(
    snapshot,
    currentSnapshot,
    fallbackScope,
  );
  const recordKind = normalizeConversationRecordKind(merged?.record_kind);
  const scope =
    fallbackScope === "all"
      ? "all"
      : recordKind === "session" || recordKind === "web"
        ? recordKind
        : fallbackScope === "session" || fallbackScope === "web"
          ? fallbackScope
          : "all";

  return {
    ...current,
    [scope]: latestConversationTask(current[scope], merged),
  };
}

export function mergeConversationTaskSnapshot(
  incoming: ConversationSyncTaskSnapshot,
  current: ConversationSyncTaskSnapshot | null,
  fallbackScope: ConversationSyncTaskScope | null = null,
): ConversationSyncTaskSnapshot {
  if (!current) {
    return {
      ...incoming,
      record_kind:
        normalizeConversationRecordKind(incoming.record_kind) ??
        normalizeConversationRecordKind(fallbackScope),
    };
  }

  if (current.id !== incoming.id) {
    return incoming;
  }

  const currentKind = normalizeConversationRecordKind(current.record_kind);
  const incomingKind = normalizeConversationRecordKind(incoming.record_kind);
  const fallbackKind = normalizeConversationRecordKind(fallbackScope);

  const preserveFinishedAt =
    current.finished_at &&
    !incoming.finished_at &&
    current.status !== "running" &&
    incoming.status !== "running";

  return {
    ...current,
    ...incoming,
    record_kind: incomingKind ?? currentKind ?? fallbackKind ?? null,
    finished_at: preserveFinishedAt
      ? current.finished_at
      : incoming.finished_at,
    result: incoming.result ?? current.result,
    error: incoming.error ?? current.error,
  };
}

export function latestConversationTask(
  current: ConversationSyncTaskSnapshot | null,
  incoming: ConversationSyncTaskSnapshot | null,
): ConversationSyncTaskSnapshot | null {
  if (!current) return incoming;
  if (!incoming) return current;
  if (current.id === incoming.id) {
    return mergeConversationTaskSnapshot(incoming, current);
  }

  const currentStartedAt = Date.parse(current.started_at);
  const incomingStartedAt = Date.parse(incoming.started_at);

  if (
    !Number.isNaN(currentStartedAt) &&
    !Number.isNaN(incomingStartedAt) &&
    incomingStartedAt >= currentStartedAt
  ) {
    return incoming;
  }

  return current;
}

function normalizeConversationRecordKind(
  kind: ConversationRecordKind | "all" | string | null | undefined,
): ConversationRecordKind | null {
  if (kind === "session" || kind === "web") {
    return kind;
  }
  return null;
}
