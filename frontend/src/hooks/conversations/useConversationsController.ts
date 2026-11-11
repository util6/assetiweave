import { useCallback, useEffect, useRef, useState } from "react";

import { useConversationSync } from "../../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../../app/backgroundTasks/SearchIndexProvider";
import { useAppSettings } from "../../store/settings/useAppSettings";
import {
  DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  type ConversationContentVisibility,
  type ConversationRecordKind,
  type ConversationSearchCardType,
  type ConversationSearchHit,
} from "../../types";

export type ConversationSessionView = "browser" | "detail";
export type ConversationSessionSortBy =
  "updated" | "started" | "title" | "question-count" | "turn-count";
export type ConversationQuestionSortBy = "index" | "updated" | "title";
export type ConversationExportMode = "session" | "questions";

export interface ConversationExportDialogState {
  mode: ConversationExportMode;
  questionIds: string[];
}

export interface ConversationSearchResultState {
  cardKinds: string[];
  semanticRoles: string[];
  includeQuestions: boolean;
  recordKind: ConversationRecordKind;
  query: string;
  totalCount: number;
  hits: ConversationSearchHit[];
}

export interface ConversationSearchTarget {
  blockId: string;
  cardType?: ConversationSearchCardType;
  questionId: string;
  sessionId: string;
}

export interface ConversationOpenTarget {
  appId: string;
  projectKey: string | null;
  searchTarget?: ConversationSearchTarget | null;
  sessionId: string;
  questionId: string | null;
}

export function useConversationsController({
  recordKind,
}: {
  recordKind: ConversationRecordKind;
}) {
  const conversationSync = useConversationSync();
  const searchIndex = useSearchIndex();
  const { settings: appSettings } = useAppSettings();
  const webRecordMode = recordKind === "web";
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(
    null,
  );
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    null,
  );
  /**
   * 子代理会话（`execution_origin = "subagent"`）默认是 `user_visible = 0`，
   * 不会出现在会话目录里。它只能通过父轮次锚点或检索命中按需打开，
   * 因此需要一个显式的“游离会话”标记，避免被目录对账立即清空选择。
   */
  const [detachedSessionId, setDetachedSessionId] = useState<string | null>(
    null,
  );
  const detachedSessionIdRef = useRef<string | null>(null);
  const [selectedQuestionId, setSelectedQuestionId] = useState<string | null>(
    null,
  );
  const [sessionView, setSessionView] =
    useState<ConversationSessionView>("browser");
  const [contentVisibility, setContentVisibility] =
    useState<ConversationContentVisibility>({
      ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
    });
  const [selectedQuestionIds, setSelectedQuestionIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [exportDialog, setExportDialog] =
    useState<ConversationExportDialogState | null>(null);
  const [exportVisibility, setExportVisibility] =
    useState<ConversationContentVisibility>({
      ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
    });
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState(
    webRecordMode
      ? "~/Desktop/assetiweave-web-records"
      : "~/Desktop/assetiweave-conversations",
  );
  const [exporting, setExporting] = useState(false);
  const [contentQuery, setContentQuery] = useState("");
  const [contentSearchCardKinds, setContentSearchCardKinds] = useState<
    string[]
  >([]);
  const [contentSearchSemanticRoles, setContentSearchSemanticRoles] = useState<
    string[]
  >([]);
  const [contentSearchIncludesQuestions, setContentSearchIncludesQuestions] =
    useState(true);
  const [contentSearchAdapterId, setContentSearchAdapterId] = useState<
    string | null
  >(null);
  const [sessionSortBy, setSessionSortBy] =
    useState<ConversationSessionSortBy>("updated");
  const [sessionSortDirection, setSessionSortDirection] = useState<
    "asc" | "desc"
  >("desc");
  const [contentSearchResult, setContentSearchResult] =
    useState<ConversationSearchResultState | null>(null);
  const [contentSearchLoading, setContentSearchLoading] = useState(false);
  const [contentSearchLoadingMore, setContentSearchLoadingMore] =
    useState(false);
  const [activeSearchTarget, setActiveSearchTarget] =
    useState<ConversationSearchTarget | null>(null);

  const clearSessionDetail = useCallback(() => {
    setSelectedQuestionId(null);
    setSelectedQuestionIds(new Set());
  }, []);

  const markDetached = useCallback((sessionId: string | null) => {
    detachedSessionIdRef.current = sessionId;
    setDetachedSessionId(sessionId);
  }, []);

  const closeSession = useCallback(() => {
    clearSessionDetail();
    setActiveSearchTarget(null);
    markDetached(null);
    setSelectedSessionId(null);
    setSessionView("browser");
  }, [clearSessionDetail, markDetached]);

  const openSession = useCallback(
    (sessionId: string) => {
      clearSessionDetail();
      markDetached(null);
      setSelectedSessionId(sessionId);
      setActiveSearchTarget(null);
      setSessionView("detail");
    },
    [clearSessionDetail, markDetached],
  );

  /**
   * 打开不在会话目录里的子代理会话：保留选择，不参与目录对账清理。
   */
  const openDetachedSession = useCallback(
    (sessionId: string) => {
      clearSessionDetail();
      markDetached(sessionId);
      setSelectedSessionId(sessionId);
      setActiveSearchTarget(null);
      setSessionView("detail");
    },
    [clearSessionDetail, markDetached],
  );

  const selectApp = useCallback((appId: string) => {
    setSelectedAppId(appId);
    setSelectedProjectKey(null);
  }, []);

  const selectProject = useCallback((projectKey: string | null) => {
    setSelectedProjectKey(projectKey);
  }, []);

  const selectQuestion = useCallback((questionId: string | null) => {
    setSelectedQuestionId(questionId);
  }, []);

  const showSessionBrowser = useCallback(() => {
    setSessionView("browser");
  }, []);

  const reconcileAppSelection = useCallback(
    (appIds: readonly string[], fallbackAppId: string | null) => {
      setSelectedAppId((current) =>
        current && appIds.includes(current) ? current : fallbackAppId,
      );
    },
    [],
  );

  const reconcileProjectSelection = useCallback(
    (projectKeys: readonly string[], fallbackProjectKey: string | null) => {
      setSelectedProjectKey((current) =>
        current && projectKeys.includes(current) ? current : fallbackProjectKey,
      );
    },
    [],
  );

  const reconcileSessionSelection = useCallback(
    (sessionIds: readonly string[]) => {
      setSelectedSessionId((current) => {
        if (current && sessionIds.includes(current)) return current;
        // 按需打开的子代理会话本来就不在目录里，保留它直到用户显式返回。
        if (current && current === detachedSessionIdRef.current) return current;
        return null;
      });
    },
    [],
  );

  const reconcileQuestionSelection = useCallback(
    (questionIds: readonly string[], preferredQuestionId?: string | null) => {
      const availableIds = new Set(questionIds);
      setSelectedQuestionId((current) => {
        const preferred = preferredQuestionId ?? current;
        return preferred && availableIds.has(preferred)
          ? preferred
          : (questionIds[0] ?? null);
      });
      setSelectedQuestionIds((current) => {
        const next = new Set(
          [...current].filter((questionId) => availableIds.has(questionId)),
        );
        return next.size === current.size ? current : next;
      });
    },
    [],
  );

  const openConversationTarget = useCallback(
    (target: ConversationOpenTarget) => {
      clearSessionDetail();
      markDetached(null);
      setActiveSearchTarget(target.searchTarget ?? null);
      setSelectedAppId(target.appId);
      setSelectedProjectKey(target.projectKey);
      setSelectedQuestionId(target.questionId);
      setSelectedSessionId(target.sessionId);
      setSessionView("detail");
    },
    [clearSessionDetail, markDetached],
  );

  const focusSearchTarget = useCallback(
    (target: ConversationSearchTarget, visibleType?: string) => {
      setActiveSearchTarget(target);
      setSelectedQuestionId(target.questionId);
      if (visibleType) {
        setContentVisibility((current) => ({
          ...current,
          [visibleType]: true,
        }));
      }
    },
    [],
  );

  const ensureContentVisibility = useCallback((types: readonly string[]) => {
    setContentVisibility((current) => {
      const next = { ...current };
      for (const type of types) {
        if (!(type in next)) next[type] = true;
      }
      return next;
    });
  }, []);

  const updateContentVisibility = useCallback(
    (type: string, checked: boolean) => {
      setContentVisibility((current) => ({ ...current, [type]: checked }));
    },
    [],
  );

  const toggleQuestionSelection = useCallback(
    (questionId: string, checked: boolean) => {
      setSelectedQuestionIds((current) => {
        const next = new Set(current);
        if (checked) next.add(questionId);
        else next.delete(questionId);
        return next;
      });
    },
    [],
  );

  const showAllContentSearchCardTypes = useCallback(() => {
    setContentSearchCardKinds([]);
    setContentSearchSemanticRoles([]);
    setContentSearchIncludesQuestions(true);
  }, []);

  const toggleContentSearchCardKind = useCallback((kind: string) => {
    setContentSearchCardKinds((current) =>
      current.includes(kind)
        ? current.filter((candidate) => candidate !== kind)
        : [...current, kind],
    );
  }, []);

  const toggleContentSearchSemanticRole = useCallback((role: string) => {
    setContentSearchSemanticRoles((current) =>
      current.includes(role)
        ? current.filter((candidate) => candidate !== role)
        : [...current, role],
    );
  }, []);

  useEffect(() => {
    setSelectedAppId(null);
    setSelectedProjectKey(null);
    setSelectedSessionId(null);
    detachedSessionIdRef.current = null;
    setDetachedSessionId(null);
    clearSessionDetail();
    setSessionView("browser");
    setContentVisibility({ ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY });
    setContentQuery("");
    setContentSearchAdapterId(null);
    setContentSearchCardKinds([]);
    setContentSearchSemanticRoles([]);
    setContentSearchIncludesQuestions(true);
    setContentSearchResult(null);
    setContentSearchLoading(false);
    setContentSearchLoadingMore(false);
    setActiveSearchTarget(null);
    setExportDialog(null);
    setImportDialogOpen(false);
    setOutputRoot(
      webRecordMode
        ? "~/Desktop/assetiweave-web-records"
        : "~/Desktop/assetiweave-conversations",
    );
  }, [clearSessionDetail, recordKind, webRecordMode]);

  return {
    activeSearchTarget,
    appSettings,
    clearSessionDetail,
    closeSession,
    contentQuery,
    contentSearchAdapterId,
    contentSearchCardKinds,
    contentSearchIncludesQuestions,
    contentSearchLoading,
    contentSearchLoadingMore,
    contentSearchResult,
    contentSearchSemanticRoles,
    contentVisibility,
    conversationSync,
    detachedSessionId,
    ensureContentVisibility,
    exportDialog,
    exportVisibility,
    exporting,
    importDialogOpen,
    focusSearchTarget,
    openConversationTarget,
    openDetachedSession,
    openSession,
    outputRoot,
    query,
    searchIndex,
    selectApp,
    selectProject,
    selectQuestion,
    selectedAppId,
    selectedProjectKey,
    selectedQuestionId,
    selectedQuestionIds,
    selectedSessionId,
    sessionSortBy,
    sessionSortDirection,
    sessionView,
    setContentQuery,
    setContentSearchAdapterId,
    setContentSearchCardKinds,
    setContentSearchIncludesQuestions,
    setContentSearchLoading,
    setContentSearchLoadingMore,
    setContentSearchResult,
    setContentSearchSemanticRoles,
    setExportDialog,
    setExportVisibility,
    setExporting,
    setImportDialogOpen,
    setOutputRoot,
    setQuery,
    setSessionSortBy,
    setSessionSortDirection,
    showAllContentSearchCardTypes,
    showSessionBrowser,
    toggleContentSearchCardKind,
    toggleContentSearchSemanticRole,
    toggleQuestionSelection,
    reconcileAppSelection,
    reconcileProjectSelection,
    reconcileQuestionSelection,
    reconcileSessionSelection,
    updateContentVisibility,
  };
}
