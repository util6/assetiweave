import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import {
  AppWindow,
  ArrowDownWideNarrow,
  ArrowLeft,
  ChevronRight,
  Download,
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
  VIRTUALIZATION_MIN_ITEMS,
  renderingFlags,
  type VirtualizedCollectionHandle,
} from "../../components/common/rendering";
import { PathPickerInput } from "../../components/common/PathPickerInput";
import type { NotificationMessage } from "../../components/notifications/NotificationBanner";
import {
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
import {
  buildQuestionTurnRail,
  QuestionTurnRail,
  SubagentChipList,
} from "../../components/conversations/ConversationTurnRail";
import {
  parentSessionBreadcrumb,
  SUBAGENT_EXECUTION_ORIGIN,
  type ParentSessionBreadcrumb,
  type SubagentDispatchPortal,
  type SubagentDispatchPortalViewModel,
  type SubagentSessionTarget,
} from "../../components/conversations/conversationSubagents";
import {
  DEFAULT_TRANSLATION_SETTINGS,
  useConversationContentController,
} from "../../components/conversations/useConversationContentController";
import { MarkdownContent } from "../../components/conversations/ConversationMarkdown";
import {
  ConversationCardKindIcon,
  conversationCardPresentationKind,
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "../../components/conversations/ConversationCardKindRegistry";
import { ConversationContentFilter } from "../../components/conversations/ConversationToolbarControls";
import { SessionShortIdChip } from "../../components/conversations/SessionShortIdChip";
import { ConversationImportDialog } from "../../components/conversations/ConversationImportDialog";
import {
  ConversationLoadingState,
  ConversationPreviewLoadingState,
  ConversationTurnSkeleton,
} from "../../components/conversations/ConversationSkeleton";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { Badge } from "../../components/foundation/Badge";
import { Button } from "../../components/ui/button";
import { ResizableColumns } from "../../components/layout/ResizableColumns";
import {
  ColumnPanel,
  EmptyPanel,
} from "../../components/conversations/ConversationColumns";
import {
  AppSessionBrowser,
  findConversationAppShortcut,
  type ConversationAppSessionGroup,
  type ConversationAppSummary,
  type ConversationProjectSessionGroup,
} from "../../components/conversations/AppSessionBrowser";
import { PageHeader } from "../../components/foundation/PageHeader";
import {
  CollapsibleHeader,
  CollapsibleHeaderProvider,
  useCollapsibleHeader,
} from "../../components/common/CollapsibleHeader";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useQueryScope } from "../../app/query/QueryScopeProvider";
import {
  conversationAdaptersQueryOptions,
  conversationKeys,
  conversationSessionDetailQueryOptions,
  conversationSessionsQueryOptions,
  isWebRecordAdapter,
  loadAllConversationSessionPages,
} from "../../app/query/conversationQueries";
import type { QueryScope } from "../../app/query/catalogQueries";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import { formatErrorMessage } from "../../utils/error";
import { DEFAULT_COLUMN_MIN_WIDTH } from "../../store/settings/settingsSchema";
import {
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  resolveAgentCapability,
  resolveFontFamilyCss,
  type ConversationContentCardColorSettings,
  type ResolvedConversationTranslationSettings,
  type SettingsPanelId,
} from "../../store/settings/settingsSchema";

import {
  exportConversationSession,
  exportWebRecordSession,
  searchConversationRecords,
} from "../../services/conversations";
import { selectTargetDirectory } from "../../services/catalog";
import {
  projectConversationCommandParts,
  type ConversationCommandProjection,
} from "../../services/conversationCommandProjection";
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
  ConversationTurn as ConversationTurnRecord,
} from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { conversationIdFragment } from "../../utils/conversationIds";
import {
  conversationQuestionPreview,
  conversationQuestionTitle,
} from "../../utils/conversationProjection";
import type { ConversationNavigationTarget } from "../../router/navigationTargets";

export { MarkdownContent } from "../../components/conversations/ConversationMarkdown";
export { AppSessionBrowser } from "../../components/conversations/AppSessionBrowser";
export type {
  ConversationAppSessionGroup,
  ConversationProjectSessionGroup,
} from "../../components/conversations/AppSessionBrowser";

const SESSION_PAGE_SIZE = 100;
const SESSION_SEARCH_COMMIT_DELAY_MS = 700;
const CONTENT_SEARCH_COMMIT_DELAY_MS = 700;
const CONVERSATION_SHORT_ID_PATTERN = /^[0-9a-f]{8}$/i;

export function isConversationShortIdQuery(value: string) {
  return CONVERSATION_SHORT_ID_PATTERN.test(value.trim());
}

type ListConversationSessionPage = (params: {
  query?: string | null;
  limit?: number;
  offset?: number;
}) => Promise<ConversationSessionListItem[]>;

import {
  ConversationSearchDialog,
  ConversationSearchTrigger,
  ConversationContentSearchResults,
  getHitKey,
  type ConversationSearchAppChipMeta,
} from "../../components/conversations/ConversationSearchDialog";
import { useOptionalConversationSearch } from "../../app/backgroundTasks/ConversationSearchProvider";

type ConversationPageNotification = Omit<NotificationMessage, "id">;

export { loadAllConversationSessionPages, ConversationContentSearchResults };

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
    detachedSessionId,
    ensureContentVisibility,
    exportDialog,
    exportVisibility,
    exporting,
    focusSearchTarget,
    importDialogOpen,
    openConversationTarget,
    openDetachedSession,
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
    contentSearchAdapterId,
    setContentSearchAdapterId,
    setContentSearchCardKinds,
    setContentSearchIncludesQuestions,
    setContentSearchLoading,
    contentSearchLoadingMore,
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
    updateContentVisibility,
  } = useConversationsController({ recordKind: currentRecordKind });
  const { startSync, taskFor } = conversationSync;
  const {
    rebuild: rebuildSearchIndex,
    status: searchIndexStatus,
    task: searchIndexTask,
  } = searchIndex;
  const syncTask = taskFor(currentRecordKind);
  const webRecordMode = currentRecordKind === "web";
  const queryClient = useQueryClient();
  const queryScope = useQueryScope();
  const fallbackScope = useMemo<QueryScope>(
    () => ({ tenantId: "default", epoch: 1 }),
    [],
  );
  const activeScope = queryScope ?? fallbackScope;

  const adaptersQuery = useQuery(
    conversationAdaptersQueryOptions(activeScope, currentRecordKind),
  );
  const sessionsQuery = useQuery(
    conversationSessionsQueryOptions(activeScope, currentRecordKind, query),
  );
  const sessionDetailQuery = useQuery({
    ...conversationSessionDetailQueryOptions(
      activeScope,
      currentRecordKind,
      selectedSessionId ?? "",
    ),
    enabled: Boolean(selectedSessionId),
  });
  const adapters = useMemo(
    () => adaptersQuery.data ?? [],
    [adaptersQuery.data],
  );
  const sessions = useMemo(
    () => sessionsQuery.data ?? [],
    [sessionsQuery.data],
  );
  const sessionDetail = sessionDetailQuery.data ?? null;
  /** 从父轮次锚点下钻时保留的溯源上下文；搜索命中打开子会话时依赖会话元数据兜底。 */
  const [subagentNavigation, setSubagentNavigation] =
    useState<SubagentSessionTarget | null>(null);
  /** 返回父会话后要定位的轮次；父会话详情到达后由工作区消费一次。 */
  const [pendingParentFocus, setPendingParentFocus] = useState<{
    sessionId: string;
    turnId?: string | null;
    turnIndex?: number | null;
  } | null>(null);
  const [contentSearchDialogOpen, setContentSearchDialogOpen] = useState(false);
  const globalSearch = useOptionalConversationSearch();
  const effectiveContentSearchDialogOpen = globalSearch
    ? globalSearch.dialogOpen
    : contentSearchDialogOpen;
  const setEffectiveContentSearchDialogOpen = useCallback(
    (next: boolean | ((current: boolean) => boolean)) => {
      if (globalSearch) {
        const resolved =
          typeof next === "function" ? next(globalSearch.dialogOpen) : next;
        globalSearch.setDialogOpen(resolved);
      } else {
        setContentSearchDialogOpen(next);
      }
    },
    [globalSearch],
  );
  const effectiveContentQuery = globalSearch
    ? globalSearch.committedQuery
    : contentQuery;
  const effectiveContentSearchResult = globalSearch
    ? globalSearch.result
    : contentSearchResult;
  const effectiveContentSearchLoading = globalSearch
    ? globalSearch.loading
    : contentSearchLoading;
  const effectiveContentSearchLoadingMore = globalSearch
    ? globalSearch.loadingMore
    : contentSearchLoadingMore;
  const effectiveContentSearchAdapterId = globalSearch
    ? globalSearch.committedAdapterId
    : contentSearchAdapterId;
  const effectiveContentSearchCardKinds = globalSearch
    ? globalSearch.committedCardKinds
    : contentSearchCardKinds;
  const effectiveContentSearchSemanticRoles = globalSearch
    ? globalSearch.committedSemanticRoles
    : contentSearchSemanticRoles;
  const effectiveContentSearchIncludesQuestions = globalSearch
    ? globalSearch.committedIncludesQuestions
    : contentSearchIncludesQuestions;
  const handledSyncTaskIdRef = useRef<string | null>(null);
  const syncedAdapterIdsRef = useRef<Set<string>>(new Set());
  const incrementalRefreshingRef = useRef(false);
  const syncRunning =
    syncTask?.status === "running" || syncTask?.status === "cancelling";
  const searchIndexRunning = searchIndexTask?.status === "running";
  const notifiedDetailErrorRef = useRef<unknown>(null);
  const sessionSearchLoading = sessionsQuery.isFetching;

  const sessionCatalogReady = sessionsQuery.data !== undefined;

  useEffect(() => {
    function handleGlobalKeyDown(event: globalThis.KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setEffectiveContentSearchDialogOpen((current) => !current);
      }
    }
    window.addEventListener("keydown", handleGlobalKeyDown);
    return () => window.removeEventListener("keydown", handleGlobalKeyDown);
  }, [setEffectiveContentSearchDialogOpen]);

  useEffect(() => {
    if (sessionsQuery.data !== undefined) {
      reconcileSessionSelection(
        sessionsQuery.data.map((session) => session.id),
      );
    }
  }, [reconcileSessionSelection, sessionsQuery.data]);

  useEffect(() => {
    const error = sessionDetailQuery.error;
    if (!error) {
      notifiedDetailErrorRef.current = null;
      return;
    }
    // 同一次失败只上报一次；重试或换会话后产生新的错误对象才会再次上报。
    if (notifiedDetailErrorRef.current === error) return;
    notifiedDetailErrorRef.current = error;
    onNotifyError(errorMessage(error));
  }, [onNotifyError, sessionDetailQuery.error]);
  const importedSourceNamesRef = useRef<Map<string, string>>(new Map());
  const startedNavigationNonceRef = useRef<string | null>(null);
  const consumedNavigationNonceRef = useRef<string | null>(null);

  const clearSessionDetail = useCallback(() => {
    clearConversationSelection();
  }, [clearConversationSelection]);

  const sessionQuestionCount = useMemo(
    () =>
      sessions.reduce((total, session) => total + session.question_count, 0),
    [sessions],
  );
  const sortedSessions = useMemo(
    () =>
      sortConversationSessions(sessions, sessionSortBy, sessionSortDirection),
    [sessionSortBy, sessionSortDirection, sessions],
  );
  const appGroups = useMemo(
    () =>
      groupConversationSessionsByApp(
        adapters,
        sortedSessions,
        currentRecordKind,
      ),
    [adapters, currentRecordKind, sortedSessions],
  );
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
    () =>
      sessionDetail?.questions.find(
        (question) => question.question.id === selectedQuestionId,
      ) ?? null,
    [selectedQuestionId, sessionDetail],
  );
  const availableContentTypes = useMemo(
    () =>
      conversationContentTypesForQuestions(
        selectedQuestion
          ? [selectedQuestion]
          : (sessionDetail?.questions ?? []),
      ),
    [selectedQuestion, sessionDetail],
  );
  const visibleSessionQuestions = useMemo(
    () =>
      sortConversationQuestions(sessionDetail?.questions ?? [], "index", "asc"),
    [sessionDetail],
  );
  const selectedQuestionCount = selectedQuestionIds.size;
  const exportAvailableContentTypes = useMemo(() => {
    if (!exportDialog || !sessionDetail) return [];
    const selectedIds = new Set(exportDialog.questionIds);
    const questions =
      exportDialog.mode === "questions"
        ? sessionDetail.questions.filter((question) =>
            selectedIds.has(question.question.id),
          )
        : sessionDetail.questions;
    return conversationContentTypesForQuestions(questions);
  }, [exportDialog, sessionDetail]);
  const conversationStyle = useMemo(
    () =>
      ({
        "--conversation-session-browser-font-family": resolveFontFamilyCss(
          appSettings.conversations.sessionBrowserFontFamily,
          "sans",
        ),
        "--conversation-session-browser-font-size": `${appSettings.conversations.sessionBrowserFontSize}px`,
        "--conversation-content-font-family": resolveFontFamilyCss(
          appSettings.conversations.contentFontFamily,
          "sans",
        ),
        "--conversation-content-font-size": `${appSettings.conversations.contentFontSize}px`,
        "--conversation-code-font-size": `${appSettings.conversations.codeFontSize}px`,
      }) as CSSProperties,
    [appSettings.conversations],
  );

  useEffect(() => {
    clearConversationSelection();
    handledSyncTaskIdRef.current = null;
    syncedAdapterIdsRef.current.clear();
  }, [clearConversationSelection, currentRecordKind]);

  useEffect(() => {
    if (sessionCatalogReady) {
      onReady?.();
    }
  }, [onReady, sessionCatalogReady]);

  useEffect(() => {
    const kinds =
      sessionDetail?.questions.flatMap((question) =>
        question.projected_content_nodes.map((node) =>
          conversationCardPresentationKind(node.node_type, node.semantic_role),
        ),
      ) ?? [];
    if (kinds.length === 0) return;
    ensureContentVisibility(kinds);
  }, [ensureContentVisibility, sessionDetail]);

  useEffect(() => {
    if (globalSearch?.lastError && globalSearch.dialogOpen) {
      onNotifyError(globalSearch.lastError);
      globalSearch.clearLastError();
    }
  }, [globalSearch, onNotifyError]);

  useEffect(() => {
    if (globalSearch) {
      return;
    }
    const trimmedQuery = contentQuery.trim();
    if (!trimmedQuery) {
      setContentSearchResult(null);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;
    setContentSearchLoading(true);
    void searchConversationRecords({
      ...(contentSearchAdapterId ? { adapter_id: contentSearchAdapterId } : {}),
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
          includeQuestions:
            result.scope?.include_questions ?? contentSearchIncludesQuestions,
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
    contentSearchAdapterId,
    contentSearchCardKinds,
    contentSearchSemanticRoles,
    contentSearchIncludesQuestions,
    currentRecordKind,
    globalSearch,
    onNotifyError,
    setContentSearchLoading,
    setContentSearchResult,
  ]);

  const handleLoadMoreSearchResults = useCallback(async () => {
    if (
      !contentSearchResult ||
      contentSearchLoading ||
      contentSearchLoadingMore ||
      contentSearchResult.hits.length >= contentSearchResult.totalCount
    ) {
      return;
    }

    const trimmedQuery = contentQuery.trim();
    if (!trimmedQuery) return;

    setContentSearchLoadingMore(true);
    try {
      const nextResult = await searchConversationRecords({
        ...(contentSearchAdapterId
          ? { adapter_id: contentSearchAdapterId }
          : {}),
        content_types: [],
        card_kinds: contentSearchCardKinds,
        semantic_roles: contentSearchSemanticRoles,
        include_questions: contentSearchIncludesQuestions,
        include_cards: true,
        limit: 50,
        offset: contentSearchResult.hits.length,
        query: trimmedQuery,
        record_kind: currentRecordKind,
      });

      setContentSearchResult((current) => {
        if (!current) return null;
        const seen = new Set(current.hits.map(getHitKey));
        const newHits = nextResult.hits.filter(
          (hit) => !seen.has(getHitKey(hit)),
        );
        return {
          ...current,
          hits: [...current.hits, ...newHits],
          totalCount: nextResult.total_count,
        };
      });
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setContentSearchLoadingMore(false);
    }
  }, [
    contentSearchResult,
    contentSearchLoading,
    contentSearchLoadingMore,
    contentQuery,
    contentSearchAdapterId,
    contentSearchCardKinds,
    contentSearchSemanticRoles,
    contentSearchIncludesQuestions,
    currentRecordKind,
    onNotifyError,
    setContentSearchLoadingMore,
    setContentSearchResult,
  ]);

  const handleApplyContentSearchFilters = useCallback(
    (filters: {
      adapterId: string | null;
      cardKinds: string[];
      includeQuestions: boolean;
      semanticRoles: string[];
    }) => {
      setContentSearchAdapterId(filters.adapterId);
      setContentSearchCardKinds(filters.cardKinds);
      setContentSearchSemanticRoles(filters.semanticRoles);
      setContentSearchIncludesQuestions(filters.includeQuestions);
    },
    [
      setContentSearchAdapterId,
      setContentSearchCardKinds,
      setContentSearchSemanticRoles,
      setContentSearchIncludesQuestions,
    ],
  );

  useEffect(() => {
    reconcileAppSelection(
      appGroups.map((group) => group.app.id),
      appGroups.find((group) => group.sessions.length > 0)?.app.id ??
        appGroups[0]?.app.id ??
        null,
    );
  }, [appGroups, reconcileAppSelection]);

  useEffect(() => {
    if (!selectedAppGroup || !selectedSessionId) return;
    // 按需打开的子代理会话不在会话目录里，不能被「不在当前分组」误判关闭。
    if (selectedSessionId === detachedSessionId) return;
    if (
      !selectedAppGroup.sessions.some(
        (session) => session.id === selectedSessionId,
      )
    ) {
      closeSession();
    }
  }, [closeSession, detachedSessionId, selectedAppGroup, selectedSessionId]);

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
  }, [
    reconcileProjectSelection,
    selectProject,
    selectedAppGroup,
    webRecordMode,
  ]);

  useEffect(() => {
    if (selectedSessionId) return;
    clearSessionDetail();
  }, [clearSessionDetail, selectedSessionId]);

  useEffect(() => {
    if (!selectedSessionId && sessionView === "detail") {
      showSessionBrowser();
    }
  }, [selectedSessionId, sessionView, showSessionBrowser]);

  useEffect(() => {
    if (!sessionDetail) {
      clearConversationSelection();
      return;
    }
    reconcileQuestionSelection(
      sessionDetail.questions.map((question) => question.question.id),
      preferredConversationQuestionId(
        sessionDetail.questions,
        selectedQuestionId,
      ),
    );
  }, [
    clearConversationSelection,
    reconcileQuestionSelection,
    selectedQuestionId,
    sessionDetail,
  ]);

  useEffect(() => {
    if (
      !activeSearchTarget ||
      sessionDetail?.session.id !== activeSearchTarget.sessionId
    ) {
      return;
    }
    if (
      sessionDetail.questions.some(
        (question) => question.question.id === activeSearchTarget.questionId,
      )
    ) {
      selectQuestion(activeSearchTarget.questionId);
    }
  }, [activeSearchTarget, selectQuestion, sessionDetail]);

  const consumeNavigationTarget = useCallback(
    (nonce: string) => {
      if (consumedNavigationNonceRef.current === nonce) return;
      consumedNavigationNonceRef.current = nonce;
      onNavigationTargetConsumed?.(nonce);
    },
    [onNavigationTargetConsumed],
  );

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
    const targetSession = sessions.find(
      (session) =>
        session.id === navigationTarget.sessionId ||
        conversationIdFragment(session.id) === navigationTarget.sessionId ||
        session.id.startsWith(navigationTarget.sessionId),
    );
    if (!targetSession) {
      onNotifyError(t("conversation.navigation.sessionMissing"));
      consumeNavigationTarget(navigationTarget.nonce);
      return;
    }

    startedNavigationNonceRef.current = navigationTarget.nonce;
    openConversationTarget({
      appId: targetSession.adapter_id,
      projectKey:
        currentRecordKind === "web"
          ? null
          : (normalizedProjectPath(targetSession) ?? NO_PROJECT_GROUP_KEY),
      questionId: navigationTarget.questionId ?? null,
      searchTarget:
        navigationTarget.blockId && navigationTarget.questionId
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
    const resolved = resolveConversationNavigationTarget(
      sessionDetail,
      navigationTarget,
    );
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

  const refreshCatalog = useCallback(
    async (options: { rethrow?: boolean } = {}) => {
      try {
        // 目录刷新按作用域整体失效：会话目录、适配器与会话详情一起保鲜。
        await queryClient.invalidateQueries({
          queryKey: conversationKeys.root(activeScope),
        });
        const [, nextSessions] = await Promise.all([
          queryClient.fetchQuery(
            conversationAdaptersQueryOptions(activeScope, currentRecordKind),
          ),
          queryClient.fetchQuery(
            conversationSessionsQueryOptions(
              activeScope,
              currentRecordKind,
              query,
            ),
          ),
        ]);
        reconcileSessionSelection(nextSessions.map((session) => session.id));
      } catch (error) {
        if (options.rethrow) throw error;
        onNotifyError(errorMessage(error));
      }
    },
    [
      activeScope,
      currentRecordKind,
      onNotifyError,
      query,
      queryClient,
      reconcileSessionSelection,
    ],
  );

  const handleCloseImportDialog = useCallback(() => {
    setImportDialogOpen(false);
  }, [setImportDialogOpen]);

  const handleScriptInstalled = useCallback(() => {
    void refreshCatalog();
  }, [refreshCatalog]);

  /**
   * 用户再次选中当前已在详情视图中展示的会话（或详情处于失败态）时显式重取，
   * 而不是静默无响应。
   */
  const refetchOpenSessionDetail = useCallback(
    (sessionId: string) => {
      if (sessionId !== selectedSessionId) return;
      if (sessionView !== "detail" && !sessionDetailQuery.isError) return;
      void queryClient.invalidateQueries({
        queryKey: conversationKeys.detail(
          activeScope,
          currentRecordKind,
          sessionId,
        ),
        exact: true,
      });
    },
    [
      activeScope,
      currentRecordKind,
      queryClient,
      selectedSessionId,
      sessionDetailQuery.isError,
      sessionView,
    ],
  );

  useEffect(() => {
    if (!syncTask) {
      syncedAdapterIdsRef.current.clear();
      return;
    }
    if (syncTask.record_kind && syncTask.record_kind !== currentRecordKind) {
      return;
    }

    if (
      handledSyncTaskIdRef.current &&
      handledSyncTaskIdRef.current !== syncTask.id
    ) {
      syncedAdapterIdsRef.current.clear();
      handledSyncTaskIdRef.current = null;
    }

    // 增量刷新：如果某个应用已经同步完成，即使整体任务还在运行，也立即刷新以在前端可见
    const completedAdapterIds = syncTask.progress?.completed_adapter_ids ?? [];
    const newlyCompletedAdapters = completedAdapterIds.filter(
      (id) => !syncedAdapterIdsRef.current.has(id),
    );

    if (
      newlyCompletedAdapters.length > 0 &&
      (syncTask.status === "running" || syncTask.status === "cancelling")
    ) {
      for (const id of newlyCompletedAdapters) {
        syncedAdapterIdsRef.current.add(id);
      }
      if (!incrementalRefreshingRef.current) {
        incrementalRefreshingRef.current = true;
        void refreshCatalog()
          .catch((error) => {
            console.error("Incremental catalog refresh failed:", error);
          })
          .finally(() => {
            incrementalRefreshingRef.current = false;
          });
      }
    }

    if (handledSyncTaskIdRef.current === syncTask.id) {
      return;
    }

    if (
      syncTask.status === "completed" ||
      syncTask.status === "partial_success"
    ) {
      handledSyncTaskIdRef.current = syncTask.id;
      void refreshCatalog({ rethrow: true }).catch((error) => {
        onNotifyError(errorMessage(error));
      });
      return;
    }

    if (syncTask.status === "failed") {
      handledSyncTaskIdRef.current = syncTask.id;
      onNotifyError(
        syncTask.error?.message ?? t("conversation.sync.description.failed"),
      );
    }
  }, [
    currentRecordKind,
    onNotifyError,
    refreshCatalog,
    syncTask,
    syncTask?.error?.message,
    syncTask?.id,
    syncTask?.progress?.completed_adapter_ids,
    syncTask?.record_kind,
    syncTask?.status,
    t,
  ]);

  async function handleSync() {
    handledSyncTaskIdRef.current = null;
    try {
      await startSync({
        dry_run: false,
        record_kind: currentRecordKind,
        source_id: null,
      });
    } catch (error) {
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

  function openExportDialog(
    mode: ConversationExportMode,
    questionIds: string[] = [],
  ) {
    setExportVisibility({ ...contentVisibility });
    setExportDialog({ mode, questionIds });
  }

  async function handleConfirmExport() {
    if (!selectedSessionId || !exportDialog) return;
    const questionIds = exportDialog.questionIds;
    setExporting(true);
    try {
      const exportSession = webRecordMode
        ? exportWebRecordSession
        : exportConversationSession;
      await exportSession(
        selectedSessionId,
        outputRoot,
        false,
        questionIds,
        exportVisibility,
      );
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

  const handleOpenSession = useCallback(
    (sessionId: string) => {
      setSubagentNavigation(null);
      openSession(sessionId);
      refetchOpenSessionDetail(sessionId);
    },
    [openSession, refetchOpenSessionDetail],
  );

  /**
   * 从父轮次锚点下钻到子代理会话：只按需拉取这一个子会话详情。
   */
  const handleOpenSubagentSession = useCallback(
    (target: SubagentSessionTarget) => {
      setSubagentNavigation(target);
      setPendingParentFocus(null);
      openDetachedSession(target.childSessionId);
      refetchOpenSessionDetail(target.childSessionId);
    },
    [openDetachedSession, refetchOpenSessionDetail],
  );

  /**
   * 子会话顶栏面包屑回跳父会话，并把父轮次作为一次性定位目标。
   */
  const handleReturnToParentSession = useCallback(
    (breadcrumb: ParentSessionBreadcrumb) => {
      setSubagentNavigation(null);
      // 从检索命中的子代理会话返回时，当前 App/项目分组可能还停在别处：
      // 父会话若在目录里，顺手把分组切回它自己的位置，否则会被目录对账立刻关闭。
      const parentSession = sessions.find(
        (session) => session.id === breadcrumb.parentSessionId,
      );
      if (parentSession) {
        openConversationTarget({
          appId: parentSession.adapter_id,
          projectKey:
            currentRecordKind === "web"
              ? null
              : (normalizedProjectPath(parentSession) ?? NO_PROJECT_GROUP_KEY),
          questionId: null,
          searchTarget: null,
          sessionId: parentSession.id,
        });
      } else {
        openSession(breadcrumb.parentSessionId);
      }
      setPendingParentFocus({
        sessionId: breadcrumb.parentSessionId,
        turnIndex: breadcrumb.parentTurnIndex ?? null,
      });
      refetchOpenSessionDetail(breadcrumb.parentSessionId);
    },
    [
      currentRecordKind,
      openConversationTarget,
      openSession,
      refetchOpenSessionDetail,
      sessions,
    ],
  );

  const handleConsumeParentFocus = useCallback(() => {
    setPendingParentFocus(null);
  }, []);

  /**
   * 顶栏返回浏览视图：控制器只清会话选择，页面级子代理溯源状态必须一起清，
   * 否则下次打开会话时会带着上一次下钻留下的面包屑/定位目标。
   */
  const handleCloseSession = useCallback(() => {
    setSubagentNavigation(null);
    setPendingParentFocus(null);
    closeSession();
  }, [closeSession]);

  const activeParentBreadcrumb = useMemo<ParentSessionBreadcrumb | null>(() => {
    const current = sessionDetail?.session;
    if (!current) return null;
    if (subagentNavigation?.childSessionId === current.id) {
      return {
        agentRole: subagentNavigation.agentRole ?? undefined,
        parentSessionId: subagentNavigation.parentSessionId,
        parentShortId: conversationIdFragment(
          subagentNavigation.parentSessionId,
        ),
        parentTurnIndex: subagentNavigation.parentTurnIndex,
        parentTurnNumber: subagentNavigation.parentTurnIndex + 1,
      };
    }
    return parentSessionBreadcrumb(current);
  }, [sessionDetail?.session, subagentNavigation]);

  const activeParentFocus =
    pendingParentFocus?.sessionId === sessionDetail?.session.id
      ? pendingParentFocus
      : null;

  const handleOpenSearchHit = useCallback(
    (hit: ConversationSearchHit) => {
      // 子代理会话默认 user_visible = 0，不在会话目录里，必须走游离会话通道。
      if (hit.session.execution_origin === SUBAGENT_EXECUTION_ORIGIN) {
        setSubagentNavigation(null);
        openDetachedSession(hit.session.id);
      } else {
        openConversationTarget({
          appId: hit.session.adapter_id,
          projectKey:
            currentRecordKind === "web"
              ? null
              : (normalizedProjectPath(hit.session) ?? NO_PROJECT_GROUP_KEY),
          questionId: hit.question_id,
          searchTarget: {
            blockId: hit.block_id,
            cardType: hit.card_type,
            questionId: hit.question_id,
            sessionId: hit.session.id,
          },
          sessionId: hit.session.id,
        });
      }
      if (hit.card_type !== "question") {
        updateContentVisibility(hit.card_type, true);
      }
      refetchOpenSessionDetail(hit.session.id);
    },
    [
      currentRecordKind,
      openConversationTarget,
      openDetachedSession,
      refetchOpenSessionDetail,
      updateContentVisibility,
    ],
  );

  const handleQuestionSelectionChange = toggleQuestionSelection;

  function handleBulkExport() {
    if (!sessionDetail || selectedQuestionIds.size === 0) return;
    const questionIds = sessionDetail.questions
      .filter((question) => selectedQuestionIds.has(question.question.id))
      .map((question) => question.question.id);
    openExportDialog("questions", questionIds);
  }

  const syncingAdapterIds = useMemo(() => {
    if (!syncRunning || !syncTask) return new Set<string>();
    if (syncTask.adapter_id) {
      return new Set([canonicalConversationAppId(syncTask.adapter_id)]);
    }
    return new Set(
      adapters.map((adapter) => canonicalConversationAppId(adapter.id)),
    );
  }, [adapters, syncRunning, syncTask]);

  const completedAdapterIds = useMemo(() => {
    return new Set(
      (syncTask?.progress?.completed_adapter_ids ?? []).map((id) =>
        canonicalConversationAppId(id),
      ),
    );
  }, [syncTask?.progress?.completed_adapter_ids]);

  return (
    <ConversationShell
      headerActions={
        sessionView === "browser" ? (
          <PageMetrics
            metrics={[
              {
                label: t("conversation.toolbar.apps"),
                value: appGroups.length,
              },
              {
                label: t("conversation.toolbar.sessions"),
                value: sessions.length,
              },
              {
                label: t("conversation.toolbar.questions"),
                value: sessionQuestionCount,
              },
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
              {
                label: t("conversation.toolbar.questions"),
                value: sessionDetail?.questions.length ?? 0,
              },
              {
                label: t("conversation.toolbar.selected"),
                value: selectedQuestionCount,
              },
            ]}
          />
        )
      }
      style={conversationStyle}
      title={t(
        webRecordMode
          ? "conversation.webRecords.title"
          : "conversation.sessions.title",
      )}
      subtitle={t(
        webRecordMode
          ? "conversation.webRecords.subtitle"
          : "conversation.sessions.subtitle",
      )}
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
                label={
                  searchIndexRunning
                    ? t("conversation.searchIndex.building")
                    : t("conversation.searchIndex.rebuild")
                }
                onClick={() => void handleSearchIndexRebuild()}
                text={
                  searchIndexRunning
                    ? t("conversation.searchIndex.building")
                    : t("conversation.searchIndex.rebuild")
                }
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
                label={
                  syncRunning
                    ? t("conversation.toolbar.syncing")
                    : t("conversation.toolbar.sync")
                }
                onClick={() => void handleSync()}
                primary
                text={
                  syncRunning
                    ? t("conversation.toolbar.syncing")
                    : t("conversation.toolbar.sync")
                }
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
              <ConversationSearchTrigger
                activeQuery={effectiveContentQuery}
                onClick={() => setEffectiveContentSearchDialogOpen(true)}
                placeholder={t("conversation.search.contentPlaceholder")}
                resultCount={
                  effectiveContentSearchResult
                    ? effectiveContentSearchResult.totalCount
                    : null
                }
                resultCountLabel={
                  effectiveContentSearchResult
                    ? t("conversation.search.triggerResults", {
                        count: effectiveContentSearchResult.totalCount,
                      })
                    : undefined
                }
                searching={Boolean(globalSearch?.activeTask)}
                searchingLabel={t("conversation.search.triggerSearching")}
                searchingQuery={globalSearch?.activeTask?.query}
                shortcut="⌘K"
              />
              <ToolbarSingleSelectDropdown
                ariaLabel={t("conversation.toolbar.sessionSort")}
                icon={<ArrowDownWideNarrow size={15} />}
                onChange={setSessionSortBy}
                options={[
                  {
                    label: t("conversation.toolbar.sort.updated"),
                    value: "updated",
                  },
                  {
                    label: t("conversation.toolbar.sort.started"),
                    value: "started",
                  },
                  { label: t("toolbar.sort.name"), value: "title" },
                  {
                    label: t("conversation.toolbar.sort.questionCount"),
                    value: "question-count",
                  },
                  {
                    label: t("conversation.toolbar.sort.turnCount"),
                    value: "turn-count",
                  },
                ]}
                value={sessionSortBy}
              />
              <ToolbarSortDirectionButton
                direction={sessionSortDirection}
                label={t("toolbar.sort.direction.label")}
                onClick={() =>
                  setSessionSortDirection((current) =>
                    current === "desc" ? "asc" : "desc",
                  )
                }
                title={t(
                  sessionSortDirection === "desc"
                    ? "toolbar.sort.direction.descTitle"
                    : "toolbar.sort.direction.ascTitle",
                )}
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
            <ToolbarActionButton
              icon={<ArrowLeft size={16} />}
              label={t("conversation.session.backToBrowser")}
              onClick={handleCloseSession}
              text={t("conversation.session.back")}
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
            <div className="ml-auto flex shrink-0 items-center gap-2">
              <ToolbarActionButton
                icon={<Download size={16} />}
                label={t("conversation.session.exportMarkdown")}
                onClick={() =>
                  openExportDialog(
                    selectedQuestionIds.size > 0 ? "questions" : "session",
                    selectedQuestionIds.size > 0
                      ? [...selectedQuestionIds]
                      : [],
                  )
                }
                text={t("toolbar.export")}
              />
            </div>
          </section>
        </div>
      )}

      <ConversationSearchDialog
        activeTask={globalSearch?.activeTask ?? null}
        adapterId={effectiveContentSearchAdapterId}
        appMetaById={appMetaById}
        commitDelayMs={CONTENT_SEARCH_COMMIT_DELAY_MS}
        commitImmediatelyWhen={isConversationShortIdQuery}
        contentCardColors={appSettings.conversations.contentCardColors}
        includeQuestions={effectiveContentSearchIncludesQuestions}
        loading={effectiveContentSearchLoading}
        loadingMore={effectiveContentSearchLoadingMore}
        onAdapterChange={setContentSearchAdapterId}
        onApplyFilters={handleApplyContentSearchFilters}
        onCancelActiveTask={
          globalSearch
            ? () => void globalSearch.cancelActiveSearch()
            : undefined
        }
        onCardKindToggle={toggleContentSearchCardKind}
        onClose={() => setEffectiveContentSearchDialogOpen(false)}
        onConfirmConflictReplace={
          globalSearch
            ? () => void globalSearch.confirmConflictReplace()
            : undefined
        }
        onDismissConflictKeepWaiting={globalSearch?.dismissConflictKeepWaiting}
        onLoadMore={
          globalSearch
            ? () => void globalSearch.loadMore()
            : handleLoadMoreSearchResults
        }
        onOpenHit={handleOpenSearchHit}
        onQueryChange={setContentQuery}
        onQuestionToggle={() =>
          setContentSearchIncludesQuestions((current) => !current)
        }
        onSemanticRoleToggle={toggleContentSearchSemanticRole}
        onShowAllCardTypes={showAllContentSearchCardTypes}
        onSubmitSearch={
          globalSearch
            ? (nextQuery, filters) =>
                void globalSearch.submitSearch({
                  query: nextQuery,
                  recordKind: currentRecordKind,
                  adapterId: filters.adapterId,
                  cardKinds: filters.cardKinds,
                  semanticRoles: filters.semanticRoles,
                  includeQuestions: filters.includeQuestions,
                })
            : undefined
        }
        open={effectiveContentSearchDialogOpen}
        pendingConflict={globalSearch?.pendingConflict ?? null}
        query={effectiveContentQuery}
        result={effectiveContentSearchResult}
        selectedCardKinds={effectiveContentSearchCardKinds}
        selectedSemanticRoles={effectiveContentSearchSemanticRoles}
        t={t}
      />
      {exportDialog ? (
        <ConversationExportDialog
          availableTypes={exportAvailableContentTypes}
          contentCardColors={appSettings.conversations.contentCardColors}
          exporting={exporting}
          mode={exportDialog.mode}
          onClose={() => setExportDialog(null)}
          onConfirm={handleConfirmExport}
          onOutputRootChange={setOutputRoot}
          onPickOutputRoot={() =>
            selectTargetDirectory(t("conversation.export.pickOutputRoot"))
          }
          onVisibilityChange={(type, checked) =>
            setExportVisibility((current) => ({ ...current, [type]: checked }))
          }
          outputRoot={outputRoot}
          questionCount={
            exportDialog.mode === "questions"
              ? exportDialog.questionIds.length
              : (sessionDetail?.questions.length ?? 0)
          }
          t={t}
          visibility={exportVisibility}
        />
      ) : null}

      {importDialogOpen ? (
        <ConversationImportDialog
          onClose={handleCloseImportDialog}
          onNotify={onNotify}
          onNotifyError={onNotifyError}
          onScriptInstalled={handleScriptInstalled}
          recordKind={currentRecordKind}
        />
      ) : null}

      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-1">
        <div
          aria-hidden={sessionView === "detail"}
          className="col-start-1 row-start-1 flex min-h-0 min-w-0 flex-col"
          inert={sessionView === "detail"}
          style={
            sessionView === "detail" ? { visibility: "hidden" } : undefined
          }
        >
          <AppSessionBrowser
            active={sessionView === "browser"}
            appShortcuts={appShortcuts}
            columnMinWidth={appSettings.columnMinWidth}
            completedAdapterIds={completedAdapterIds}
            groups={appGroups}
            onAppSelect={selectApp}
            onProjectSelect={selectProject}
            onSessionOpen={handleOpenSession}
            recordKind={currentRecordKind}
            selectedAppId={selectedAppId}
            selectedProjectKey={selectedProjectKey}
            syncingAdapterIds={syncingAdapterIds}
            t={t}
          />
        </div>
        {sessionView === "detail" ? (
          <div className="col-start-1 row-start-1 flex min-h-0 min-w-0 flex-col">
            <SessionQuestionWorkspace
              adapterVersion={
                adapters.find(
                  (adapter) =>
                    canonicalConversationAppId(adapter.id) ===
                    canonicalConversationAppId(
                      sessionDetail?.session.adapter_id ?? "",
                    ),
                )?.version ?? "unknown"
              }
              activeSearchTarget={activeSearchTarget}
              contentCardColors={appSettings.conversations.contentCardColors}
              focusTurn={activeParentFocus}
              onFocusTurnConsumed={handleConsumeParentFocus}
              onExport={() => openExportDialog("session")}
              onCopyError={onNotifyError}
              onOpenSubagentSession={handleOpenSubagentSession}
              onPickOutputRoot={() =>
                selectTargetDirectory(t("conversation.export.pickOutputRoot"))
              }
              onQuestionSelect={selectQuestion}
              onQuestionSelectionChange={handleQuestionSelectionChange}
              onReturnToParentSession={handleReturnToParentSession}
              parentBreadcrumb={activeParentBreadcrumb}
              columnMinWidth={appSettings.columnMinWidth}
              outputRoot={outputRoot}
              question={selectedQuestion}
              questions={visibleSessionQuestions}
              resultPreviewLineLimit={
                appSettings.conversations.resultPreviewLineLimit
              }
              selectedQuestionId={selectedQuestionId}
              selectedQuestionIds={selectedQuestionIds}
              session={sessionDetail}
              setOutputRoot={setOutputRoot}
              t={t}
              recordKind={currentRecordKind}
              translationSettings={{
                ...appSettings.conversationTranslation,
                ...resolveAgentCapability(appSettings, "cardTranslation"),
              }}
              visibility={contentVisibility}
            />
          </div>
        ) : null}
      </div>
    </ConversationShell>
  );
}

export function resolveConversationNavigationTarget(
  session: ConversationSessionDetail,
  target: ConversationNavigationTarget,
): {
  blockFound: boolean;
  blockId: string | null;
  cardType: ConversationSearchCardType | null;
  questionId: string;
} | null {
  const candidateQuestions = target.questionId
    ? session.questions.filter(
        (question) => question.question.id === target.questionId,
      )
    : session.questions;
  if (candidateQuestions.length === 0) return null;
  if (!target.blockId) {
    return {
      blockFound: true,
      blockId: null,
      cardType: null,
      questionId: candidateQuestions[0].question.id,
    };
  }

  for (const question of candidateQuestions) {
    const turnBlockMatch = question.turns.some((turn) => {
      if (
        `${turn.id}-question` === target.blockId ||
        turn.id === target.blockId
      ) {
        return true;
      }
      if (target.blockId != null) {
        const numeric = Number(target.blockId);
        if (!Number.isNaN(numeric) && turn.turn_index === numeric) {
          return true;
        }
        if (target.blockId.startsWith("T")) {
          const tNum = Number(target.blockId.slice(1));
          if (!Number.isNaN(tNum) && turn.turn_index === tNum) {
            return true;
          }
        }
      }
      return false;
    });

    if (turnBlockMatch) {
      return {
        blockFound: true,
        blockId: target.blockId,
        cardType: "question",
        questionId: question.question.id,
      };
    }
    const block = question.projected_content_nodes.find(
      (candidate) =>
        candidate.node_id === target.blockId ||
        candidate.legacy_anchor_ids.includes(target.blockId ?? ""),
    );
    if (block) {
      return {
        blockFound: true,
        blockId: block.node_id,
        cardType: block.node_type,
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

function ConversationShellContent({
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
  const collapsible = useCollapsibleHeader();

  return (
    <div
      className="app-bounded-route flex w-full flex-col px-[var(--app-page-x)] py-6"
      style={style}
      onScrollCapture={(event) => {
        // 页面根节点自身不滚动（overflow:hidden）：它自己的 0 偏移事件不能驱动折叠头。
        // capture 阶段的 currentTarget 仍是根节点，真正的滚动主体是事件目标。
        if (event.target === event.currentTarget) return;
        if (!(event.target instanceof HTMLElement)) return;
        collapsible?.onScroll({ currentTarget: event.target });
      }}
    >
      <CollapsibleHeader className="mb-5">
        <PageHeader
          actions={headerActions}
          eyebrow={t("conversation.eyebrow")}
          icon={<AppWindow size={21} />}
          title={title}
          titleAction={<ManualHelpButton onOpen={onManualOpen} />}
        />
      </CollapsibleHeader>
      {children}
    </div>
  );
}

export function ConversationShell(props: {
  children: ReactNode;
  headerActions?: ReactNode;
  onManualOpen: () => void;
  style?: CSSProperties;
  subtitle: string;
  t: Translator;
  title: string;
}) {
  return (
    <CollapsibleHeaderProvider>
      <ConversationShellContent {...props} />
    </CollapsibleHeaderProvider>
  );
}

export function conversationContentTypesForQuestions(
  questions: readonly ConversationQuestionDetail[],
): ConversationContentType[] {
  return Array.from(
    new Set(
      questions.flatMap((question) =>
        question.projected_content_nodes.map((node) =>
          conversationCardPresentationKind(node.node_type, node.semantic_role),
        ),
      ),
    ),
  );
}

export function canonicalConversationAppId(adapterId: string): string {
  const normalized = adapterId.toLowerCase().replace(/_/g, "-");
  if (
    normalized === "codebuddy" ||
    normalized === "code-buddy" ||
    normalized === "workbuddy" ||
    normalized === "work-buddy"
  ) {
    return "codebuddy";
  }
  return adapterId;
}

export function canonicalConversationAppName(
  appId: string,
  originalName?: string,
): string {
  if (appId === "codebuddy") return "CodeBuddy";
  return originalName || appId;
}

export function groupConversationSessionsByApp(
  adapters: ConversationAdapter[],
  sessions: ConversationSessionListItem[],
  recordKind?: ConversationRecordKind,
): ConversationAppSessionGroup[] {
  const sessionsByApp = new Map<string, ConversationSessionListItem[]>();
  for (const session of sessions) {
    const canonicalId = canonicalConversationAppId(session.adapter_id);
    const appSessions = sessionsByApp.get(canonicalId) ?? [];
    appSessions.push(session);
    sessionsByApp.set(canonicalId, appSessions);
  }

  const appMap = new Map<string, ConversationAppSummary>();
  for (const adapter of adapters) {
    const canonicalId = canonicalConversationAppId(adapter.id);
    if (!appMap.has(canonicalId)) {
      appMap.set(canonicalId, {
        appKind: inferAppKindFromAdapterId(canonicalId),
        id: canonicalId,
        name: canonicalConversationAppName(canonicalId, adapter.name),
      });
    }
  }

  const groups: ConversationAppSessionGroup[] = [];
  for (const [appId, app] of appMap) {
    groups.push(createAppSessionGroup(app, sessionsByApp.get(appId) ?? []));
  }

  for (const [appId, appSessions] of sessionsByApp) {
    if (appMap.has(appId)) continue;
    const isWeb = appId.endsWith("-web");
    if (recordKind === "session" && isWeb) continue;
    if (recordKind === "web" && !isWeb) continue;
    groups.push(
      createAppSessionGroup(
        {
          appKind: inferAppKindFromAdapterId(appId),
          id: appId,
          name: canonicalConversationAppName(appId),
        },
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
    questionCount: sessions.reduce(
      (total, session) => total + session.question_count,
      0,
    ),
    turnCount: sessions.reduce(
      (total, session) => total + session.turn_count,
      0,
    ),
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
  const projectPath = session.project_path?.trim() || session.last_cwd?.trim();
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

    return (
      left.title.localeCompare(right.title) || left.id.localeCompare(right.id)
    );
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
      primary = (conversationQuestionTitle(left) ?? "").localeCompare(
        conversationQuestionTitle(right) ?? "",
      );
    } else if (sortBy === "updated") {
      primary = compareOptionalDate(
        left.question.updated_at,
        right.question.updated_at,
      );
    } else {
      primary =
        conversationQuestionOrder(left) - conversationQuestionOrder(right);
    }

    if (primary !== 0) {
      return primary * direction;
    }

    return (
      conversationQuestionOrder(left) - conversationQuestionOrder(right) ||
      left.question.id.localeCompare(right.question.id)
    );
  });
}

function compareOptionalDate(
  left: string | null | undefined,
  right: string | null | undefined,
) {
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
  onVisibilityChange: (
    type: keyof ConversationContentVisibility,
    checked: boolean,
  ) => void;
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
            label={
              exporting
                ? t("conversation.export.exporting")
                : t("conversation.export.confirm")
            }
            onClick={() => void onConfirm()}
            primary
            text={
              exporting
                ? t("conversation.export.exporting")
                : t("conversation.export.confirm")
            }
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
          <span className="text-label-caps text-on-surface-muted">
            {t("conversation.export.scope")}
          </span>
          <strong className="text-body-sm text-on-surface">{scopeLabel}</strong>
        </div>
        <div className="grid gap-2">
          <span className="text-label-caps text-on-surface-muted">
            {t("conversation.session.outputRoot")}
          </span>
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

function conversationSearchCardKinds(result: ConversationSearchResult) {
  const dynamicKinds = [
    ...Object.keys(result.content_type_counts ?? {}),
    ...result.hits.map((hit) => hit.card_type),
  ].filter((kind) => kind !== "question");
  return [...new Set(dynamicKinds)].sort((left, right) =>
    left.localeCompare(right),
  );
}

function inferAppKindFromAdapterId(adapterId: string): AppKind {
  const normalized = adapterId.toLowerCase().replace(/_/g, "-");
  if (normalized === "claude" || normalized === "claude-code") return "claude";
  if (normalized === "codex") return "codex";
  if (normalized === "opencode" || normalized === "open-code")
    return "opencode";
  if (normalized === "gemini") return "gemini";
  if (normalized === "cursor") return "cursor";
  if (normalized === "antigravity") return "antigravity";
  if (normalized === "openclaw" || normalized === "open-claw")
    return "openclaw";
  if (normalized === "codebuddy" || normalized === "code-buddy")
    return "codebuddy";
  if (normalized === "workbuddy" || normalized === "work-buddy")
    return "codebuddy";
  return "custom";
}

export function SessionQuestionWorkspace({
  activeSearchTarget,
  adapterVersion = "unknown",
  columnMinWidth = DEFAULT_COLUMN_MIN_WIDTH,
  contentCardColors,
  onExport,
  onCopyError,
  onOpenSubagentSession,
  onPickOutputRoot,
  onQuestionSelect,
  onQuestionSelectionChange,
  onReturnToParentSession,
  focusTurn,
  onFocusTurnConsumed,
  outputRoot,
  parentBreadcrumb,
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
  adapterVersion?: string;
  columnMinWidth?: number;
  contentCardColors: ConversationContentCardColorSettings;
  focusTurn?: {
    sessionId: string;
    turnId?: string | null;
    turnIndex?: number | null;
  } | null;
  onExport: () => void;
  onCopyError?: (message: string) => void;
  onFocusTurnConsumed?: () => void;
  onOpenSubagentSession?: (target: SubagentSessionTarget) => void;
  onPickOutputRoot: () => Promise<string | null>;
  onQuestionSelect: (questionId: string) => void;
  onQuestionSelectionChange: (questionId: string, checked: boolean) => void;
  onReturnToParentSession?: (breadcrumb: ParentSessionBreadcrumb) => void;
  outputRoot: string;
  parentBreadcrumb?: ParentSessionBreadcrumb | null;
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
  const questionListScrollRef = useRef<HTMLDivElement>(null);
  const selectedQuestionListKey = selectedQuestionId
    ? `${selectedQuestionId}:${questions.findIndex((item) => item.question.id === selectedQuestionId)}`
    : null;
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
  // 左栏选中某个 Turn 时，右侧只渲染这一个 Turn；为空则回到整个问题视图。
  const [selectedTurnId, setSelectedTurnId] = useState<string | null>(null);
  const [railExpandedQuestionId, setRailExpandedQuestionId] = useState<
    string | null
  >(null);
  const focusedQuestion = useMemo(() => {
    if (!question) return question;
    if (!selectedTurnId) return question;
    const turn = question.turns.find((item) => item.id === selectedTurnId);
    if (!turn) return question;
    return {
      ...question,
      turns: [turn],
      projected_content_nodes: question.projected_content_nodes.filter(
        (node) => node.turn_id === turn.id,
      ),
    };
  }, [question, selectedTurnId]);

  // 从子会话返回父会话时，一次性定位到派发该子代理的父轮次。
  useEffect(() => {
    if (!focusTurn) return;
    const matcher = (turn: ConversationTurnRecord) =>
      focusTurn.turnId
        ? turn.id === focusTurn.turnId
        : focusTurn.turnIndex != null &&
          turn.turn_index === focusTurn.turnIndex;
    const targetQuestion = questions.find((item) => item.turns.some(matcher));
    if (targetQuestion) {
      const targetTurn = targetQuestion.turns.find(matcher);
      onQuestionSelect(targetQuestion.question.id);
      setRailExpandedQuestionId(targetQuestion.question.id);
      setSelectedTurnId(targetTurn?.id ?? null);
    }
    onFocusTurnConsumed?.();
  }, [focusTurn, onFocusTurnConsumed, onQuestionSelect, questions]);

  const handleRailSubagentOpen = useCallback(
    (
      portal: SubagentDispatchPortalViewModel,
      turn: ConversationTurnRecord,
      questionId: string,
    ) => {
      // 历史锚点可能只有 external_id，没有可用子会话主键：保持可见但不可跳转。
      if (portal.unresolved || !portal.childSessionId) return;
      onOpenSubagentSession?.({
        agentRole: portal.agentRole ?? null,
        childSessionId: portal.childSessionId,
        parentQuestionId: questionId,
        parentSessionId: session?.session.id ?? "",
        parentTurnId: turn.id,
        parentTurnIndex: turn.turn_index ?? 0,
      });
    },
    [onOpenSubagentSession, session?.session.id],
  );

  const previewPanel = (
    <section className="h-full min-h-0 min-w-0">
      {session && question ? (
        <QuestionPreview
          key={`${session.session.id}:${question.question.id}:${selectedTurnId ?? "all"}`}
          activeSearchTarget={activeSearchTarget}
          adapterVersion={adapterVersion}
          contentCardColors={contentCardColors}
          onExport={onExport}
          onCopyError={onCopyError}
          onOpenSubagent={(portal, turn) =>
            handleRailSubagentOpen(portal, turn, question.question.id)
          }
          onPickOutputRoot={onPickOutputRoot}
          onReturnToParentSession={onReturnToParentSession}
          outputRoot={outputRoot}
          parentBreadcrumb={parentBreadcrumb}
          question={focusedQuestion ?? question}
          questionListToggle={
            questionListCollapsed ? questionListToggle : undefined
          }
          resultPreviewLineLimit={resultPreviewLineLimit}
          recordKind={recordKind}
          session={session}
          setOutputRoot={setOutputRoot}
          t={t}
          translationSettings={translationSettings}
          visibility={visibility}
        />
      ) : session ? (
        <ConversationSelectionState
          label={t("conversation.question.noSelection")}
        />
      ) : (
        <ConversationPreviewLoadingState
          label={t("conversation.question.previewLoading")}
        />
      )}
    </section>
  );

  if (questionListCollapsed) {
    return (
      <section className="conversation-readable conversation-surface mt-5 min-h-0 flex-1 overflow-hidden rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]">
        {previewPanel}
      </section>
    );
  }

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-readable conversation-surface mt-5 min-h-0 flex-1 rounded-2xl shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
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
        badge={
          session ? (
            <SessionShortIdChip sessionId={session.session.id} t={t} />
          ) : null
        }
        className="max-[920px]:border-r-0 max-[920px]:border-b"
        title={t("conversation.column.questions")}
        icon={<Layers3 size={16} />}
        scrollRef={questionListScrollRef}
      >
        {!session ? (
          <ConversationLoadingState label={t("conversation.session.loading")} />
        ) : session.questions.length === 0 ? (
          <EmptyPanel>{t("conversation.question.empty")}</EmptyPanel>
        ) : questions.length === 0 ? (
          <EmptyPanel>{t("conversation.question.emptyForSearch")}</EmptyPanel>
        ) : (
          <RenderActivityProvider scrollElementRef={questionListScrollRef}>
            <VirtualizedCollection
              contentVisibilityContainmentEnabled={
                renderingFlags.contentVisibilityContainment
              }
              deferredRenderingEnabled={
                renderingFlags.deferredSkeletonRendering &&
                questions.length >= 12
              }
              eagerKeys={
                selectedQuestionListKey
                  ? new Set([selectedQuestionListKey])
                  : undefined
              }
              enabled={renderingFlags.conversationListVirtualization}
              estimateSize={192}
              fallback={() => <QuestionListItemSkeleton />}
              getItemKey={(item, index) => `${item.question.id}:${index}`}
              gap={0}
              items={questions}
              minItems={VIRTUALIZATION_MIN_ITEMS}
              pinnedKeys={
                selectedQuestionListKey
                  ? new Set([selectedQuestionListKey])
                  : undefined
              }
              renderItem={(item, index) => (
                <QuestionListItem
                  index={index + 1}
                  onOpenSubagent={(portal, turn) =>
                    handleRailSubagentOpen(portal, turn, item.question.id)
                  }
                  onSelect={() => {
                    onQuestionSelect(item.question.id);
                    setRailExpandedQuestionId(item.question.id);
                    setSelectedTurnId(null);
                  }}
                  onSelectTurn={(turnId) => {
                    onQuestionSelect(item.question.id);
                    setRailExpandedQuestionId(item.question.id);
                    setSelectedTurnId(turnId);
                  }}
                  onSelectionChange={(checked) =>
                    onQuestionSelectionChange(item.question.id, checked)
                  }
                  onToggleRail={() =>
                    setRailExpandedQuestionId((current) =>
                      current === item.question.id ? null : item.question.id,
                    )
                  }
                  question={item}
                  railExpanded={railExpandedQuestionId === item.question.id}
                  selected={item.question.id === selectedQuestionId}
                  selectedForExport={selectedQuestionIds.has(item.question.id)}
                  selectedTurnId={selectedTurnId}
                  t={t}
                />
              )}
              scrollElementRef={questionListScrollRef}
              size="compact"
            />
          </RenderActivityProvider>
        )}
      </ColumnPanel>
      {previewPanel}
    </ResizableColumns>
  );
}

function QuestionListItemSkeleton() {
  return (
    <div
      aria-hidden="true"
      className="conversation-row grid h-48 animate-pulse gap-3 px-4 py-4"
    >
      <div className="h-4 w-3/4 rounded bg-theme-control" />
      <div className="h-3 w-full rounded bg-theme-control/75" />
      <div className="h-3 w-2/3 rounded bg-theme-control/60" />
    </div>
  );
}

export function ConversationSelectionState({ label }: { label: string }) {
  return (
    <div aria-live="polite" className="conversation-selection-state">
      <div className="conversation-selection-state-content">
        <span aria-hidden="true" className="conversation-selection-orb">
          <Layers3 size={25} />
        </span>
        <p className="max-w-sm text-center text-body-sm font-semibold leading-6 text-on-surface">
          {label}
        </p>
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
  const label = t(
    collapsed
      ? "conversation.questionSidebar.expand"
      : "conversation.questionSidebar.collapse",
  );

  return (
    <button
      aria-expanded={!collapsed}
      aria-label={label}
      className={iconButtonRecipe({
        className:
          "size-8 shrink-0 border border-theme-control-border bg-theme-control text-on-surface-variant shadow-[var(--theme-shadow-control-inset)]",
      })}
      onClick={onClick}
      title={label}
      type="button"
    >
      {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
    </button>
  );
}

function QuestionListItem({
  index,
  onOpenSubagent,
  onSelect,
  onSelectTurn,
  onSelectionChange,
  onToggleRail,
  question,
  railExpanded,
  selected,
  selectedForExport,
  selectedTurnId,
  t,
}: {
  index?: number;
  onOpenSubagent: (
    portal: SubagentDispatchPortal,
    turn: ConversationTurnRecord,
  ) => void;
  onSelect: () => void;
  onSelectTurn: (turnId: string) => void;
  onSelectionChange: (checked: boolean) => void;
  onToggleRail: () => void;
  question: ConversationQuestionDetail;
  railExpanded: boolean;
  selected: boolean;
  selectedForExport: boolean;
  selectedTurnId: string | null;
  t: Translator;
}) {
  const title =
    conversationQuestionTitle(question) ??
    t("conversation.markdown.untitledQuestion");
  const answerPreview =
    conversationQuestionPreview(question) ?? t("conversation.markdown.empty");
  const railEntries = useMemo(
    () => buildQuestionTurnRail(question),
    [question],
  );
  // 1 问 1 轮时直接把子代理胶囊挂在问题卡片上，不再多一层 Turn 列表。
  const directRailEntry = railEntries.length === 1 ? railEntries[0]! : null;
  const directSubagents = directRailEntry?.subagents ?? [];
  const showDirectSubagents = directSubagents.length > 0;
  const railToggleLabel = showDirectSubagents
    ? t("conversation.turn.childCount", { count: directSubagents.length })
    : t("conversation.question.turnCount", { count: railEntries.length });
  const railToggleAriaLabel = showDirectSubagents
    ? t("conversation.turn.railToggleSubagents", {
        count: directSubagents.length,
      })
    : t("conversation.question.turnCount", { count: railEntries.length });
  const [expandedRailRootIds, setExpandedRailRootIds] = useState<
    ReadonlySet<string>
  >(() => new Set<string>());
  const toggleRailRoot = useCallback((turnId: string) => {
    setExpandedRailRootIds((current) => {
      const next = new Set(current);
      if (next.has(turnId)) next.delete(turnId);
      else next.add(turnId);
      return next;
    });
  }, []);

  return (
    <article
      className={`conversation-row flex min-h-48 flex-col overflow-hidden ${selected ? "text-on-surface" : ""}`}
      data-selected={selected}
    >
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
            <h3 className="line-clamp-2 min-w-0 break-words text-body-sm font-semibold text-on-surface">
              {typeof index === "number" ? (
                <span className="mr-1.5 inline-flex items-center justify-center rounded-full bg-theme-control px-1.5 py-0.5 font-mono text-code-xs font-semibold text-on-surface-muted align-middle">
                  #{index}
                </span>
              ) : null}
              {title}
            </h3>
            <span
              aria-expanded={railExpanded}
              aria-label={railToggleAriaLabel}
              className="inline-flex shrink-0 items-center gap-1 rounded-full bg-theme-control px-2 py-1 text-code-sm text-on-surface-muted"
              onClick={(event) => {
                event.stopPropagation();
                onToggleRail();
              }}
              role="button"
              tabIndex={0}
            >
              {railToggleLabel}
              <ChevronRight
                className={`size-3 transition-transform ${
                  railExpanded ? "rotate-90" : ""
                }`}
              />
            </span>
          </div>
          <p className="mt-2 line-clamp-2 text-body-sm text-on-surface-variant">
            {answerPreview}
          </p>
          <p className="mt-auto pt-2 text-label-caps text-on-surface-muted">
            {questionOriginLabel(question, t)}
          </p>
        </button>
      </div>
      {railExpanded ? (
        <div className="px-4 pb-3 pl-11">
          {showDirectSubagents ? (
            <SubagentChipList
              className=""
              onOpen={(portal) => onOpenSubagent(portal, directRailEntry!.turn)}
              portals={directSubagents}
              t={t}
            />
          ) : (
            <QuestionTurnRail
              entries={railEntries}
              expandedRootIds={expandedRailRootIds}
              onOpenSubagent={onOpenSubagent}
              onSelectTurn={onSelectTurn}
              onToggleRoot={toggleRailRoot}
              selectedTurnId={selectedTurnId}
              t={t}
            />
          )}
        </div>
      ) : null}
    </article>
  );
}

export function QuestionPreview({
  activeSearchTarget,
  adapterVersion = "unknown",
  contentCardColors,
  onExport,
  onCopyError,
  onOpenSubagent,
  onPickOutputRoot,
  onReturnToParentSession,
  outputRoot,
  parentBreadcrumb,
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
  adapterVersion?: string;
  contentCardColors?: ConversationContentCardColorSettings;
  onExport?: () => void;
  onCopyError?: (message: string) => void;
  onOpenSubagent?: (
    portal: SubagentDispatchPortalViewModel,
    turn: ConversationTurnRecord,
  ) => void;
  onPickOutputRoot?: () => Promise<string | null>;
  onReturnToParentSession?: (breadcrumb: ParentSessionBreadcrumb) => void;
  outputRoot?: string;
  parentBreadcrumb?: ParentSessionBreadcrumb | null;
  question: ConversationQuestionDetail;
  questionListToggle?: ReactNode;
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  session: ConversationSessionDetail;
  setOutputRoot?: (value: string) => void;
  t: Translator;
  translationSettings?: ResolvedConversationTranslationSettings;
  visibility?: ConversationContentVisibility;
}) {
  const title =
    conversationQuestionTitle(question) ??
    t("conversation.markdown.untitledQuestion");
  const [commandProjections, setCommandProjections] = useState<
    ConversationCommandProjection[]
  >([]);
  const requestedProjectionPartIdsRef = useRef(new Set<string>());
  const commandPartById = useMemo(
    () =>
      new Map(
        question.parts.flatMap((part) =>
          part.command?.trim() ? [[part.id, part] as const] : [],
        ),
      ),
    [question.parts],
  );
  const activeBlockId =
    activeSearchTarget?.questionId === question.question.id
      ? activeSearchTarget.blockId
      : null;
  const previewScrollRef = useRef<HTMLDivElement>(null);
  const virtualizedCollectionRef = useRef<VirtualizedCollectionHandle>(null);
  const turnModels = useMemo(
    () => buildConversationTurnPresentations(question, commandProjections),
    [commandProjections, question],
  );
  const contentController = useConversationContentController({
    blocks: collectConversationTurnBlocks(turnModels),
    onCopyError,
    onTranslationError: onCopyError,
    recordKind,
    scopeKey: question.question.id,
    t,
    translationSettings: translationSettings ?? DEFAULT_TRANSLATION_SETTINGS,
  });
  const blockTurnIndex = useMemo(
    () => buildConversationBlockTurnIndex(turnModels),
    [turnModels],
  );
  const activeTurnId = activeBlockId
    ? (blockTurnIndex.get(activeBlockId) ??
      blockTurnIndex.get(conversationProjectionPartId(activeBlockId)) ??
      null)
    : null;
  const [focusedTurnId, setFocusedTurnId] = useState<string | null>(null);
  const eagerKeys = useMemo(
    () => (activeTurnId ? new Set([activeTurnId]) : new Set<string>()),
    [activeTurnId],
  );
  const pinnedKeys = useMemo(
    () =>
      new Set(
        [activeTurnId, focusedTurnId].filter((value): value is string =>
          Boolean(value),
        ),
      ),
    [activeTurnId, focusedTurnId],
  );

  const handleCommandPartsVisible = useCallback(
    (partIds: string[]) => {
      const parts = partIds.flatMap((partId) => {
        if (requestedProjectionPartIdsRef.current.has(partId)) return [];
        const part = commandPartById.get(partId);
        if (!part?.command?.trim()) return [];
        requestedProjectionPartIdsRef.current.add(partId);
        return [
          { partId, command: part.command, commandLabel: part.command_label },
        ];
      });
      if (parts.length === 0) return;
      void projectConversationCommandParts({
        adapterId: session.session.adapter_id,
        adapterVersion,
        parts,
      })
        .then((projections) => {
          setCommandProjections((current) => {
            const merged = new Map(
              current.map((projection) => [projection.part_id, projection]),
            );
            projections.forEach((projection) =>
              merged.set(projection.part_id, projection),
            );
            return [...merged.values()];
          });
        })
        .catch((error) => {
          onCopyError?.(
            t("conversation.content.projectionFailed", {
              message: errorMessage(error),
            }),
          );
        });
    },
    [
      adapterVersion,
      commandPartById,
      onCopyError,
      session.session.adapter_id,
      t,
    ],
  );

  const scrolledTurnIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!activeTurnId) {
      scrolledTurnIdRef.current = null;
      return;
    }
    if (scrolledTurnIdRef.current === activeTurnId) return;
    if (!turnModels.some((model) => model.turn.id === activeTurnId)) return;
    scrolledTurnIdRef.current = activeTurnId;
    virtualizedCollectionRef.current?.scrollToKey(activeTurnId, {
      align: "center",
      behavior: "auto",
    });
  }, [activeTurnId, turnModels]);

  const handleTurnReady = useCallback(
    (turnId: string) => {
      if (turnId !== activeTurnId || !activeBlockId) return;
      window.requestAnimationFrame(() => {
        document
          .getElementById(conversationCardDomId(activeBlockId))
          ?.scrollIntoView?.({ behavior: "auto", block: "center" });
      });
    },
    [activeBlockId, activeTurnId],
  );

  const handlePreviewFocus = useCallback(
    (event: React.FocusEvent<HTMLDivElement>) => {
      const turn = (event.target as HTMLElement).closest<HTMLElement>(
        "[data-conversation-turn-id]",
      );
      setFocusedTurnId(turn?.dataset.conversationTurnId ?? null);
    },
    [],
  );

  return (
    <div className="conversation-readable flex h-full min-h-0 flex-col">
      <header className="conversation-section-header flex shrink-0 items-center justify-between gap-3 px-4 py-2.5">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          {questionListToggle}
          {parentBreadcrumb && onReturnToParentSession ? (
            <Button
              aria-label={
                parentBreadcrumb.parentTurnNumber === undefined
                  ? t("conversation.subagent.backToParent", {
                      id: parentBreadcrumb.parentShortId,
                    })
                  : t("conversation.subagent.backToParentTurn", {
                      id: parentBreadcrumb.parentShortId,
                      turn: parentBreadcrumb.parentTurnNumber,
                    })
              }
              className="h-8 shrink-0 gap-1.5 rounded-full border border-primary/30 bg-theme-control/70 px-3 text-code-sm text-primary hover:bg-primary/10"
              onClick={() => onReturnToParentSession(parentBreadcrumb)}
              size="sm"
              type="button"
              variant="outline"
            >
              <ArrowLeft aria-hidden="true" className="size-3.5" />
              <span>
                {parentBreadcrumb.parentTurnNumber === undefined
                  ? t("conversation.subagent.backToParent", {
                      id: parentBreadcrumb.parentShortId,
                    })
                  : t("conversation.subagent.backToParentTurn", {
                      id: parentBreadcrumb.parentShortId,
                      turn: parentBreadcrumb.parentTurnNumber,
                    })}
              </span>
            </Button>
          ) : null}
          {parentBreadcrumb?.agentRole ? (
            <Badge
              className="shrink-0 px-2.5 py-0.5 text-xs normal-case tracking-normal"
              title={parentBreadcrumb.agentRole}
              tone="primary"
            >
              {parentBreadcrumb.agentRole}
            </Badge>
          ) : null}
          <div className="flex min-w-0 items-center gap-2">
            <h2
              className="truncate text-body-sm font-semibold text-on-surface"
              title={
                session.session.title || t("conversation.session.untitled")
              }
            >
              {session.session.title || t("conversation.session.untitled")}
            </h2>
            {recordKind !== "web" &&
            (session.session.project_path || session.session.last_cwd) ? (
              <span
                className="hidden truncate font-mono text-code-xs text-on-surface-muted sm:inline-block"
                title={
                  session.session.project_path || session.session.last_cwd || ""
                }
              >
                {abbreviateHomePath(
                  session.session.project_path ||
                    session.session.last_cwd ||
                    "",
                )}
              </span>
            ) : null}
          </div>
        </div>
      </header>
      <RenderSafeScrollSurface
        className="min-h-0 flex-1"
        onFocusCapture={handlePreviewFocus}
        ref={previewScrollRef}
        tabIndex={0}
        aria-label={title}
      >
        <RenderActivityProvider scrollElementRef={previewScrollRef}>
          <div className="render-safe-scroll-content px-5 py-5">
            <VirtualizedCollection
              contentVisibilityContainmentEnabled={
                renderingFlags.contentVisibilityContainment
              }
              deferredRenderingEnabled={
                renderingFlags.deferredSkeletonRendering &&
                turnModels.length >= 12
              }
              eagerKeys={eagerKeys}
              enabled={renderingFlags.conversationTurnVirtualization}
              estimateSize={420}
              fallback={() => <ConversationTurnSkeleton />}
              getItemKey={(model) => model.turn.id}
              gap={0}
              items={turnModels}
              minItems={VIRTUALIZATION_MIN_ITEMS}
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
                  onCommandPartsVisible={handleCommandPartsVisible}
                  onOpenSubagent={onOpenSubagent}
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

function questionOriginLabel(
  question: ConversationQuestionDetail,
  t: Translator,
) {
  const origins = new Set(
    question.question_turns.map((membership) => membership.assignment_origin),
  );
  const origin = origins.has("manual")
    ? "manual"
    : origins.has("auto_merged")
      ? "auto_merged"
      : "imported";
  const key = `conversation.question.origin.${origin}` as TranslationKey;
  return t(key);
}

function conversationProjectionPartId(blockId: string) {
  return blockId.replace(/::display:\d+$/, "");
}

function conversationQuestionOrder(question: ConversationQuestionDetail) {
  return Math.min(
    ...question.turns.map((turn) => turn.turn_index),
    Number.MAX_SAFE_INTEGER,
  );
}

function firstLine(value: string, t: Translator) {
  return (
    value
      .split(/\r?\n/)
      .find((line) => line.trim())
      ?.trim() ?? t("conversation.markdown.untitledQuestion")
  );
}

export function preferredConversationQuestionId(
  questions: ConversationQuestionDetail[],
  currentQuestionId: string | null,
) {
  const currentQuestion = currentQuestionId
    ? questions.find((question) => question.question.id === currentQuestionId)
    : null;
  if (currentQuestion && currentQuestion.parts.length > 0)
    return currentQuestion.question.id;
  const firstWithContent = questions.find(
    (question) => question.parts.length > 0,
  );
  if (firstWithContent) return firstWithContent.question.id;
  return currentQuestion?.question.id ?? questions[0]?.question.id ?? null;
}

function errorMessage(error: unknown) {
  return formatErrorMessage(error);
}
