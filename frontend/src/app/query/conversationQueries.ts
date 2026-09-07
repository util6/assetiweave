import { queryOptions } from "@tanstack/react-query";
import {
  listConversationAdapters,
  listConversationSessions,
  listWebRecordSessions,
  type ConversationSessionListParams,
} from "../../services/conversations";
import type {
  ConversationAdapter,
  ConversationRecordKind,
  ConversationSessionListItem,
} from "../../types";
import type { QueryScope } from "./catalogQueries";

export const conversationKeys = {
  root: (scope: QueryScope) =>
    ["tenant", scope.tenantId, scope.epoch, "conversations"] as const,
  adapters: (scope: QueryScope, recordKind: ConversationRecordKind) =>
    [...conversationKeys.root(scope), "adapters", recordKind] as const,
  sessions: (
    scope: QueryScope,
    recordKind: ConversationRecordKind,
    search: string,
    page = 1,
    pageSize = 100,
  ) =>
    [
      ...conversationKeys.root(scope),
      "sessions",
      recordKind,
      search,
      page,
      pageSize,
    ] as const,
};

const DEFAULT_CONVERSATION_STALE_TIME = 1000 * 60;
export const SESSION_PAGE_SIZE = 100;

export type ListConversationSessionPage = (
  params: ConversationSessionListParams,
) => Promise<ConversationSessionListItem[]>;

export async function loadAllConversationSessionPages(
  listSessions: ListConversationSessionPage,
  query: string | null,
  pageSize = SESSION_PAGE_SIZE,
): Promise<ConversationSessionListItem[]> {
  const sessions: ConversationSessionListItem[] = [];
  for (let offset = 0; ; offset += pageSize) {
    const page = await listSessions({ query, limit: pageSize, offset });
    sessions.push(...page);
    if (page.length < pageSize) {
      return sessions;
    }
  }
}

export function conversationAdaptersQueryOptions(
  scope: QueryScope,
  recordKind: ConversationRecordKind,
) {
  return queryOptions({
    queryKey: conversationKeys.adapters(scope, recordKind),
    queryFn: async (): Promise<ConversationAdapter[]> => {
      const isWeb = recordKind === "web";
      const adapters = await listConversationAdapters();
      return adapters.filter(
        (adapter) => adapter.capabilities.includes("web_history") === isWeb,
      );
    },
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CONVERSATION_STALE_TIME,
  });
}

export function conversationSessionsQueryOptions(
  scope: QueryScope,
  recordKind: ConversationRecordKind,
  search: string,
) {
  return queryOptions({
    queryKey: conversationKeys.sessions(scope, recordKind, search),
    queryFn: async (): Promise<ConversationSessionListItem[]> => {
      const listSessions =
        recordKind === "web" ? listWebRecordSessions : listConversationSessions;
      return loadAllConversationSessionPages(
        listSessions,
        search.trim() || null,
      );
    },
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CONVERSATION_STALE_TIME,
  });
}
