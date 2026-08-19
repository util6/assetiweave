import { useCallback, useEffect, useState } from "react";

import { useConversationSync } from "../../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../../app/backgroundTasks/SearchIndexProvider";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import {
  DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  type ConversationContentVisibility,
  type ConversationRecordKind,
  type ConversationSearchCardType,
  type ConversationSearchHit,
  type ConversationSyncProgressState,
} from "../../types";

export type ConversationSessionView = "browser" | "detail";
export type ConversationSessionSortBy = "updated" | "started" | "title" | "question-count" | "turn-count";
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

export function useConversationsController({ recordKind }: { recordKind: ConversationRecordKind }) {
  const conversationSync = useConversationSync();
  const searchIndex = useSearchIndex();
  const { settings: appSettings } = useAppSettings();
  const webRecordMode = recordKind === "web";
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedQuestionId, setSelectedQuestionId] = useState<string | null>(null);
  const [sessionView, setSessionView] = useState<ConversationSessionView>("browser");
  const [contentVisibility, setContentVisibility] = useState<ConversationContentVisibility>({
    ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  });
  const [selectedQuestionIds, setSelectedQuestionIds] = useState<Set<string>>(() => new Set());
  const [exportDialog, setExportDialog] = useState<ConversationExportDialogState | null>(null);
  const [exportVisibility, setExportVisibility] = useState<ConversationContentVisibility>({
    ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  });
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [syncProgress, setSyncProgress] = useState<ConversationSyncProgressState | null>(null);
  const [syncProgressDismissed, setSyncProgressDismissed] = useState(false);
  const [query, setQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState(
    webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations",
  );
  const [exporting, setExporting] = useState(false);
  const [contentQuery, setContentQuery] = useState("");
  const [contentSearchCardKinds, setContentSearchCardKinds] = useState<string[]>([]);
  const [contentSearchSemanticRoles, setContentSearchSemanticRoles] = useState<string[]>([]);
  const [contentSearchIncludesQuestions, setContentSearchIncludesQuestions] = useState(true);
  const [sessionSortBy, setSessionSortBy] = useState<ConversationSessionSortBy>("updated");
  const [sessionSortDirection, setSessionSortDirection] = useState<"asc" | "desc">("desc");
  const [contentSearchResult, setContentSearchResult] = useState<ConversationSearchResultState | null>(null);
  const [contentSearchLoading, setContentSearchLoading] = useState(false);
  const [activeSearchTarget, setActiveSearchTarget] = useState<ConversationSearchTarget | null>(null);

  const clearSessionDetail = useCallback(() => {
    setSelectedQuestionId(null);
    setSelectedQuestionIds(new Set());
  }, []);

  const closeSession = useCallback(() => {
    clearSessionDetail();
    setActiveSearchTarget(null);
    setSelectedSessionId(null);
    setSessionView("browser");
  }, [clearSessionDetail]);

  const openSession = useCallback((sessionId: string) => {
    clearSessionDetail();
    setSelectedSessionId(sessionId);
    setActiveSearchTarget(null);
    setSessionView("detail");
  }, [clearSessionDetail]);

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

  const reconcileAppSelection = useCallback((appIds: readonly string[], fallbackAppId: string | null) => {
    setSelectedAppId((current) => current && appIds.includes(current) ? current : fallbackAppId);
  }, []);

  const reconcileProjectSelection = useCallback((projectKeys: readonly string[], fallbackProjectKey: string | null) => {
    setSelectedProjectKey((current) => current && projectKeys.includes(current) ? current : fallbackProjectKey);
  }, []);

  const reconcileSessionSelection = useCallback((sessionIds: readonly string[]) => {
    setSelectedSessionId((current) => current && sessionIds.includes(current) ? current : null);
  }, []);

  const reconcileQuestionSelection = useCallback((questionIds: readonly string[], preferredQuestionId?: string | null) => {
    const availableIds = new Set(questionIds);
    setSelectedQuestionId((current) => {
      const preferred = preferredQuestionId ?? current;
      return preferred && availableIds.has(preferred) ? preferred : questionIds[0] ?? null;
    });
    setSelectedQuestionIds((current) => {
      const next = new Set([...current].filter((questionId) => availableIds.has(questionId)));
      return next.size === current.size ? current : next;
    });
  }, []);

  const openConversationTarget = useCallback((target: ConversationOpenTarget) => {
    clearSessionDetail();
    setActiveSearchTarget(target.searchTarget ?? null);
    setSelectedAppId(target.appId);
    setSelectedProjectKey(target.projectKey);
    setSelectedQuestionId(target.questionId);
    setSelectedSessionId(target.sessionId);
    setSessionView("detail");
  }, [clearSessionDetail]);

  const focusSearchTarget = useCallback((target: ConversationSearchTarget, visibleType?: string) => {
    setActiveSearchTarget(target);
    setSelectedQuestionId(target.questionId);
    if (visibleType) {
      setContentVisibility((current) => ({ ...current, [visibleType]: true }));
    }
  }, []);

  const ensureContentVisibility = useCallback((types: readonly string[]) => {
    setContentVisibility((current) => {
      const next = { ...current };
      for (const type of types) {
        if (!(type in next)) next[type] = true;
      }
      return next;
    });
  }, []);

  const updateContentVisibility = useCallback((type: string, checked: boolean) => {
    setContentVisibility((current) => ({ ...current, [type]: checked }));
  }, []);

  const toggleQuestionSelection = useCallback((questionId: string, checked: boolean) => {
    setSelectedQuestionIds((current) => {
      const next = new Set(current);
      if (checked) next.add(questionId);
      else next.delete(questionId);
      return next;
    });
  }, []);

  const updateQuestionSelectionAfterMerge = useCallback((previousQuestionId: string, currentQuestionId: string) => {
    setSelectedQuestionIds((selectedIds) => {
      const next = new Set(selectedIds);
      if (next.delete(currentQuestionId)) {
        next.add(previousQuestionId);
      }
      return next;
    });
  }, []);

  const showAllContentSearchCardTypes = useCallback(() => {
    setContentSearchCardKinds([]);
    setContentSearchSemanticRoles([]);
    setContentSearchIncludesQuestions(true);
  }, []);

  const toggleContentSearchCardKind = useCallback((kind: string) => {
    setContentSearchCardKinds((current) => current.includes(kind)
      ? current.filter((candidate) => candidate !== kind)
      : [...current, kind]);
  }, []);

  const toggleContentSearchSemanticRole = useCallback((role: string) => {
    setContentSearchSemanticRoles((current) => current.includes(role)
      ? current.filter((candidate) => candidate !== role)
      : [...current, role]);
  }, []);

  useEffect(() => {
    setSelectedAppId(null);
    setSelectedProjectKey(null);
    setSelectedSessionId(null);
    clearSessionDetail();
    setSessionView("browser");
    setContentVisibility({ ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY });
    setContentQuery("");
    setContentSearchCardKinds([]);
    setContentSearchSemanticRoles([]);
    setContentSearchIncludesQuestions(true);
    setContentSearchResult(null);
    setContentSearchLoading(false);
    setActiveSearchTarget(null);
    setExportDialog(null);
    setImportDialogOpen(false);
    setSyncProgress(null);
    setSyncProgressDismissed(false);
    setOutputRoot(webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations");
  }, [clearSessionDetail, recordKind, webRecordMode]);

  return {
    activeSearchTarget,
    appSettings,
    clearSessionDetail,
    closeSession,
    contentQuery,
    contentSearchCardKinds,
    contentSearchIncludesQuestions,
    contentSearchLoading,
    contentSearchResult,
    contentSearchSemanticRoles,
    contentVisibility,
    conversationSync,
    ensureContentVisibility,
    exportDialog,
    exportVisibility,
    exporting,
    importDialogOpen,
    focusSearchTarget,
    openConversationTarget,
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
    setContentSearchIncludesQuestions,
    setContentSearchLoading,
    setContentSearchResult,
    setExportDialog,
    setExportVisibility,
    setExporting,
    setImportDialogOpen,
    setOutputRoot,
    setQuery,
    setSessionSortBy,
    setSessionSortDirection,
    setSyncProgress,
    setSyncProgressDismissed,
    showAllContentSearchCardTypes,
    showSessionBrowser,
    syncProgress,
    syncProgressDismissed,
    toggleContentSearchCardKind,
    toggleContentSearchSemanticRole,
    toggleQuestionSelection,
    updateQuestionSelectionAfterMerge,
    reconcileAppSelection,
    reconcileProjectSelection,
    reconcileQuestionSelection,
    reconcileSessionSelection,
    updateContentVisibility,
  };
}
