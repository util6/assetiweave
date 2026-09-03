import { queryOptions } from "@tanstack/react-query";
import {
  getConversationSearchIndexStatus,
  getConversationSearchIndexTask,
  type ConversationSearchIndexStatus,
  type ConversationSearchIndexTaskSnapshot,
} from "../../services/conversations";
import type { QueryScope } from "../query/catalogQueries";
import { taskKeys } from "../query/taskKeys";

export interface SearchIndexQueryState {
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
}

export function mergeSearchIndexQueryState(
  previous: SearchIndexQueryState | undefined,
  incoming: SearchIndexQueryState,
): SearchIndexQueryState {
  if (!previous) {
    return incoming;
  }

  const task = mergeTasks(previous.task, incoming.task);
  const status = mergeStatuses(previous.status, incoming.status);

  if (task === previous.task && status === previous.status) {
    return previous;
  }

  return { status, task };
}

function mergeTasks(
  previous: ConversationSearchIndexTaskSnapshot | null,
  incoming: ConversationSearchIndexTaskSnapshot | null,
): ConversationSearchIndexTaskSnapshot | null {
  // null 不擦掉尚在执行的当前任务
  if (!incoming) {
    return previous?.status === "running" ? previous : null;
  }
  if (!previous) {
    return incoming;
  }

  // 同 ID 终态不回退 running
  if (previous.id === incoming.id) {
    if (previous.status !== "running" && incoming.status === "running") {
      return previous;
    }
    return incoming;
  }

  // 不同 ID 比较 started_at，保留较新的任务
  if (previous.started_at && incoming.started_at) {
    const prevTime = new Date(previous.started_at).getTime();
    const incTime = new Date(incoming.started_at).getTime();
    if (prevTime > incTime) {
      return previous;
    }
    return incoming;
  }

  // Fallback: 如果一个正在运行，优先保留运行中任务
  if (previous.status === "running" && incoming.status !== "running") {
    return previous;
  }

  return incoming;
}

function mergeStatuses(
  previous: ConversationSearchIndexStatus | null,
  incoming: ConversationSearchIndexStatus | null,
): ConversationSearchIndexStatus | null {
  if (!incoming) return previous ?? null;
  if (!previous) return incoming;

  // status 用其 source_revision 保留最新快照
  if (
    typeof incoming.source_revision === "number" &&
    typeof previous.source_revision === "number"
  ) {
    return incoming.source_revision >= previous.source_revision
      ? incoming
      : previous;
  }

  return incoming;
}

export function searchIndexQueryOptions(scope: QueryScope) {
  return queryOptions<SearchIndexQueryState>({
    queryKey: taskKeys.resource(scope, "search-index"),
    queryFn: async () => {
      const [status, task] = await Promise.all([
        getConversationSearchIndexStatus(),
        getConversationSearchIndexTask(),
      ]);
      return { status, task };
    },
    structuralSharing: (oldData, newData) => {
      return mergeSearchIndexQueryState(
        oldData as SearchIndexQueryState | undefined,
        newData as SearchIndexQueryState,
      );
    },
    staleTime: 1000,
  });
}
