import clsx from "clsx";
import { memo, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  AppWindow,
  ArrowDownWideNarrow,
  ArrowLeft,
  ChevronRight,
  Download,
  Folder,
  GitMerge,
  Layers3,
  PackageCheck,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Settings,
  X,
} from "lucide-react";
import {
  DataToolbar,
  DebouncedToolbarSearch,
  ToolbarActionButton,
  ToolbarSingleSelectDropdown,
  ToolbarSortDirectionButton,
  ToolbarTextButton,
} from "../../components/common/DataToolbar";
import { PageMetrics } from "../../components/common/PageMetrics";
import {
  RenderActivityProvider,
  RenderSafeScrollSurface,
  VirtualizedCollection,
  renderingFlags,
  type VirtualizedCollectionHandle,
} from "../../components/common/rendering";
import { PathPickerInput } from "../../components/common/PathPickerInput";
import {
  AppShortcutIconForShortcut,
  APP_SHORTCUT_ICON_FRAME_CLASS,
} from "../../components/apps/AppShortcutIcon";
import type { NotificationMessage } from "../../components/notifications/NotificationBanner";
import {
  buildConversationContentBlocks,
  conversationCardColor,
  conversationCardLabel,
  conversationCardDomId,
  DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  type ConversationContentType,
  type ConversationContentVisibility,
} from "../../components/conversations/ConversationContentCards";
import {
  buildConversationBlockTurnIndex,
  buildConversationTurnPresentations,
  collectConversationTurnBlocks,
  ConversationTurn,
} from "../../components/conversations/ConversationTurn";
import { DEFAULT_TRANSLATION_SETTINGS, useConversationContentController } from "../../components/conversations/useConversationContentController";
import { MarkdownContent } from "../../components/conversations/ConversationMarkdown";
import {
  ConversationCardKindIcon,
  conversationCardPresentationKind,
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "../../components/conversations/ConversationCardKindRegistry";
import {
  ConversationContentFilter,
  ConversationSyncProgress,
} from "../../components/conversations/ConversationToolbarControls";
import { ConversationImportDialog } from "../../components/conversations/ConversationImportDialog";
import {
  ConversationLoadingState,
  ConversationPreviewLoadingState,
  ConversationTurnSkeleton,
} from "../../components/conversations/ConversationSkeleton";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { ResizableColumns } from "../../components/layout/ResizableColumns";
import { PageHeader } from "../../components/foundation/PageHeader";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import { loadSharedResource, readSharedResource } from "../../lib/asyncCache";
import type { TranslationKey } from "../../i18n/messages";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import { DEFAULT_COLUMN_MIN_WIDTH } from "../../store/settings/settingsSchema";
import {
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  resolveAgentCapability,
  resolveFontFamilyCss,
  type ConversationContentCardColorSettings,
  type ResolvedConversationTranslationSettings,
  type SettingsPanelId,
} from "../../store/settings/AppSettingsProvider";
import {
  exportConversationSession,
  exportWebRecordSession,
  getConversationSession,
  getWebRecordSession,
  listConversationAdapters,
  listConversationSessions,
  listWebRecordSessions,
  mergeConversationQuestions,
  searchConversationRecords,
  splitConversationQuestion,
  summarizeConversationSyncTask,
  type ConversationSyncSummaryCounts,
  type ConversationSyncTaskSnapshot,
} from "../../services/conversations";
import { selectTargetDirectory } from "../../services/catalog";
import {
  useConversationsController,
  type ConversationExportDialogState,
  type ConversationExportMode,
  type ConversationQuestionSortBy,
  type ConversationSearchResultState,
  type ConversationSearchTarget,
  type ConversationSessionSortBy,
} from "../../hooks/conversations/useConversationsController";
import { iconButtonRecipe } from "../../theme/recipes";
import type {
  AppKind,
  AppShortcut,
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSearchCardType,
  ConversationSearchHit,
  ConversationSearchResult,
  ConversationRecordKind,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import type { ConversationNavigationTarget } from "../../router/navigationTargets";

export { MarkdownContent } from "../../components/conversations/ConversationMarkdown";

const SESSION_PAGE_SIZE = 100;
const SESSION_SEARCH_COMMIT_DELAY_MS = 700;
const CONTENT_SEARCH_COMMIT_DELAY_MS = 700;
const CONVERSATION_SHORT_ID_PATTERN = /^[0-9a-f]{8}$/i;

export function isConversationShortIdQuery(value: string) {
  return CONVERSATION_SHORT_ID_PATTERN.test(value.trim());
}
const DISMISSED_SYNC_PROGRESS_TASK_LIMIT = 50;
const dismissedConversationSyncProgressTaskKeys = new Set<string>();

type ListConversationSessionPage = (params: {
  query?: string | null;
  limit?: number;
  offset?: number;
}) => Promise<ConversationSessionListItem[]>;

interface ConversationSearchAppChipMeta {
  accentColor?: string | null;
  name: string;
}

type ConversationPageNotification = Omit<NotificationMessage, "id">;

export async function loadAllConversationSessionPages(
  listSessions: ListConversationSessionPage,
  query: string | null,
  pageSize = SESSION_PAGE_SIZE,
) {
  const sessions: ConversationSessionListItem[] = [];
  for (let offset = 0; ; offset += pageSize) {
    const page = await listSessions({ query, limit: pageSize, offset });
    sessions.push(...page);
    if (page.length < pageSize) {
      return sessions;
    }
  }
}

function conversationAdapterCacheKey(recordKind: ConversationRecordKind) {
  return `conversation.adapters.${recordKind}`;
}

function conversationSessionCacheKey(recordKind: ConversationRecordKind, query: string) {
  return `conversation.sessions.${recordKind}.${query}`;
}

export function ConversationsPage({
  appShortcuts,
  navigationTarget,
  onManualOpen,
  onNavigationTargetConsumed,
  onNotify,
  onNotifyError,
  onOpenSettings,
  onReady,
  recordKind = "session",
}: {
  activeSubNavId?: string;
  appShortcuts: AppShortcut[];
  navigationTarget?: ConversationNavigationTarget | null;
  onManualOpen: () => void;
  onNavigationTargetConsumed?: (nonce: string) => void;
  onNotify: (notification: ConversationPageNotification) => void;
  onNotifyError: (message: string) => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
  onReady?: () => void;
  recordKind?: "session" | "web";
}) {
  const { t } = useI18n();
  const currentRecordKind: ConversationRecordKind = recordKind;
  const {
    activeSearchTarget,
    appSettings,
    clearSessionDetail: clearConversationSelection,
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
    focusSearchTarget,
    importDialogOpen,
    openConversationTarget,
    openSession,
    outputRoot,
    query,
    reconcileAppSelection,
    reconcileProjectSelection,
    reconcileQuestionSelection,
    reconcileSessionSelection,
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
    updateContentVisibility,
  } = useConversationsController({ recordKind: currentRecordKind });
  const { startSync, taskFor } = conversationSync;
  const { rebuild: rebuildSearchIndex, status: searchIndexStatus, task: searchIndexTask } = searchIndex;
  const syncTask = taskFor(currentRecordKind);
  const webRecordMode = currentRecordKind === "web";
  const [adapters, setAdapters] = useState<ConversationAdapter[]>(
    () => readSharedResource<ConversationAdapter[]>(conversationAdapterCacheKey(currentRecordKind)) ?? [],
  );
  const [sessions, setSessions] = useState<ConversationSessionListItem[]>(
    () => readSharedResource<ConversationSessionListItem[]>(conversationSessionCacheKey(currentRecordKind, "")) ?? [],
  );
  const [sessionDetail, setSessionDetail] = useState<ConversationSessionDetail | null>(null);
  const handledSyncTaskIdRef = useRef<string | null>(null);
  const sessionSearchRequestIdRef = useRef(0);
  const syncRunning = syncTask?.status === "running";
  const searchIndexRunning = searchIndexTask?.status === "running";
  const [sessionSearchLoading, setSessionSearchLoading] = useState(false);
  const sessionDetailRequestIdRef = useRef(0);
  const [sessionCatalogReady, setSessionCatalogReady] = useState(
    () => readSharedResource<ConversationSessionListItem[]>(conversationSessionCacheKey(currentRecordKind, "")) !== undefined,
  );
  const importedSourceNamesRef = useRef<Map<string, string>>(new Map());
  const startedNavigationNonceRef = useRef<string | null>(null);
  const consumedNavigationNonceRef = useRef<string | null>(null);
  const previousQueryRef = useRef(query);

  function clearSessionDetail() {
    sessionDetailRequestIdRef.current += 1;
    setSessionDetail(null);
    clearConversationSelection();
  }

  const sessionQuestionCount = useMemo(() => sessions.reduce((total, session) => total + session.question_count, 0), [sessions]);
  const sortedSessions = useMemo(
    () => sortConversationSessions(sessions, sessionSortBy, sessionSortDirection),
    [sessionSortBy, sessionSortDirection, sessions],
  );
  const appGroups = useMemo(() => groupConversationSessionsByApp(adapters, sortedSessions), [adapters, sortedSessions]);
  const appMetaById = useMemo(
    () =>
      new Map(
        appGroups.map((group) => {
          const shortcut = findConversationAppShortcut(appShortcuts, group.app);
          return [
            group.app.id,
            {
              accentColor: shortcut?.accentColor ?? null,
              name: group.app.name,
            },
          ] as const;
        }),
      ),
    [appGroups, appShortcuts],
  );
  const selectedAppGroup = useMemo(
    () => appGroups.find((group) => group.app.id === selectedAppId) ?? null,
    [appGroups, selectedAppId],
  );
  const selectedQuestion = useMemo(
    () => sessionDetail?.questions.find((question) => question.question.id === selectedQuestionId) ?? null,
    [selectedQuestionId, sessionDetail],
  );
  const availableContentTypes = useMemo(
    () => conversationContentTypesForQuestions(
      selectedQuestion ? [selectedQuestion] : sessionDetail?.questions ?? [],
    ),
    [selectedQuestion, sessionDetail],
  );
  const visibleSessionQuestions = useMemo(
    () => sortConversationQuestions(sessionDetail?.questions ?? [], "index", "asc"),
    [sessionDetail],
  );
  const selectedQuestionCount = selectedQuestionIds.size;
  const exportAvailableContentTypes = useMemo(() => {
    if (!exportDialog || !sessionDetail) return [];
    const selectedIds = new Set(exportDialog.questionIds);
    const questions = exportDialog.mode === "questions"
      ? sessionDetail.questions.filter((question) => selectedIds.has(question.question.id))
      : sessionDetail.questions;
    return conversationContentTypesForQuestions(questions);
  }, [exportDialog, sessionDetail]);
  const conversationStyle = useMemo(
    () =>
      ({
        "--conversation-session-browser-font-family":
          resolveFontFamilyCss(appSettings.conversations.sessionBrowserFontFamily, "sans"),
        "--conversation-session-browser-font-size": `${appSettings.conversations.sessionBrowserFontSize}px`,
        "--conversation-content-font-family": resolveFontFamilyCss(appSettings.conversations.contentFontFamily, "sans"),
        "--conversation-content-font-size": `${appSettings.conversations.contentFontSize}px`,
        "--conversation-code-font-size": `${appSettings.conversations.codeFontSize}px`,
      }) as CSSProperties,
    [appSettings.conversations],
  );

  useEffect(() => {
    setSessionDetail(null);
    sessionSearchRequestIdRef.current += 1;
    setSessionSearchLoading(false);
    setSessionCatalogReady(false);
    sessionDetailRequestIdRef.current += 1;
    handledSyncTaskIdRef.current = null;
    void refreshCatalog();
  }, [currentRecordKind]);

  useEffect(() => () => {
    sessionDetailRequestIdRef.current += 1;
  }, []);

  useEffect(() => {
    if (sessionCatalogReady) {
      onReady?.();
    }
  }, [sessionCatalogReady]);

  useEffect(() => {
    if (previousQueryRef.current === query) {
      return;
    }
    previousQueryRef.current = query;
    void refreshSessions();
  }, [query]);

  useEffect(() => {
    const kinds = sessionDetail?.questions.flatMap((question) =>
      question.cards?.map((card) => conversationCardPresentationKind(card.kind, card.semantic_role)) ?? []
    ) ?? [];
    if (kinds.length === 0) return;
    ensureContentVisibility(kinds);
  }, [ensureContentVisibility, sessionDetail]);

  useEffect(() => {
    const trimmedQuery = contentQuery.trim();
    if (!trimmedQuery) {
      setContentSearchResult(null);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;
    setContentSearchLoading(true);
    void searchConversationRecords({
      content_types: [],
      card_kinds: contentSearchCardKinds,
      semantic_roles: contentSearchSemanticRoles,
      include_questions: contentSearchIncludesQuestions,
      include_cards: true,
      limit: 50,
      query: trimmedQuery,
      record_kind: currentRecordKind,
    })
      .then((result) => {
        if (cancelled) return;
        setContentSearchResult({
          cardKinds: conversationSearchCardKinds(result),
          semanticRoles: Object.keys(result.semantic_role_counts ?? {}),
          includeQuestions: result.scope?.include_questions ?? contentSearchIncludesQuestions,
          hits: result.hits,
          recordKind: result.record_kind,
          query: result.query,
          totalCount: result.total_count,
        });
      })
      .catch((error) => {
        if (!cancelled) {
          setContentSearchResult(null);
          onNotifyError(errorMessage(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setContentSearchLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    contentQuery,
    contentSearchCardKinds,
    contentSearchSemanticRoles,
    contentSearchIncludesQuestions,
    currentRecordKind,
    onNotifyError,
  ]);

  useEffect(() => {
    reconcileAppSelection(
      appGroups.map((group) => group.app.id),
      appGroups.find((group) => group.sessions.length > 0)?.app.id ?? appGroups[0]?.app.id ?? null,
    );
  }, [appGroups, reconcileAppSelection]);

  useEffect(() => {
    if (!selectedAppGroup || !selectedSessionId) return;
    if (!selectedAppGroup.sessions.some((session) => session.id === selectedSessionId)) {
      closeSession();
    }
  }, [closeSession, selectedAppGroup, selectedSessionId]);

  useEffect(() => {
    if (!selectedAppGroup) {
      selectProject(null);
      return;
    }
    if (webRecordMode) {
      selectProject(null);
      return;
    }
    reconcileProjectSelection(
      selectedAppGroup.projectGroups.map((group) => group.key),
      selectedAppGroup.projectGroups[0]?.key ?? null,
    );
  }, [reconcileProjectSelection, selectProject, selectedAppGroup, webRecordMode]);

  useEffect(() => {
    if (!selectedSessionId) {
      clearSessionDetail();
      return;
    }
    void loadSession(selectedSessionId);
  }, [selectedSessionId]);

  useEffect(() => {
    if (!selectedSessionId && sessionView === "detail") {
      showSessionBrowser();
    }
  }, [selectedSessionId, sessionView, showSessionBrowser]);

  useEffect(() => {
    window.scrollTo({ top: 0, behavior: "auto" });
  }, [sessionView]);

  useEffect(() => {
    if (!sessionDetail) {
      clearConversationSelection();
      return;
    }
    reconcileQuestionSelection(
      sessionDetail.questions.map((question) => question.question.id),
      preferredConversationQuestionId(sessionDetail.questions, selectedQuestionId),
    );
  }, [clearConversationSelection, reconcileQuestionSelection, selectedQuestionId, sessionDetail]);

  useEffect(() => {
    if (!activeSearchTarget || sessionDetail?.session.id !== activeSearchTarget.sessionId) {
      return;
    }
    if (sessionDetail.questions.some((question) => question.question.id === activeSearchTarget.questionId)) {
      selectQuestion(activeSearchTarget.questionId);
    }
  }, [activeSearchTarget, selectQuestion, sessionDetail]);

  const consumeNavigationTarget = useCallback((nonce: string) => {
    if (consumedNavigationNonceRef.current === nonce) return;
    consumedNavigationNonceRef.current = nonce;
    onNavigationTargetConsumed?.(nonce);
  }, [onNavigationTargetConsumed]);

  useEffect(() => {
    if (
      !navigationTarget ||
      navigationTarget.recordKind !== currentRecordKind ||
      startedNavigationNonceRef.current === navigationTarget.nonce ||
      consumedNavigationNonceRef.current === navigationTarget.nonce ||
      !sessionCatalogReady
    ) {
      return;
    }
    const targetSession = sessions.find((session) => session.id === navigationTarget.sessionId);
    if (!targetSession) {
      onNotifyError(t("conversation.navigation.sessionMissing"));
      consumeNavigationTarget(navigationTarget.nonce);
      return;
    }

    startedNavigationNonceRef.current = navigationTarget.nonce;
    openConversationTarget({
      appId: targetSession.adapter_id,
      projectKey: currentRecordKind === "web"
        ? null
        : normalizedProjectPath(targetSession) ?? NO_PROJECT_GROUP_KEY,
      questionId: navigationTarget.questionId ?? null,
      searchTarget: navigationTarget.blockId && navigationTarget.questionId
        ? {
            blockId: navigationTarget.blockId,
            questionId: navigationTarget.questionId,
            sessionId: navigationTarget.sessionId,
          }
        : null,
      sessionId: targetSession.id,
    });
  }, [
    consumeNavigationTarget,
    currentRecordKind,
    navigationTarget,
    onNotifyError,
    openConversationTarget,
    sessionCatalogReady,
    sessions,
    t,
  ]);

  useEffect(() => {
    if (
      !navigationTarget ||
      navigationTarget.recordKind !== currentRecordKind ||
      consumedNavigationNonceRef.current === navigationTarget.nonce ||
      startedNavigationNonceRef.current !== navigationTarget.nonce ||
      sessionDetail?.session.id !== navigationTarget.sessionId
    ) {
      return;
    }
    const resolved = resolveConversationNavigationTarget(sessionDetail, navigationTarget);
    if (!resolved) {
      onNotifyError(t("conversation.navigation.questionMissing"));
      consumeNavigationTarget(navigationTarget.nonce);
      return;
    }

    selectQuestion(resolved.questionId);
    if (navigationTarget.blockId) {
      if (!resolved.blockFound) {
        onNotifyError(t("conversation.navigation.blockMissing"));
      } else {
        const searchTarget = {
          blockId: resolved.blockId ?? navigationTarget.blockId,
          cardType: resolved.cardType ?? undefined,
          questionId: resolved.questionId,
          sessionId: navigationTarget.sessionId,
        };
        focusSearchTarget(
          searchTarget,
          resolved.cardType && resolved.cardType !== "question"
            ? conversationCardPresentationKind(resolved.cardType)
            : undefined,
        );
      }
    }
    consumeNavigationTarget(navigationTarget.nonce);
  }, [
    consumeNavigationTarget,
    currentRecordKind,
    focusSearchTarget,
    navigationTarget,
    onNotifyError,
    selectQuestion,
    sessionDetail,
    t,
  ]);

  useEffect(() => {
    if (!syncTask) {
      return;
    }
    if (syncTask.record_kind && syncTask.record_kind !== currentRecordKind) {
      return;
    }
    if (
      syncTask.status === "completed" &&
      dismissedConversationSyncProgressTaskKeys.has(
        conversationSyncProgressTaskKey(currentRecordKind, syncTask.id),
      )
    ) {
      handledSyncTaskIdRef.current = syncTask.id;
      setSyncProgress(null);
      setSyncProgressDismissed(true);
      return;
    }

    const sourceLabel = syncSourceLabel(syncTask.source_id);
    if (syncTask.status === "running") {
      setSyncProgressDismissed(false);
      setSyncProgress({ phase: "importing", sourceLabel, taskId: syncTask.id });
      return;
    }
    if (handledSyncTaskIdRef.current === syncTask.id) {
      return;
    }
    handledSyncTaskIdRef.current = syncTask.id;

    if (syncTask.status === "failed") {
      setSyncProgress({ failedStep: 2, phase: "failed", sourceLabel, taskId: syncTask.id });
      onNotifyError(syncTask.error ?? t("conversation.sync.description.failed"));
      return;
    }

    const summaryCounts = summarizeConversationSyncTask(syncTask);
    const summary = formatConversationSyncSummary(summaryCounts, t, currentRecordKind);
    const advice = formatConversationSyncAdvice(summaryCounts, t, currentRecordKind);
    const failureItems = formatConversationSyncFailureItems(syncTask, syncSourceLabel, t);
    let cancelled = false;
    setSyncProgress({ advice, failureItems, phase: "refreshing", sourceLabel, summary, taskId: syncTask.id });
    void refreshCatalog({ rethrow: true })
      .then(() => {
        if (cancelled) {
          return;
        }
        setSyncProgress({ advice, failureItems, phase: "completed", sourceLabel, summary, taskId: syncTask.id });
      })
      .catch((error) => {
        if (!cancelled) {
          setSyncProgress({ failedStep: 3, phase: "failed", sourceLabel, taskId: syncTask.id });
          onNotifyError(errorMessage(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [currentRecordKind, syncTask?.id, syncTask?.record_kind, syncTask?.source_id, syncTask?.status]);

  function syncSourceLabel(sourceId: string | null | undefined) {
    if (!sourceId) {
      return t("conversation.sync.allSources");
    }
    return importedSourceNamesRef.current.get(sourceId) ?? sourceId;
  }

  async function refreshCatalog(options: { rethrow?: boolean } = {}) {
    try {
      const nextAdapters = await loadSharedResource(
        conversationAdapterCacheKey(currentRecordKind),
        async () =>
          (await listConversationAdapters()).filter(
            (adapter) => isWebRecordAdapter(adapter) === webRecordMode,
          ),
        { force: true },
      );
      setAdapters(nextAdapters);
      await refreshSessions({ rethrow: true });
    } catch (error) {
      if (options.rethrow) throw error;
      onNotifyError(errorMessage(error));
    }
  }

  async function refreshSessions(options: { rethrow?: boolean } = {}) {
    const requestId = sessionSearchRequestIdRef.current + 1;
    sessionSearchRequestIdRef.current = requestId;
    setSessionSearchLoading(true);
    try {
      const listSessions = webRecordMode ? listWebRecordSessions : listConversationSessions;
      const nextSessions = await loadSharedResource(
        conversationSessionCacheKey(currentRecordKind, query),
        () => loadAllConversationSessionPages(listSessions, query || null),
        { force: true },
      );
      if (sessionSearchRequestIdRef.current !== requestId) return;
      setSessions(nextSessions);
      reconcileSessionSelection(nextSessions.map((session) => session.id));
    } catch (error) {
      if (options.rethrow) throw error;
      if (sessionSearchRequestIdRef.current === requestId) {
        onNotifyError(errorMessage(error));
      }
    } finally {
      if (sessionSearchRequestIdRef.current === requestId) {
        setSessionSearchLoading(false);
        setSessionCatalogReady(true);
      }
    }
  }

  async function loadSession(sessionId: string) {
    const requestId = sessionDetailRequestIdRef.current + 1;
    sessionDetailRequestIdRef.current = requestId;
    setSessionDetail(null);
    clearConversationSelection();

    try {
      const getSession = webRecordMode ? getWebRecordSession : getConversationSession;
      const detail = await getSession(sessionId);
      if (sessionDetailRequestIdRef.current !== requestId) return;
      setSessionDetail(detail);
    } catch (error) {
      if (sessionDetailRequestIdRef.current === requestId) {
        onNotifyError(errorMessage(error));
      }
    }
  }

  async function handleSync() {
    const sourceLabel = t("conversation.sync.allSources");
    setSyncProgressDismissed(false);
    handledSyncTaskIdRef.current = null;
    setSyncProgress({ phase: "preparing", sourceLabel });

    try {
      const task = await startSync({
        dry_run: false,
        record_kind: currentRecordKind,
        source_id: null,
      });
      const summaryCounts = summarizeConversationSyncTask(task);
      const summary = formatConversationSyncSummary(
        summaryCounts,
        t,
        currentRecordKind,
      );
      const advice = formatConversationSyncAdvice(
        summaryCounts,
        t,
        currentRecordKind,
      );
      const failureItems = formatConversationSyncFailureItems(task, syncSourceLabel, t);
      setSyncProgress({
        advice,
        failedStep: task.status === "failed" ? 2 : undefined,
        failureItems,
        phase:
          task.status === "failed"
            ? "failed"
            : task.status === "completed"
              ? "refreshing"
              : "importing",
        sourceLabel,
        summary,
        taskId: task.id,
      });
    } catch (error) {
      setSyncProgress({ failedStep: 1, phase: "failed", sourceLabel });
      onNotifyError(errorMessage(error));
    }
  }

  async function handleSearchIndexRebuild() {
    try {
      await rebuildSearchIndex();
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleMerge(previous: ConversationQuestionDetail, current: ConversationQuestionDetail) {
    try {
      await mergeConversationQuestions([previous.question.id, current.question.id], false);
      selectQuestion(previous.question.id);
      updateQuestionSelectionAfterMerge(previous.question.id, current.question.id);
      onNotify({ messageKey: "conversation.status.merged", tone: "success" });
      if (selectedSessionId) await loadSession(selectedSessionId);
      await refreshSessions();
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleSplit(question: ConversationQuestionDetail, turnId: string) {
    try {
      await splitConversationQuestion(question.question.id, turnId, false);
      onNotify({ messageKey: "conversation.status.split", tone: "success" });
      if (selectedSessionId) await loadSession(selectedSessionId);
      await refreshSessions();
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  function openExportDialog(mode: ConversationExportMode, questionIds: string[] = []) {
    setExportVisibility({ ...contentVisibility });
    setExportDialog({ mode, questionIds });
  }

  async function handleConfirmExport() {
    if (!selectedSessionId || !exportDialog) return;
    const questionIds = exportDialog.questionIds;
    setExporting(true);
    try {
      const exportSession = webRecordMode ? exportWebRecordSession : exportConversationSession;
      await exportSession(selectedSessionId, outputRoot, false, questionIds, exportVisibility);
      onNotify(
        questionIds.length > 0
          ? {
              messageKey: "conversation.status.exportedSelected",
              messageParams: { count: questionIds.length },
              tone: "success",
            }
          : { messageKey: "conversation.status.exported", tone: "success" },
      );
      setExportDialog(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setExporting(false);
    }
  }

  const handleOpenSession = useCallback((sessionId: string) => {
    openSession(sessionId);
    if (sessionId === selectedSessionId) {
      void loadSession(sessionId);
    }
  }, [openSession, selectedSessionId]);

  const handleOpenSearchHit = useCallback((hit: ConversationSearchHit) => {
    openConversationTarget({
      appId: hit.session.adapter_id,
      projectKey: currentRecordKind === "web" ? null : normalizedProjectPath(hit.session) ?? NO_PROJECT_GROUP_KEY,
      questionId: hit.question_id,
      searchTarget: {
        blockId: hit.block_id,
        cardType: hit.card_type,
        questionId: hit.question_id,
        sessionId: hit.session.id,
      },
      sessionId: hit.session.id,
    });
    if (hit.card_type !== "question") {
      updateContentVisibility(hit.card_type, true);
    }
    if (hit.session.id === selectedSessionId) {
      void loadSession(hit.session.id);
    }
  }, [currentRecordKind, openConversationTarget, selectedSessionId, updateContentVisibility]);

  const handleQuestionSelectionChange = toggleQuestionSelection;

  function handleBulkExport() {
    if (!sessionDetail || selectedQuestionIds.size === 0) return;
    const questionIds = sessionDetail.questions
      .filter((question) => selectedQuestionIds.has(question.question.id))
      .map((question) => question.question.id);
    openExportDialog("questions", questionIds);
  }

  function handleDismissSyncProgress() {
    if (syncProgress?.phase === "completed" && syncProgress.taskId) {
      rememberDismissedConversationSyncProgressTask(currentRecordKind, syncProgress.taskId);
    }
    setSyncProgressDismissed(true);
    setSyncProgress(null);
  }

  return (
    <ConversationShell
      headerActions={
        sessionView === "browser" ? (
          <PageMetrics
            metrics={[
              { label: t("conversation.toolbar.apps"), value: appGroups.length },
              { label: t("conversation.toolbar.sessions"), value: sessions.length },
              { label: t("conversation.toolbar.questions"), value: sessionQuestionCount },
              {
                label: t("conversation.searchIndex.metric"),
                value: searchIndexRunning
                  ? t("conversation.searchIndex.building")
                  : (searchIndexStatus?.health ?? t("common.loading")),
              },
            ]}
          />
        ) : (
          <PageMetrics
            metrics={[
              { label: t("conversation.toolbar.questions"), value: sessionDetail?.questions.length ?? 0 },
              { label: t("conversation.toolbar.selected"), value: selectedQuestionCount },
            ]}
          />
        )
      }
      style={conversationStyle}
      title={t(webRecordMode ? "conversation.webRecords.title" : "conversation.sessions.title")}
      subtitle={t(webRecordMode ? "conversation.webRecords.subtitle" : "conversation.sessions.subtitle")}
      onManualOpen={onManualOpen}
      t={t}
    >
      {sessionView === "browser" ? (
        <DataToolbar
          actions={
            <>
              <ToolbarTextButton
                icon={<Settings size={16} />}
                label={t("toolbar.settings")}
                onClick={() => onOpenSettings("conversations.sessions")}
              />
              <ToolbarActionButton
                disabled={searchIndexRunning}
                icon={<Layers3 size={17} />}
                label={searchIndexRunning ? t("conversation.searchIndex.building") : t("conversation.searchIndex.rebuild")}
                onClick={() => void handleSearchIndexRebuild()}
                text={searchIndexRunning ? t("conversation.searchIndex.building") : t("conversation.searchIndex.rebuild")}
              />
              <ToolbarActionButton
                disabled={syncRunning}
                icon={<PackageCheck size={17} />}
                label={t("conversation.scriptMarket.inlineTitle")}
                onClick={() => setImportDialogOpen(true)}
                text={t("conversation.scriptMarket.inlineTitle")}
              />
              <ToolbarActionButton
                disabled={syncRunning}
                icon={<RefreshCw size={17} />}
                label={syncRunning ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
                onClick={() => void handleSync()}
                primary
                text={syncRunning ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
              />
            </>
          }
          ariaLabel={t("conversation.toolbar.aria")}
          compact={appSettings.conversations.sessionToolbarCompact}
          leading={
            <>
              <DebouncedToolbarSearch
                className="w-[min(22rem,100%)] max-[980px]:w-64"
                commitDelayMs={SESSION_SEARCH_COMMIT_DELAY_MS}
                commitImmediatelyWhen={isConversationShortIdQuery}
                onChange={setQuery}
                placeholder={t("conversation.toolbar.searchPlaceholder")}
                resetSignal={currentRecordKind}
                searching={sessionSearchLoading}
                submitLabel={t("conversation.toolbar.searchSubmit")}
                value={query}
              />
              <DebouncedToolbarSearch
                className="w-[min(24rem,100%)] max-[980px]:w-64"
                commitDelayMs={CONTENT_SEARCH_COMMIT_DELAY_MS}
                commitImmediatelyWhen={isConversationShortIdQuery}
                onChange={setContentQuery}
                placeholder={t("conversation.search.contentPlaceholder")}
                resetSignal={currentRecordKind}
                searching={contentSearchLoading}
                submitLabel={t("conversation.search.submit")}
                value={contentQuery}
              />
              <ToolbarSingleSelectDropdown
                ariaLabel={t("conversation.toolbar.sessionSort")}
                icon={<ArrowDownWideNarrow size={15} />}
                onChange={setSessionSortBy}
                options={[
                  { label: t("conversation.toolbar.sort.updated"), value: "updated" },
                  { label: t("conversation.toolbar.sort.started"), value: "started" },
                  { label: t("toolbar.sort.name"), value: "title" },
                  { label: t("conversation.toolbar.sort.questionCount"), value: "question-count" },
                  { label: t("conversation.toolbar.sort.turnCount"), value: "turn-count" },
                ]}
                value={sessionSortBy}
              />
              <ToolbarSortDirectionButton
                direction={sessionSortDirection}
                label={t("toolbar.sort.direction.label")}
                onClick={() => setSessionSortDirection((current) => (current === "desc" ? "asc" : "desc"))}
                title={t(sessionSortDirection === "desc" ? "toolbar.sort.direction.descTitle" : "toolbar.sort.direction.ascTitle")}
              />
            </>
          }
          sticky
          stickyBleed
        />
      ) : (
        <div className="sticky top-[calc(var(--app-toolbar-top)+var(--app-notification-offset,0px))] z-10 -mx-[var(--app-page-x)] bg-theme-toolbar/78 shadow-[0_12px_28px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur-xl">
          <section
            aria-label={t("conversation.content.filterAria")}
            className="conversation-section-header flex min-w-0 flex-nowrap items-center gap-3 overflow-x-auto px-[var(--app-page-x)] py-3"
          >
            <ToolbarTextButton
              icon={<ArrowLeft size={16} />}
              label={t("conversation.session.backToBrowser")}
              onClick={() => {
                clearSessionDetail();
                showSessionBrowser();
              }}
            />
            <ConversationContentFilter
              availableTypes={availableContentTypes}
              colors={appSettings.conversations.contentCardColors}
              onChange={(type, checked) =>
                updateContentVisibility(type, checked)
              }
              t={t}
              visibility={contentVisibility}
            />
          </section>
        </div>
      )}

      {syncProgress && !syncProgressDismissed ? (
        <ConversationSyncProgress
          onDismiss={
            syncProgress.phase === "completed"
              ? handleDismissSyncProgress
              : undefined
          }
          recordKind={currentRecordKind}
          state={syncProgress}
          t={t}
        />
      ) : null}
      {sessionView === "browser" && (contentSearchResult || contentSearchLoading || contentQuery.trim()) ? (
        <ConversationContentSearchResults
          appMetaById={appMetaById}
          contentCardColors={appSettings.conversations.contentCardColors}
          includeQuestions={contentSearchIncludesQuestions}
          loading={contentSearchLoading}
          onCardKindToggle={toggleContentSearchCardKind}
          onQuestionToggle={() => setContentSearchIncludesQuestions((current) => !current)}
          onSemanticRoleToggle={toggleContentSearchSemanticRole}
          onShowAllCardTypes={showAllContentSearchCardTypes}
          onOpenHit={handleOpenSearchHit}
          result={contentSearchResult}
          selectedCardKinds={contentSearchCardKinds}
          selectedSemanticRoles={contentSearchSemanticRoles}
          t={t}
        />
      ) : null}
      {exportDialog ? (
        <ConversationExportDialog
          availableTypes={exportAvailableContentTypes}
          contentCardColors={appSettings.conversations.contentCardColors}
          exporting={exporting}
          mode={exportDialog.mode}
          onClose={() => setExportDialog(null)}
          onConfirm={handleConfirmExport}
          onOutputRootChange={setOutputRoot}
          onPickOutputRoot={() => selectTargetDirectory(t("conversation.export.pickOutputRoot"))}
          onVisibilityChange={(type, checked) =>
            setExportVisibility((current) => ({ ...current, [type]: checked }))
          }
          outputRoot={outputRoot}
          questionCount={
            exportDialog.mode === "questions"
              ? exportDialog.questionIds.length
              : sessionDetail?.questions.length ?? 0
          }
          t={t}
          visibility={exportVisibility}
        />
      ) : null}

      {importDialogOpen ? (
        <ConversationImportDialog
          onClose={() => setImportDialogOpen(false)}
          onNotify={onNotify}
          onNotifyError={onNotifyError}
          onScriptInstalled={() => refreshCatalog()}
          recordKind={currentRecordKind}
        />
      ) : null}

      {sessionView === "browser" ? (
        <AppSessionBrowser
          appShortcuts={appShortcuts}
          columnMinWidth={appSettings.columnMinWidth}
          groups={appGroups}
          onAppSelect={selectApp}
          onProjectSelect={selectProject}
          onSessionOpen={handleOpenSession}
          recordKind={currentRecordKind}
          selectedAppId={selectedAppId}
          selectedProjectKey={selectedProjectKey}
          t={t}
        />
      ) : (
        <SessionQuestionWorkspace
          activeSearchTarget={activeSearchTarget}
          contentCardColors={appSettings.conversations.contentCardColors}
          onExport={() => openExportDialog("session")}
          onCopyError={onNotifyError}
          onMerge={webRecordMode ? undefined : handleMerge}
          onPickOutputRoot={() => selectTargetDirectory(t("conversation.export.pickOutputRoot"))}
          onQuestionSelect={selectQuestion}
          onQuestionSelectionChange={handleQuestionSelectionChange}
          onSplit={webRecordMode ? undefined : handleSplit}
          columnMinWidth={appSettings.columnMinWidth}
          outputRoot={outputRoot}
          question={selectedQuestion}
          questions={visibleSessionQuestions}
          resultPreviewLineLimit={appSettings.conversations.resultPreviewLineLimit}
          selectedQuestionId={selectedQuestionId}
          selectedQuestionIds={selectedQuestionIds}
          session={sessionDetail}
          setOutputRoot={setOutputRoot}
          t={t}
          recordKind={currentRecordKind}
          translationSettings={{
            ...appSettings.conversationTranslation,
            ...appSettings.aiRuntime,
            ...resolveAgentCapability(appSettings, "cardTranslation"),
          }}
          visibility={contentVisibility}
        />
      )}
    </ConversationShell>
  );
}

export function resolveConversationNavigationTarget(
  session: ConversationSessionDetail,
  target: ConversationNavigationTarget,
): { blockFound: boolean; blockId: string | null; cardType: ConversationSearchCardType | null; questionId: string } | null {
  const candidateQuestions = target.questionId
    ? session.questions.filter((question) => question.question.id === target.questionId)
    : session.questions;
  if (candidateQuestions.length === 0) return null;
  if (!target.blockId) {
    return { blockFound: true, blockId: null, cardType: null, questionId: candidateQuestions[0].question.id };
  }

  for (const question of candidateQuestions) {
    if (question.turns.some((turn) => `${turn.id}-question` === target.blockId)) {
      return { blockFound: true, blockId: target.blockId, cardType: "question", questionId: question.question.id };
    }
    const block = buildConversationContentBlocks(question.parts, question.cards).find(
      (candidate) => candidate.id === target.blockId || candidate.legacyAnchorIds?.includes(target.blockId ?? ""),
    );
    if (block) {
      return {
        blockFound: true,
        blockId: block.id,
        cardType: block.kind ?? block.type,
        questionId: question.question.id,
      };
    }
  }

  return {
    blockFound: false,
    blockId: null,
    cardType: null,
    questionId: candidateQuestions[0].question.id,
  };
}

function ConversationShell({
  children,
  headerActions,
  onManualOpen,
  style,
  subtitle,
  t,
  title,
}: {
  children: ReactNode;
  headerActions?: ReactNode;
  onManualOpen: () => void;
  style?: CSSProperties;
  subtitle: string;
  t: Translator;
  title: string;
}) {
  return (
    <div className="flex w-full flex-1 flex-col px-[var(--app-page-x)] py-6" style={style}>
      <PageHeader
        actions={headerActions}
        className="mb-5"
        eyebrow={t("conversation.eyebrow")}
        icon={<AppWindow size={21} />}
        title={title}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />
      {children}
    </div>
  );
}

function ColumnPanel({
  actions,
  children,
  className = "",
  icon,
  title,
}: {
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  icon: ReactNode;
  title: string;
}) {
  return (
    <section className={`conversation-column flex min-h-0 flex-col ${className}`}>
      <header className="conversation-column-header flex h-12 shrink-0 items-center justify-between gap-2 px-4">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-primary">{icon}</span>
          <h2 className="truncate text-label-caps text-on-surface-variant">{title}</h2>
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-1">{actions}</div> : null}
      </header>
      <div className="conversation-column-scroll min-h-0 flex-1 overflow-auto">{children}</div>
    </section>
  );
}

interface ConversationAppSummary {
  appKind: AppKind;
  id: string;
  name: string;
}

export interface ConversationAppSessionGroup {
  app: ConversationAppSummary;
  projectGroups: ConversationProjectSessionGroup[];
  sessions: ConversationSessionListItem[];
  questionCount: number;
  turnCount: number;
}

export interface ConversationProjectSessionGroup {
  key: string;
  projectPath: string | null;
  sessions: ConversationSessionListItem[];
  questionCount: number;
  turnCount: number;
}

export function conversationContentTypesForQuestions(
  questions: readonly ConversationQuestionDetail[],
): ConversationContentType[] {
  return Array.from(new Set(
    questions.flatMap((question) =>
      buildConversationContentBlocks(question.parts, question.cards).map((block) => block.type)
    ),
  ));
}

export function groupConversationSessionsByApp(
  adapters: ConversationAdapter[],
  sessions: ConversationSessionListItem[],
): ConversationAppSessionGroup[] {
  const sessionsByApp = new Map<string, ConversationSessionListItem[]>();
  for (const session of sessions) {
    const appSessions = sessionsByApp.get(session.adapter_id) ?? [];
    appSessions.push(session);
    sessionsByApp.set(session.adapter_id, appSessions);
  }

  const groups = adapters.map((adapter) =>
    createAppSessionGroup(
      {
        appKind: inferAppKindFromAdapterId(adapter.id),
        id: adapter.id,
        name: adapter.name,
      },
      sessionsByApp.get(adapter.id) ?? [],
    ),
  );

  for (const [appId, appSessions] of sessionsByApp) {
    if (adapters.some((adapter) => adapter.id === appId)) continue;
    groups.push(
      createAppSessionGroup(
        { appKind: inferAppKindFromAdapterId(appId), id: appId, name: appId },
        appSessions,
      ),
    );
  }

  return groups;
}

function createAppSessionGroup(
  app: ConversationAppSummary,
  sessions: ConversationSessionListItem[],
): ConversationAppSessionGroup {
  return {
    app,
    projectGroups: groupConversationSessionsByProject(sessions),
    sessions,
    questionCount: sessions.reduce((total, session) => total + session.question_count, 0),
    turnCount: sessions.reduce((total, session) => total + session.turn_count, 0),
  };
}

const NO_PROJECT_GROUP_KEY = "__assetiweave_no_project__";

export function groupConversationSessionsByProject(
  sessions: ConversationSessionListItem[],
): ConversationProjectSessionGroup[] {
  const groups: ConversationProjectSessionGroup[] = [];
  const groupByKey = new Map<string, ConversationProjectSessionGroup>();

  for (const session of sessions) {
    const projectPath = normalizedProjectPath(session);
    const key = projectPath ?? NO_PROJECT_GROUP_KEY;
    let group = groupByKey.get(key);
    if (!group) {
      group = {
        key,
        projectPath,
        sessions: [],
        questionCount: 0,
        turnCount: 0,
      };
      groupByKey.set(key, group);
      groups.push(group);
    }

    group.sessions.push(session);
    group.questionCount += session.question_count;
    group.turnCount += session.turn_count;
  }

  return groups;
}

function normalizedProjectPath(session: ConversationSessionListItem) {
  const projectPath = session.project_path?.trim();
  return projectPath ? projectPath : null;
}

function sortConversationSessions(
  sessions: ConversationSessionListItem[],
  sortBy: ConversationSessionSortBy,
  sortDirection: "asc" | "desc",
) {
  return [...sessions].sort((left, right) => {
    const direction = sortDirection === "asc" ? 1 : -1;
    let primary = 0;

    if (sortBy === "started") {
      primary = compareOptionalDate(left.started_at, right.started_at);
    } else if (sortBy === "title") {
      primary = left.title.localeCompare(right.title);
    } else if (sortBy === "question-count") {
      primary = left.question_count - right.question_count;
    } else if (sortBy === "turn-count") {
      primary = left.turn_count - right.turn_count;
    } else {
      primary = compareOptionalDate(left.updated_at, right.updated_at);
    }

    if (primary !== 0) {
      return primary * direction;
    }

    return left.title.localeCompare(right.title) || left.id.localeCompare(right.id);
  });
}

function sortConversationQuestions(
  questions: ConversationQuestionDetail[],
  sortBy: ConversationQuestionSortBy,
  sortDirection: "asc" | "desc",
) {
  return [...questions].sort((left, right) => {
    const direction = sortDirection === "asc" ? 1 : -1;
    let primary = 0;

    if (sortBy === "title") {
      primary = (left.question.title ?? left.question.question_text).localeCompare(
        right.question.title ?? right.question.question_text,
      );
    } else if (sortBy === "updated") {
      primary = compareOptionalDate(left.question.updated_at, right.question.updated_at);
    } else {
      primary = left.question.question_index - right.question.question_index;
    }

    if (primary !== 0) {
      return primary * direction;
    }

    return left.question.question_index - right.question.question_index || left.question.id.localeCompare(right.question.id);
  });
}

function compareOptionalDate(left: string | null | undefined, right: string | null | undefined) {
  const leftTime = left ? Date.parse(left) : 0;
  const rightTime = right ? Date.parse(right) : 0;
  return leftTime - rightTime;
}

export function ConversationExportDialog({
  availableTypes,
  contentCardColors,
  exporting,
  mode,
  onClose,
  onConfirm,
  onOutputRootChange,
  onPickOutputRoot,
  onVisibilityChange,
  outputRoot,
  questionCount,
  t,
  visibility,
}: {
  availableTypes: readonly ConversationContentType[];
  contentCardColors: ConversationContentCardColorSettings;
  exporting: boolean;
  mode: ConversationExportMode;
  onClose: () => void;
  onConfirm: () => Promise<void>;
  onOutputRootChange: (value: string) => void;
  onPickOutputRoot: () => Promise<string | null>;
  onVisibilityChange: (type: keyof ConversationContentVisibility, checked: boolean) => void;
  outputRoot: string;
  questionCount: number;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  const [pickingOutputRoot, setPickingOutputRoot] = useState(false);
  const scopeLabel =
    mode === "questions"
      ? t("conversation.export.scopeQuestions", { count: questionCount })
      : t("conversation.export.scopeSession", { count: questionCount });

  async function handlePickOutputRoot() {
    setPickingOutputRoot(true);
    try {
      const selected = await onPickOutputRoot();
      if (selected) {
        onOutputRootChange(abbreviateHomePath(selected));
      }
    } finally {
      setPickingOutputRoot(false);
    }
  }

  return (
    <DialogFrame
      busy={exporting}
      closeLabel={t("conversation.export.close")}
      description={t("conversation.export.description")}
      footer={
        <>
          <ToolbarTextButton
            disabled={exporting}
            icon={<X size={16} />}
            label={t("toolbar.cancel")}
            onClick={onClose}
          />
          <ToolbarActionButton
            disabled={exporting}
            icon={<Download size={17} />}
            label={exporting ? t("conversation.export.exporting") : t("conversation.export.confirm")}
            onClick={() => void onConfirm()}
            primary
            text={exporting ? t("conversation.export.exporting") : t("conversation.export.confirm")}
          />
        </>
      }
      icon={<Download size={18} />}
      onClose={onClose}
      size="lg"
      title={t("conversation.export.title")}
    >
      <div className="grid gap-4">
        <div className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-control/55 px-3 py-3">
          <span className="text-label-caps text-on-surface-muted">{t("conversation.export.scope")}</span>
          <strong className="text-body-sm text-on-surface">{scopeLabel}</strong>
        </div>
        <div className="grid gap-2">
          <span className="text-label-caps text-on-surface-muted">{t("conversation.session.outputRoot")}</span>
          <PathPickerInput
            aria-label={t("conversation.session.outputRoot")}
            disabled={exporting}
            onChange={(event) => onOutputRootChange(event.target.value)}
            onPick={() => void handlePickOutputRoot()}
            pickLabel={t("conversation.export.pickOutputRoot")}
            picking={pickingOutputRoot}
            value={outputRoot}
          />
        </div>
        <ConversationContentFilter
          availableTypes={availableTypes}
          colors={contentCardColors}
          onChange={onVisibilityChange}
          t={t}
          visibility={visibility}
        />
      </div>
    </DialogFrame>
  );
}

export const AppSessionBrowser = memo(function AppSessionBrowser({
  appShortcuts,
  columnMinWidth = DEFAULT_COLUMN_MIN_WIDTH,
  groups,
  onAppSelect,
  onProjectSelect,
  onSessionOpen,
  recordKind = "session",
  selectedAppId,
  selectedProjectKey,
  t,
}: {
  appShortcuts: AppShortcut[];
  columnMinWidth?: number;
  groups: ConversationAppSessionGroup[];
  onAppSelect: (appId: string) => void;
  onProjectSelect: (projectKey: string) => void;
  onSessionOpen: (sessionId: string) => void;
  recordKind?: ConversationRecordKind;
  selectedAppId: string | null;
  selectedProjectKey: string | null;
  t: Translator;
}) {
  const showProjectColumn = recordKind !== "web";
  const selectedGroup = groups.find((group) => group.app.id === selectedAppId) ?? null;
  const selectedProjectGroup = showProjectColumn
    ? selectedGroup?.projectGroups.find((group) => group.key === selectedProjectKey) ??
      selectedGroup?.projectGroups[0] ??
      null
    : selectedGroup
      ? {
          key: selectedGroup.app.id,
          projectPath: null,
          sessions: selectedGroup.sessions,
          questionCount: selectedGroup.questionCount,
          turnCount: selectedGroup.turnCount,
        }
      : null;
  const selectedShortcut = selectedGroup ? findConversationAppShortcut(appShortcuts, selectedGroup.app) : null;
  const browserColumns = showProjectColumn
    ? [
        { defaultWeight: 0.3 },
        { defaultWeight: 0.62 },
        { defaultWeight: 1.08, minWidthScale: 1.25 },
      ]
    : [
        { defaultWeight: 0.36 },
        { defaultWeight: 1.64, minWidthScale: 1.25 },
      ];
  const sessionHeaderTitle = showProjectColumn
    ? selectedProjectGroup
      ? projectGroupLabel(selectedProjectGroup, t)
      : t("conversation.project.select")
    : selectedGroup?.app.name ?? t("conversation.app.select");
  const emptySessionsMessage = showProjectColumn
    ? t("conversation.session.emptyForProject")
    : t("conversation.session.emptyForApp");

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-session-browser conversation-surface mt-5 min-h-[620px] rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
      columns={browserColumns}
      handleClassName="max-[1040px]:hidden"
      minimumWidth={columnMinWidth}
      responsiveClassName="max-[1040px]:w-full max-[1040px]:grid-cols-1"
      scrollBarLabel={t("layout.scrollColumns")}
      scrollLeftLabel={t("layout.scrollColumnsLeft")}
      scrollRightLabel={t("layout.scrollColumnsRight")}
      storageKey={showProjectColumn ? "assetiweave.conversationBrowserColumns.v2" : "assetiweave.webRecordBrowserColumns.v1"}
    >
      <ColumnPanel title={t("conversation.column.apps")} icon={<AppWindow size={16} />}>
        {groups.length === 0 ? (
          <EmptyPanel>{t("conversation.app.empty")}</EmptyPanel>
        ) : (
          groups.map((group) => (
            <AppListItem
              group={group}
              key={group.app.id}
              onSelect={() => onAppSelect(group.app.id)}
              selected={group.app.id === selectedAppId}
              shortcut={findConversationAppShortcut(appShortcuts, group.app)}
              t={t}
            />
          ))
        )}
      </ColumnPanel>
      {showProjectColumn ? (
        <ColumnPanel title={t("conversation.column.projects")} icon={<Folder size={16} />}>
          {!selectedGroup ? (
            <EmptyPanel>{t("conversation.app.select")}</EmptyPanel>
          ) : selectedGroup.projectGroups.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : (
            selectedGroup.projectGroups.map((group) => (
              <ProjectListItem
                key={group.key}
                group={group}
                onSelect={() => onProjectSelect(group.key)}
                selected={group.key === selectedProjectGroup?.key}
                t={t}
              />
            ))
          )}
        </ColumnPanel>
      ) : null}
      <section className="conversation-column flex min-h-0 flex-col">
        <header className="conversation-column-header flex min-h-16 shrink-0 items-center justify-between gap-4 px-5 py-3">
          <div className="flex min-w-0 items-center gap-3">
            {selectedGroup ? <ConversationAppIcon appName={selectedGroup.app.name} shortcut={selectedShortcut} /> : null}
            <div className="min-w-0">
              <p className="text-label-caps text-primary">{t("conversation.column.sessions")}</p>
              <h2 className="mt-1 truncate text-title-sm text-on-surface">{sessionHeaderTitle}</h2>
            </div>
          </div>
          {selectedProjectGroup ? (
            <span className="shrink-0 rounded-full bg-theme-control px-3 py-1 text-code-sm text-on-surface-variant">
              {t("conversation.project.summary", {
                questions: selectedProjectGroup.questionCount,
                sessions: selectedProjectGroup.sessions.length,
                turns: selectedProjectGroup.turnCount,
              })}
            </span>
          ) : null}
        </header>
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {!selectedGroup ? (
            <EmptyPanel>{t("conversation.app.select")}</EmptyPanel>
          ) : showProjectColumn && selectedGroup.projectGroups.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : !selectedProjectGroup ? (
            <EmptyPanel>{showProjectColumn ? t("conversation.project.select") : t("conversation.app.select")}</EmptyPanel>
          ) : selectedProjectGroup.sessions.length === 0 ? (
            <EmptyPanel>{emptySessionsMessage}</EmptyPanel>
          ) : (
            <div className="grid gap-3">
              {selectedProjectGroup.sessions.map((session) => (
                <SessionCard
                  key={session.id}
                  onOpen={() => onSessionOpen(session.id)}
                  session={session}
                  showProjectPath={showProjectColumn}
                  t={t}
                />
              ))}
            </div>
          )}
        </div>
      </section>
    </ResizableColumns>
  );
});

function ProjectListItem({
  group,
  onSelect,
  selected,
  t,
}: {
  group: ConversationProjectSessionGroup;
  onSelect: () => void;
  selected: boolean;
  t: Translator;
}) {
  const label = projectGroupLabel(group, t);

  return (
    <button
      aria-label={t("conversation.project.selectNamed", { path: label })}
      aria-pressed={selected}
      className={`conversation-row grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 px-3 py-3 text-left ${selected ? "text-on-surface" : ""}`}
      data-selected={selected}
      onClick={onSelect}
      type="button"
    >
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
        <Folder size={16} />
      </span>
      <span className="min-w-0">
        <span className="block truncate font-mono text-code-sm font-semibold text-on-surface">{label}</span>
        <span className="mt-1 block text-code-sm text-on-surface-variant">
          {t("conversation.project.sessionCount", { count: group.sessions.length })}
        </span>
      </span>
      <ChevronRight className={selected ? "text-primary" : "text-on-surface-muted"} size={16} />
    </button>
  );
}

function projectGroupLabel(group: ConversationProjectSessionGroup, t: Translator) {
  return group.projectPath ? abbreviateHomePath(group.projectPath) : t("conversation.session.noProject");
}

function conversationSearchCardKinds(result: ConversationSearchResult) {
  const dynamicKinds = [
    ...Object.keys(result.content_type_counts ?? {}),
    ...result.hits.map((hit) => hit.card_type),
  ].filter((kind) => kind !== "question");
  return [...new Set(dynamicKinds)].sort((left, right) => left.localeCompare(right));
}

export function ConversationContentSearchResults({
  appMetaById,
  contentCardColors,
  includeQuestions,
  loading,
  onCardKindToggle,
  onOpenHit,
  onQuestionToggle,
  onSemanticRoleToggle,
  onShowAllCardTypes,
  result,
  selectedCardKinds,
  selectedSemanticRoles,
  t,
}: {
  appMetaById?: ReadonlyMap<string, ConversationSearchAppChipMeta>;
  contentCardColors: ConversationContentCardColorSettings;
  includeQuestions: boolean;
  loading: boolean;
  onCardKindToggle: (kind: string) => void;
  onOpenHit: (hit: ConversationSearchHit) => void;
  onQuestionToggle: () => void;
  onSemanticRoleToggle: (role: string) => void;
  onShowAllCardTypes: () => void;
  result: ConversationSearchResultState | null;
  selectedCardKinds: string[];
  selectedSemanticRoles: string[];
  t: Translator;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const hits = result?.hits ?? [];
  const availableCardKinds = (result?.cardKinds ?? []).filter(
    (kind) => !isRedundantConversationCardKind(kind, definitions.get(kind)),
  );
  const availableSemanticRoles = [...new Set([
    ...(result?.semanticRoles ?? []),
    ...(result?.cardKinds ?? []).flatMap((kind) => {
      const definition = definitions.get(kind);
      return isRedundantConversationCardKind(kind, definition) && definition?.semantic_role
        ? [definition.semantic_role]
        : [];
    }),
  ])];
  const showProjectPath = result?.recordKind !== "web";
  const allCardTypesSelected = includeQuestions
    && selectedCardKinds.length === 0
    && selectedSemanticRoles.length === 0;
  // Keep the previous result useful while a narrowed request is in flight. Card
  // kinds are carried by every hit, so they can be filtered optimistically;
  // semantic roles are facet metadata and are applied by the backend response.
  const visibleHits = hits.filter((hit) => {
    if (hit.card_type === "question") return includeQuestions;
    if (selectedCardKinds.length === 0 || selectedSemanticRoles.length > 0) return true;
    return selectedCardKinds.includes(hit.card_type);
  });
  const query = result?.query ?? "";
  const displayedTotalCount = result?.totalCount ?? visibleHits.length;
  const groupedCardTypes = [...new Set(visibleHits
    .filter((hit) => hit.card_type !== "question")
    .map((hit) => conversationCardPresentationKind(
      hit.card_type,
      definitions.get(hit.card_type)?.semantic_role,
    )))].sort((left, right) => left.localeCompare(right));
  const groupedHits = ["question", ...groupedCardTypes]
    .map((cardType) => ({
      cardType,
      hits: visibleHits.filter((hit) => conversationCardPresentationKind(
        hit.card_type,
        definitions.get(hit.card_type)?.semantic_role,
      ) === cardType),
    }))
    .filter((group) => group.hits.length > 0);

  return (
    <section
      aria-live="polite"
      className="conversation-surface mt-4 overflow-hidden rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.14)]"
    >
      <header className="conversation-section-header grid gap-3 px-4 py-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
        <div className="min-w-0">
          <h2 className="text-label-caps text-on-surface-variant">{t("conversation.search.resultsTitle")}</h2>
          <p className="mt-1 truncate text-body-sm text-on-surface">
            {loading
              ? t("conversation.search.loading")
              : result
                ? t("conversation.search.resultsCount", { count: displayedTotalCount, query })
                : t("conversation.search.empty")}
          </p>
        </div>
        <div
          aria-label={t("conversation.search.typeFilterAria")}
          className="flex min-w-0 flex-wrap items-center gap-1.5"
          role="group"
        >
          <button
            aria-pressed={allCardTypesSelected}
            className={`inline-flex h-8 shrink-0 items-center rounded-xl border px-2.5 text-label-caps transition-[transform,background-color,border-color,box-shadow,color] duration-200 hover:-translate-y-px active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 ${
              allCardTypesSelected
                ? "border-primary/50 bg-primary/12 text-primary"
                : "border-theme-control-border bg-theme-control/80 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface"
            }`}
            onClick={onShowAllCardTypes}
            type="button"
          >
            {t("conversation.search.type.all")}
          </button>
          <SearchCardTypeFilterButton
            active={includeQuestions}
            cardType="question"
            colors={contentCardColors}
            disabled={false}
            onClick={onQuestionToggle}
            t={t}
          />
          {availableCardKinds.map((cardType) => (
            <SearchCardTypeFilterButton
              active={selectedCardKinds.includes(cardType)}
              cardType={cardType}
              colors={contentCardColors}
              disabled={false}
              key={cardType}
              onClick={() => onCardKindToggle(cardType)}
              t={t}
            />
          ))}
          {availableSemanticRoles.map((role) => (
            <SemanticRoleFilterButton
              active={selectedSemanticRoles.includes(role)}
              key={role}
              onClick={() => onSemanticRoleToggle(role)}
              role={role}
            />
          ))}
        </div>
      </header>
      {loading ? (
        <div
          aria-label={t("conversation.search.loading")}
          className="h-1 overflow-hidden bg-theme-control"
          role="progressbar"
        >
          <div className="h-full w-full animate-pulse bg-status-update" />
        </div>
      ) : null}
      {visibleHits.length === 0 ? (
        <div className="px-4 py-6 text-body-sm text-on-surface-variant">
          {loading ? t("conversation.search.loading") : t("conversation.search.empty")}
        </div>
      ) : (
        <div className="grid gap-2">
          {groupedHits.map((group) => (
            <section className="conversation-search-group" key={group.cardType}>
              <header className="flex min-w-0 flex-wrap items-center justify-between gap-2 bg-theme-card-header/35 px-4 py-2">
                <SearchCardTypeBadge cardType={group.cardType} colors={contentCardColors} t={t} />
                <span className="text-code-sm text-on-surface-muted">
                  {t("conversation.search.groupCount", { count: group.hits.length })}
                </span>
              </header>
              <div className="grid gap-2">
                {group.hits.map((hit) => {
                  const appMeta = appMetaById?.get(hit.session.adapter_id);
                  const appName = appMeta?.name ?? hit.session.adapter_id;
                  return (
                    <button
                      aria-label={t("conversation.search.openHit", {
                        title: hit.session.title,
                        type: conversationSearchCardTypeLabel(
                          conversationCardPresentationKind(
                            hit.card_type,
                            definitions.get(hit.card_type)?.semantic_role,
                          ),
                          t,
                        ),
                      })}
                      className="conversation-search-hit grid gap-2 px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
                      key={`${hit.session.id}-${hit.block_id}-${hit.question_id}`}
                      onClick={() => onOpenHit(hit)}
                      type="button"
                    >
                      <span className="flex min-w-0 flex-wrap items-center gap-2">
                        <SearchCardTypeBadge cardType={hit.card_type} colors={contentCardColors} t={t} />
                        <SearchHitMetaChip
                          accentColor={appMeta?.accentColor}
                          label={t("conversation.search.appChip", { app: appName })}
                        />
                        <SearchHitMetaChip
                          className="font-mono"
                          label={t("conversation.search.sessionChip", { sessionId: conversationIdFragment(hit.session.id) })}
                        />
                        <span className="min-w-0 truncate text-body-sm font-semibold text-on-surface">
                          {hit.session.title}
                        </span>
                        <span className="min-w-0 truncate text-code-sm text-on-surface-muted">
                          {hit.question_title}
                        </span>
                      </span>
                      <span className="line-clamp-2 text-body-sm text-on-surface-variant">{hit.snippet}</span>
                      {showProjectPath && hit.session.project_path ? (
                        <span className="truncate font-mono text-code-sm text-on-surface-muted">
                          {abbreviateHomePath(hit.session.project_path)}
                        </span>
                      ) : null}
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}
    </section>
  );
}

function SearchHitMetaChip({
  accentColor,
  className = "",
  label,
}: {
  accentColor?: string | null;
  className?: string;
  label: string;
}) {
  return (
    <span
      className={`inline-flex h-6 min-w-0 max-w-full items-center overflow-hidden text-ellipsis whitespace-nowrap rounded-md border border-theme-control-border bg-theme-control/80 px-2 text-code-sm font-medium text-on-surface-variant shadow-[var(--theme-shadow-control-inset)] ${className}`}
      style={accentColor ? searchHitMetaChipAccentStyle(accentColor) : undefined}
      title={label}
    >
      {label}
    </span>
  );
}

function searchHitMetaChipAccentStyle(accentColor: string): CSSProperties {
  return {
    backgroundColor: `${accentColor}1f`,
    borderColor: `${accentColor}66`,
    color: accentColor,
  };
}

function SearchCardTypeFilterButton({
  active,
  cardType,
  colors,
  disabled,
  onClick,
  t,
}: {
  active: boolean;
  cardType: ConversationSearchCardType;
  colors: ConversationContentCardColorSettings;
  disabled: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const definition = definitions.get(cardType);
  const palette = searchCardTypePalette(cardType, colors);
  return (
    <button
      aria-pressed={active}
      className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-xl border border-theme-control-border px-2.5 text-label-caps text-on-surface-variant transition-[transform,background-color,border-color,box-shadow,color] duration-200 hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 disabled:cursor-default disabled:hover:bg-transparent"
      disabled={disabled}
      onClick={onClick}
      style={{
        backgroundColor: active ? palette.backgroundColor : undefined,
        borderColor: active ? palette.borderColor : undefined,
        color: active ? palette.accentColor : undefined,
      }}
      type="button"
    >
      {cardType === "question" ? (
        <span className="size-2 rounded-full" style={{ backgroundColor: palette.accentColor }} />
      ) : (
        <ConversationCardKindIcon
          iconHint={definition?.icon_hint}
          kind={cardType}
          renderer={definition?.default_renderer ?? "plain"}
          size={13}
        />
      )}
      <span>{definition?.label ?? conversationSearchCardTypeLabel(cardType, t)}</span>
    </button>
  );
}

function SemanticRoleFilterButton({
  active,
  onClick,
  role,
}: {
  active: boolean;
  onClick: () => void;
  role: string;
}) {
  return (
    <button
      aria-pressed={active}
      className={`inline-flex h-8 shrink-0 items-center rounded-xl border px-2.5 text-label-caps transition-[transform,background-color,border-color,box-shadow,color] duration-200 hover:-translate-y-px active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 ${
        active
          ? "border-primary/50 bg-primary/12 text-primary"
          : "border-theme-control-border bg-theme-control/80 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface"
      }`}
      onClick={onClick}
      type="button"
    >
      role:{role.replace(/_/g, " ")}
    </button>
  );
}

function SearchCardTypeBadge({
  cardType,
  colors,
  t,
}: {
  cardType: ConversationSearchCardType;
  colors: ConversationContentCardColorSettings;
  t: Translator;
}) {
  const { definitions } = useConversationCardKindRegistry();
  const definition = definitions.get(cardType);
  const presentationType = conversationCardPresentationKind(cardType, definition?.semantic_role);
  const presentationDefinition = presentationType === cardType ? definition : undefined;
  const palette = searchCardTypePalette(presentationType, colors);
  return (
    <span
      className="inline-flex shrink-0 items-center rounded-full border px-2 py-1 text-label-caps"
      data-search-card-type-badge={cardType}
      style={{
        backgroundColor: palette.backgroundColor,
        borderColor: palette.borderColor,
        color: palette.accentColor,
      }}
    >
      {presentationDefinition?.label ?? conversationSearchCardTypeLabel(presentationType, t)}
    </span>
  );
}

function searchCardTypePalette(
  cardType: ConversationSearchCardType,
  colors: ConversationContentCardColorSettings,
) {
  if (cardType === "question") {
    return {
      accentColor: "rgb(var(--color-primary-strong))",
      backgroundColor: "rgb(var(--color-primary-strong) / 0.12)",
      borderColor: "rgb(var(--color-primary-strong) / 0.42)",
    };
  }
  const accentColor = conversationCardColor(cardType, colors);
  return {
    accentColor,
    backgroundColor: hexWithAlpha(accentColor, "18"),
    borderColor: hexWithAlpha(accentColor, "66"),
  };
}

function hexWithAlpha(hexColor: string, alpha: string) {
  return `${hexColor}${alpha}`;
}

function conversationSearchCardTypeLabel(cardType: ConversationSearchCardType, t: Translator) {
  if (cardType === "question") {
    return t("conversation.search.card.question");
  }
  return conversationCardLabel(cardType, t);
}

function AppListItem({
  group,
  onSelect,
  selected,
  shortcut,
  t,
}: {
  group: ConversationAppSessionGroup;
  onSelect: () => void;
  selected: boolean;
  shortcut: AppShortcut | null;
  t: Translator;
}) {
  return (
    <button
      aria-label={t("conversation.app.selectNamed", { name: group.app.name })}
      aria-pressed={selected}
      className={`conversation-row grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 text-left ${selected ? "text-on-surface" : ""}`}
      data-selected={selected}
      onClick={onSelect}
      type="button"
    >
      <ConversationAppIcon appName={group.app.name} shortcut={shortcut} />
      <span className="min-w-0">
        <span className="block truncate text-body-sm font-semibold text-on-surface">{group.app.name}</span>
        <span className="mt-1 block text-code-sm text-on-surface-variant">
          {t("conversation.app.sessionCount", { count: group.sessions.length })}
        </span>
      </span>
      <ChevronRight className={selected ? "text-primary" : "text-on-surface-muted"} size={16} />
    </button>
  );
}

function ConversationAppIcon({
  appName,
  shortcut,
}: {
  appName: string;
  shortcut: AppShortcut | null;
}) {
  if (!shortcut) {
    return (
      <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
        <AppWindow size={17} />
      </span>
    );
  }

  return (
    <span
      aria-label={appName}
      className={clsx(APP_SHORTCUT_ICON_FRAME_CLASS, "size-9 shrink-0 text-[13px] font-bold")}
      style={{
        borderColor: `${shortcut.accentColor}66`,
        backgroundColor: `${shortcut.accentColor}1f`,
        color: shortcut.accentColor,
      }}
    >
      <AppShortcutIconForShortcut className="size-5" shortcut={shortcut} />
    </span>
  );
}

function findConversationAppShortcut(shortcuts: AppShortcut[], app: ConversationAppSummary) {
  return (
    shortcuts.find((shortcut) => shortcut.profileId === app.id) ??
    shortcuts.find((shortcut) => app.appKind !== "custom" && shortcut.appKind === app.appKind) ??
    null
  );
}

function isWebRecordAdapter(adapter: ConversationAdapter) {
  return adapter.capabilities.includes("web_records") || adapter.id.endsWith("-web");
}

function inferAppKindFromAdapterId(adapterId: string): AppKind {
  const normalized = adapterId.toLowerCase().replace(/_/g, "-");
  if (normalized === "claude" || normalized === "claude-code") return "claude";
  if (normalized === "codex") return "codex";
  if (normalized === "opencode" || normalized === "open-code") return "opencode";
  if (normalized === "gemini") return "gemini";
  if (normalized === "cursor") return "cursor";
  if (normalized === "antigravity") return "antigravity";
  if (normalized === "openclaw" || normalized === "open-claw") return "openclaw";
  return "custom";
}

function SessionCard({
  onOpen,
  session,
  showProjectPath = true,
  t,
}: {
  onOpen: () => void;
  session: ConversationSessionListItem;
  showProjectPath?: boolean;
  t: Translator;
}) {
  const idFragment = conversationIdFragment(session.id);

  return (
    <article className="conversation-session-card group grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border px-4 py-4 text-left">
      <span className="min-w-0 select-text">
        <span className="block truncate text-body-sm font-semibold text-on-surface">{session.title}</span>
        {showProjectPath ? (
          <span className="mt-1 block truncate font-mono text-code-sm text-on-surface-variant">
            {session.project_path ? abbreviateHomePath(session.project_path) : t("conversation.session.noProject")}
          </span>
        ) : null}
        <SessionMetaChips
          idFragment={idFragment}
          questions={session.question_count}
          t={t}
          turns={session.turn_count}
        />
      </span>
      <button
        aria-label={t("conversation.session.open", { title: session.title })}
        className={iconButtonRecipe({ className: "size-9 shrink-0 border border-theme-control-border bg-theme-control text-on-surface-variant group-hover:text-primary" })}
        onClick={onOpen}
        type="button"
      >
        <ChevronRight size={17} />
      </button>
    </article>
  );
}

function SessionMetaChips({
  idFragment,
  questions,
  t,
  turns,
}: {
  idFragment: string;
  questions: number;
  t: Translator;
  turns: number;
}) {
  return (
    <span
      aria-label={t("conversation.session.counts", { questions, turns })}
      className="mt-3 flex min-w-0 flex-wrap items-center gap-1.5"
    >
      <SessionMetaChip className="font-mono" label={idFragment} />
      <SessionMetaChip label={t("conversation.session.questionCountChip", { count: questions })} />
      <SessionMetaChip label={t("conversation.session.turnCountChip", { count: turns })} />
    </span>
  );
}

function SessionMetaChip({
  className = "",
  label,
}: {
  className?: string;
  label: string;
}) {
  return (
    <span
      className={`inline-flex h-7 max-w-full items-center rounded-xl border border-theme-control-border bg-theme-control/80 px-2.5 text-code-sm font-medium text-on-surface-variant shadow-[var(--theme-shadow-control-inset)] ${className}`}
    >
      {label}
    </span>
  );
}

export function SessionQuestionWorkspace({
  activeSearchTarget,
  columnMinWidth = DEFAULT_COLUMN_MIN_WIDTH,
  contentCardColors,
  onExport,
  onCopyError,
  onMerge,
  onPickOutputRoot,
  onQuestionSelect,
  onQuestionSelectionChange,
  onSplit,
  outputRoot,
  question,
  questions,
  recordKind = "session",
  resultPreviewLineLimit = DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  selectedQuestionId,
  selectedQuestionIds,
  session,
  setOutputRoot,
  t,
  translationSettings,
  visibility,
}: {
  activeSearchTarget?: ConversationSearchTarget | null;
  columnMinWidth?: number;
  contentCardColors: ConversationContentCardColorSettings;
  onExport: () => void;
  onCopyError?: (message: string) => void;
  onMerge?: (previous: ConversationQuestionDetail, current: ConversationQuestionDetail) => Promise<void>;
  onPickOutputRoot: () => Promise<string | null>;
  onQuestionSelect: (questionId: string) => void;
  onQuestionSelectionChange: (questionId: string, checked: boolean) => void;
  onSplit?: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail | null;
  questions: ConversationQuestionDetail[];
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  selectedQuestionId: string | null;
  selectedQuestionIds: Set<string>;
  session: ConversationSessionDetail | null;
  setOutputRoot: (value: string) => void;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility: ConversationContentVisibility;
}) {
  const [questionListCollapsed, setQuestionListCollapsed] = useState(false);
  const toggleQuestionList = useCallback(() => {
    setQuestionListCollapsed((current) => !current);
  }, []);
  const questionListToggle = (
    <QuestionListToggleButton
      collapsed={questionListCollapsed}
      onClick={toggleQuestionList}
      t={t}
    />
  );
  const previewPanel = (
    <section className="h-full min-h-0 min-w-0">
      {session && question ? (
        <QuestionPreview
          key={`${session.session.id}:${question.question.id}`}
          activeSearchTarget={activeSearchTarget}
          contentCardColors={contentCardColors}
          onExport={onExport}
          onCopyError={onCopyError}
          onPickOutputRoot={onPickOutputRoot}
          onSplit={onSplit}
          outputRoot={outputRoot}
          question={question}
          questionListToggle={questionListCollapsed ? questionListToggle : undefined}
          resultPreviewLineLimit={resultPreviewLineLimit}
          recordKind={recordKind}
          session={session}
          setOutputRoot={setOutputRoot}
          t={t}
          translationSettings={translationSettings}
          visibility={visibility}
        />
      ) : session ? (
        <ConversationSelectionState label={t("conversation.question.noSelection")} />
      ) : (
        <ConversationPreviewLoadingState label={t("conversation.question.previewLoading")} />
      )}
    </section>
  );

  if (questionListCollapsed) {
    return (
      <section className="conversation-readable conversation-surface mt-5 min-h-[680px] overflow-hidden rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]">
        {previewPanel}
      </section>
    );
  }

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-readable conversation-surface mt-5 min-h-[680px] rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
      columns={[
        { defaultWeight: 0.42 },
        { defaultWeight: 1.58, minWidthScale: 1.35 },
      ]}
      handleClassName="max-[920px]:hidden"
      minimumWidth={columnMinWidth}
      responsiveClassName="max-[920px]:w-full max-[920px]:grid-cols-1"
      scrollBarLabel={t("layout.scrollColumns")}
      scrollLeftLabel={t("layout.scrollColumnsLeft")}
      scrollRightLabel={t("layout.scrollColumnsRight")}
      storageKey="assetiweave.conversationDetailColumns.v1"
    >
      <ColumnPanel
        actions={questionListToggle}
        className="max-[920px]:border-r-0 max-[920px]:border-b"
        title={t("conversation.column.questions")}
        icon={<Layers3 size={16} />}
      >
        {!session ? (
          <ConversationLoadingState label={t("conversation.session.loading")} />
        ) : session.questions.length === 0 ? (
          <EmptyPanel>{t("conversation.question.empty")}</EmptyPanel>
        ) : questions.length === 0 ? (
          <EmptyPanel>{t("conversation.question.emptyForSearch")}</EmptyPanel>
        ) : (
          questions.map((item) => {
            const sessionQuestionIndex = session.questions.findIndex(
              (candidate) => candidate.question.id === item.question.id,
            );
            const previousQuestion =
              sessionQuestionIndex > 0 ? session.questions[sessionQuestionIndex - 1] : null;

            return (
              <QuestionListItem
                key={item.question.id}
                onMergeWithPrevious={
                  previousQuestion && onMerge ? () => void onMerge(previousQuestion, item) : undefined
                }
                onSelect={() => onQuestionSelect(item.question.id)}
                onSelectionChange={(checked) => onQuestionSelectionChange(item.question.id, checked)}
                question={item}
                selected={item.question.id === selectedQuestionId}
                selectedForExport={selectedQuestionIds.has(item.question.id)}
                t={t}
              />
            );
          })
        )}
      </ColumnPanel>
      {previewPanel}
    </ResizableColumns>
  );
}

export function ConversationSelectionState({ label }: { label: string }) {
  return (
    <div aria-live="polite" className="conversation-selection-state">
      <div className="conversation-selection-state-content">
        <span aria-hidden="true" className="conversation-selection-orb">
          <Layers3 size={25} />
        </span>
        <p className="max-w-sm text-center text-body-sm font-semibold leading-6 text-on-surface">{label}</p>
      </div>
    </div>
  );
}

function QuestionListToggleButton({
  collapsed,
  onClick,
  t,
}: {
  collapsed: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const label = t(collapsed ? "conversation.questionSidebar.expand" : "conversation.questionSidebar.collapse");

  return (
    <button
      aria-expanded={!collapsed}
      aria-label={label}
      className={iconButtonRecipe({ className: "size-8 shrink-0 border border-theme-control-border bg-theme-control text-on-surface-variant shadow-[var(--theme-shadow-control-inset)]" })}
      onClick={onClick}
      title={label}
      type="button"
    >
      {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
    </button>
  );
}

function QuestionListItem({
  onMergeWithPrevious,
  onSelect,
  onSelectionChange,
  question,
  selected,
  selectedForExport,
  t,
}: {
  onMergeWithPrevious?: () => void;
  onSelect: () => void;
  onSelectionChange: (checked: boolean) => void;
  question: ConversationQuestionDetail;
  selected: boolean;
  selectedForExport: boolean;
  t: Translator;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);
  const answerPreview = firstLine(question.question.answer_text || question.question.command_text || question.question.code_text, t);

  return (
    <article className={`conversation-row flex h-48 flex-col overflow-hidden ${selected ? "text-on-surface" : ""}`} data-selected={selected}>
      <div className="grid min-h-0 flex-1 grid-cols-[auto_minmax(0,1fr)]">
        <label className="flex px-4 py-3 pr-3">
          <input
            aria-label={t("conversation.question.selectForExport", { title })}
            checked={selectedForExport}
            className="mt-1 size-4 rounded border-theme-control-border bg-theme-control [accent-color:rgb(var(--color-primary-strong))]"
            onChange={(event) => onSelectionChange(event.target.checked)}
            type="checkbox"
          />
        </label>
        <button
          aria-label={t("conversation.question.select", { title })}
          className="flex min-h-0 min-w-0 flex-col overflow-hidden py-3 pr-4 text-left"
          onClick={onSelect}
          title={title}
          type="button"
        >
          <div className="flex min-w-0 items-start justify-between gap-3">
            <h3 className="line-clamp-2 min-w-0 break-words text-body-sm font-semibold text-on-surface">{title}</h3>
            <span className="shrink-0 rounded-full bg-theme-control px-2 py-1 text-code-sm text-on-surface-muted">
              {t("conversation.question.turnCount", { count: question.turns.length })}
            </span>
          </div>
          <p className="mt-2 line-clamp-2 text-body-sm text-on-surface-variant">{answerPreview}</p>
          <p className="mt-auto pt-2 text-label-caps text-on-surface-muted">{questionOriginLabel(question.question.grouping_origin, t)}</p>
        </button>
      </div>
      {onMergeWithPrevious ? (
        <div className="px-4 pb-3 pl-14">
          <ToolbarTextButton icon={<GitMerge size={15} />} label={t("conversation.question.mergePrevious")} onClick={onMergeWithPrevious} />
        </div>
      ) : null}
    </article>
  );
}

export function QuestionPreview({
  activeSearchTarget,
  contentCardColors,
  onExport,
  onCopyError,
  onPickOutputRoot,
  onSplit,
  outputRoot,
  question,
  questionListToggle,
  recordKind = "session",
  resultPreviewLineLimit,
  session,
  setOutputRoot,
  t,
  translationSettings,
  visibility = DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
}: {
  activeSearchTarget?: ConversationSearchTarget | null;
  contentCardColors?: ConversationContentCardColorSettings;
  onExport: () => void;
  onCopyError?: (message: string) => void;
  onPickOutputRoot: () => Promise<string | null>;
  onSplit?: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail;
  questionListToggle?: ReactNode;
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  session: ConversationSessionDetail;
  setOutputRoot: (value: string) => void;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility?: ConversationContentVisibility;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);
  const [pickingOutputRoot, setPickingOutputRoot] = useState(false);
  const activeBlockId = activeSearchTarget?.questionId === question.question.id ? activeSearchTarget.blockId : null;
  const previewScrollRef = useRef<HTMLDivElement>(null);
  const virtualizedCollectionRef = useRef<VirtualizedCollectionHandle>(null);
  const turnModels = useMemo(() => buildConversationTurnPresentations(question), [question]);
  const contentController = useConversationContentController({
    blocks: collectConversationTurnBlocks(turnModels),
    onCopyError,
    recordKind,
    scopeKey: question.question.id,
    t,
    translationSettings: translationSettings ?? DEFAULT_TRANSLATION_SETTINGS,
  });
  const blockTurnIndex = useMemo(() => buildConversationBlockTurnIndex(turnModels), [turnModels]);
  const activeTurnId = activeBlockId ? blockTurnIndex.get(activeBlockId) ?? null : null;
  const [focusedTurnId, setFocusedTurnId] = useState<string | null>(null);
  const eagerKeys = useMemo(
    () => activeTurnId ? new Set([activeTurnId]) : new Set<string>(),
    [activeTurnId],
  );
  const pinnedKeys = useMemo(
    () => new Set([activeTurnId, focusedTurnId].filter((value): value is string => Boolean(value))),
    [activeTurnId, focusedTurnId],
  );

  useEffect(() => {
    if (!activeTurnId) return;
    virtualizedCollectionRef.current?.scrollToKey(activeTurnId, {
      align: "center",
      behavior: "auto",
    });
  }, [activeTurnId]);

  const handleTurnReady = useCallback((turnId: string) => {
    if (turnId !== activeTurnId || !activeBlockId) return;
    window.requestAnimationFrame(() => {
      document
        .getElementById(conversationCardDomId(activeBlockId))
        ?.scrollIntoView?.({ behavior: "auto", block: "center" });
    });
  }, [activeBlockId, activeTurnId]);

  const handlePreviewFocus = useCallback((event: React.FocusEvent<HTMLDivElement>) => {
    const turn = (event.target as HTMLElement).closest<HTMLElement>("[data-conversation-turn-id]");
    setFocusedTurnId(turn?.dataset.conversationTurnId ?? null);
  }, []);

  async function handlePickOutputRoot() {
    setPickingOutputRoot(true);
    try {
      const selected = await onPickOutputRoot();
      if (selected) {
        setOutputRoot(abbreviateHomePath(selected));
      }
    } finally {
      setPickingOutputRoot(false);
    }
  }

  return (
    <div className="conversation-readable flex min-h-full flex-col">
      <header className="conversation-section-header px-5 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex min-w-0 flex-1 items-start gap-3">
            {questionListToggle}
            <div className="min-w-0">
              <p className="text-label-caps text-primary">{questionOriginLabel(question.question.grouping_origin, t)}</p>
              <h2 className="mt-1 text-title-md text-on-surface">{title}</h2>
              {recordKind !== "web" ? (
                <p className="mt-1 text-body-sm text-on-surface-variant">
                  {session.session.project_path ? abbreviateHomePath(session.session.project_path) : t("conversation.session.noProject")}
                </p>
              ) : null}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <PathPickerInput
              aria-label={t("conversation.session.outputRoot")}
              className="min-w-64"
              onChange={(event) => setOutputRoot(event.target.value)}
              onPick={() => void handlePickOutputRoot()}
              pickLabel={t("conversation.export.pickOutputRoot")}
              picking={pickingOutputRoot}
              value={outputRoot}
            />
            <ToolbarActionButton icon={<Download size={17} />} label={t("conversation.session.exportMarkdown")} onClick={onExport} text={t("toolbar.export")} />
          </div>
        </div>
      </header>
      <RenderSafeScrollSurface
        className="min-h-0 flex-1"
        onFocusCapture={handlePreviewFocus}
        ref={previewScrollRef}
      >
        <RenderActivityProvider scrollElementRef={previewScrollRef}>
          <div className="render-safe-scroll-content px-5 py-5">
            <VirtualizedCollection
              contentVisibilityContainmentEnabled={renderingFlags.contentVisibilityContainment}
              deferredRenderingEnabled={renderingFlags.deferredSkeletonRendering && turnModels.length >= 12}
              eagerKeys={eagerKeys}
              enabled={renderingFlags.conversationTurnVirtualization}
              estimateSize={420}
              fallback={() => <ConversationTurnSkeleton />}
              getItemKey={(model) => model.turn.id}
              gap={0}
              items={turnModels}
              minItems={12}
              onItemReady={handleTurnReady}
              pinnedKeys={pinnedKeys}
              ref={virtualizedCollectionRef}
              renderItem={(model, index) => (
                <ConversationTurn
                  activeBlockId={activeBlockId}
                  colors={contentCardColors}
                  controller={contentController}
                  index={index}
                  model={model}
                  onCopyError={onCopyError}
                  onSplit={onSplit ? (turnId) => void onSplit(question, turnId) : undefined}
                  recordKind={recordKind}
                  resultPreviewLineLimit={resultPreviewLineLimit}
                  t={t}
                  translationSettings={translationSettings}
                  visibility={visibility}
                />
              )}
              scrollElementRef={previewScrollRef}
              size="tall"
            />
          </div>
        </RenderActivityProvider>
      </RenderSafeScrollSurface>
    </div>
  );
}

function questionOriginLabel(origin: string, t: Translator) {
  const key = `conversation.question.origin.${origin}` as TranslationKey;
  return t(key);
}

function EmptyPanel({ children }: { children: ReactNode }) {
  return <div className="conversation-empty-state m-2 rounded-2xl p-6 text-center text-body-sm text-on-surface-variant">{children}</div>;
}

function firstLine(value: string, t: Translator) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? t("conversation.markdown.untitledQuestion");
}

export function preferredConversationQuestionId(
  questions: ConversationQuestionDetail[],
  currentQuestionId: string | null,
) {
  const currentQuestion = currentQuestionId
    ? questions.find((question) => question.question.id === currentQuestionId)
    : null;
  if (currentQuestion && currentQuestion.parts.length > 0) return currentQuestion.question.id;
  const firstWithContent = questions.find((question) => question.parts.length > 0);
  if (firstWithContent) return firstWithContent.question.id;
  return currentQuestion?.question.id ?? questions[0]?.question.id ?? null;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function rememberDismissedConversationSyncProgressTask(
  recordKind: ConversationRecordKind,
  taskId: string,
) {
  dismissedConversationSyncProgressTaskKeys.add(
    conversationSyncProgressTaskKey(recordKind, taskId),
  );
  if (dismissedConversationSyncProgressTaskKeys.size <= DISMISSED_SYNC_PROGRESS_TASK_LIMIT) {
    return;
  }
  const oldestKey = dismissedConversationSyncProgressTaskKeys.values().next().value;
  if (oldestKey) {
    dismissedConversationSyncProgressTaskKeys.delete(oldestKey);
  }
}

function conversationSyncProgressTaskKey(recordKind: ConversationRecordKind, taskId: string) {
  return `${recordKind}:${taskId}`;
}

function formatConversationSyncSummary(
  summary: ConversationSyncSummaryCounts | null,
  t: Translator,
  recordKind: ConversationRecordKind = "session",
) {
  if (!summary) {
    return t(
      recordKind === "web"
        ? "conversation.sync.web.summaryUnavailable"
        : "conversation.sync.summaryUnavailable",
    );
  }

  return t(
    recordKind === "web"
      ? summary.incrementalStatsAvailable
        ? summary.errorCount > 0
          ? "conversation.sync.web.incrementalSummaryWithErrors"
          : "conversation.sync.web.incrementalSummary"
        : summary.errorCount > 0
          ? "conversation.sync.web.summaryWithErrors"
          : "conversation.sync.web.summary"
      : summary.incrementalStatsAvailable
        ? summary.errorCount > 0
          ? "conversation.sync.incrementalSummaryWithErrors"
          : "conversation.sync.incrementalSummary"
        : summary.errorCount > 0
          ? "conversation.sync.summaryWithErrors"
          : "conversation.sync.summary",
    {
      errors: summary.errorCount,
      discovered: summary.discoveredSessionCount,
      retained: summary.retainedSessionCount,
      sessions: summary.changedSessionCount,
      skipped: summary.skippedSessionCount,
      sources: summary.sourceCount,
      turns: summary.turnCount,
      warnings: summary.warningCount,
    },
  );
}

function formatConversationSyncAdvice(
  summary: ConversationSyncSummaryCounts | null,
  t: Translator,
  recordKind: ConversationRecordKind = "session",
) {
  if (!summary || summary.errorCount <= 0) {
    return undefined;
  }
  return t(
    recordKind === "web"
      ? "conversation.sync.web.partialFailureAdvice"
      : "conversation.sync.partialFailureAdvice",
  );
}

function formatConversationSyncFailureItems(
  task: ConversationSyncTaskSnapshot,
  sourceLabel: (sourceId: string | null | undefined) => string,
  t: Translator,
) {
  if (!isPlainRecord(task.result) || !Array.isArray(task.result.errors)) {
    return undefined;
  }

  const items = task.result.errors
    .map((rawError) => formatConversationSyncFailureItem(rawError, sourceLabel, t))
    .filter((item): item is { message: string; source: string } => Boolean(item));

  return items.length > 0 ? items : undefined;
}

function formatConversationSyncFailureItem(
  rawError: unknown,
  sourceLabel: (sourceId: string | null | undefined) => string,
  t: Translator,
) {
  if (!isPlainRecord(rawError)) {
    return null;
  }

  const adapterId = stringRecordValue(rawError.adapter_id);
  const sourceId = stringRecordValue(rawError.source_id);
  const sourceName = sourceId ? sourceLabel(sourceId) : null;
  const source = formatConversationSyncFailureSource(adapterId, sourceId, sourceName, t);
  const message = compactConversationSyncFailureMessage(
    stringRecordValue(rawError.message),
    t,
  );

  return { message, source };
}

function formatConversationSyncFailureSource(
  adapterId: string | null,
  sourceId: string | null,
  sourceName: string | null,
  t: Translator,
) {
  const labelParts = [adapterId, sourceName ?? sourceId]
    .filter((part): part is string => Boolean(part))
    .filter((part, index, parts) => parts.indexOf(part) === index);

  return labelParts.length > 0
    ? labelParts.join(" · ")
    : t("conversation.sync.unknownFailedSource");
}

function compactConversationSyncFailureMessage(message: string | null, t: Translator) {
  const normalized = message?.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return t("conversation.sync.failureMessageUnavailable");
  }
  return normalized.length > 260 ? `${normalized.slice(0, 257)}...` : normalized;
}

function stringRecordValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
