import { memo, useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent, type CompositionEvent, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import {
  AppWindow,
  ArrowDownWideNarrow,
  ArrowLeft,
  Check,
  ChevronRight,
  Copy,
  Download,
  Folder,
  GitMerge,
  Layers3,
  RefreshCw,
  Scissors,
  Search,
  Settings,
  UploadCloud,
  X,
} from "lucide-react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarSearch,
  ToolbarSingleSelectDropdown,
  ToolbarSortDirectionButton,
  ToolbarTextButton,
} from "../../components/common/DataToolbar";
import { PageMetrics } from "../../components/common/PageMetrics";
import { PathPickerInput } from "../../components/common/PathPickerInput";
import { AppShortcutIconForShortcut } from "../../components/apps/AppShortcutIcon";
import type { NotificationMessage } from "../../components/notifications/NotificationBanner";
import {
  buildConversationContentBlocks,
  conversationCardDomId,
  ConversationContentCards,
  DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  type ConversationContentVisibility,
} from "../../components/conversations/ConversationContentCards";
import { MarkdownContent } from "../../components/conversations/ConversationMarkdown";
import {
  ConversationContentFilter,
  ConversationSyncProgress,
  type ConversationSyncProgressState,
} from "../../components/conversations/ConversationToolbarControls";
import {
  ConversationImportDialog,
  type ConversationImportFormValues,
  type ConversationImportStep,
} from "../../components/conversations/ConversationImportDialog";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { ResizableColumns } from "../../components/layout/ResizableColumns";
import { PageHeader } from "../../components/foundation/PageHeader";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import { DEFAULT_COLUMN_MIN_WIDTH } from "../../store/settings/settingsSchema";
import {
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  resolveFontFamilyCss,
  useAppSettings,
  type ConversationContentCardColorSettings,
  type ConversationTranslationSettings,
  type SettingsPanelId,
} from "../../store/settings/AppSettingsProvider";
import {
  exportConversationSession,
  exportWebRecordSession,
  getConversationSession,
  getWebRecordSession,
  importConversationSource,
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
import { selectFilePath, selectTargetDirectory } from "../../services/catalog";
import { useConversationSync } from "../../app/backgroundTasks/ConversationSyncProvider";
import type {
  AppKind,
  AppShortcut,
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSourceKind,
  ConversationSearchCardType,
  ConversationSearchHit,
  ConversationRecordKind,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import { abbreviateHomePath } from "../../utils/path";

export { MarkdownContent } from "../../components/conversations/ConversationMarkdown";

const SESSION_PAGE_SIZE = 100;
const CONTENT_SEARCH_COMMIT_DELAY_MS = 700;
const CONVERSATION_SEARCH_CARD_TYPES: ConversationSearchCardType[] = [
  "question",
  "answer",
  "tool",
  "command",
  "code",
  "result",
];
const DISMISSED_SYNC_PROGRESS_TASK_LIMIT = 50;
const dismissedConversationSyncProgressTaskKeys = new Set<string>();

type ListConversationSessionPage = (params: {
  query?: string | null;
  limit?: number;
  offset?: number;
}) => Promise<ConversationSessionListItem[]>;

interface ConversationSearchResultState {
  contentTypes: ConversationSearchCardType[];
  query: string;
  totalCount: number;
  hits: ConversationSearchHit[];
}

interface ConversationSearchTarget {
  blockId: string;
  cardType: ConversationSearchCardType;
  questionId: string;
  sessionId: string;
}

type ConversationPageNotification = Omit<NotificationMessage, "id">;
type ConversationSessionSortBy = "updated" | "started" | "title" | "question-count" | "turn-count";
type ConversationQuestionSortBy = "index" | "updated" | "title";

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

export function ConversationsPage({
  appShortcuts,
  onManualOpen,
  onNotify,
  onNotifyError,
  onOpenSettings,
  recordKind = "session",
}: {
  activeSubNavId?: string;
  appShortcuts: AppShortcut[];
  onManualOpen: () => void;
  onNotify: (notification: ConversationPageNotification) => void;
  onNotifyError: (message: string) => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
  recordKind?: "session" | "web";
}) {
  const { t } = useI18n();
  const { startSync, task: syncTask } = useConversationSync();
  const { settings: appSettings } = useAppSettings();
  const currentRecordKind: ConversationRecordKind = recordKind;
  const webRecordMode = currentRecordKind === "web";
  const [adapters, setAdapters] = useState<ConversationAdapter[]>([]);
  const [sessions, setSessions] = useState<ConversationSessionListItem[]>([]);
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [selectedProjectKey, setSelectedProjectKey] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedQuestionId, setSelectedQuestionId] = useState<string | null>(null);
  const [sessionDetail, setSessionDetail] = useState<ConversationSessionDetail | null>(null);
  const [sessionView, setSessionView] = useState<"browser" | "detail">("browser");
  const [contentVisibility, setContentVisibility] = useState<ConversationContentVisibility>({
    ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  });
  const [selectedQuestionIds, setSelectedQuestionIds] = useState<Set<string>>(() => new Set());
  const [exportDialog, setExportDialog] = useState<ConversationExportDialogState | null>(null);
  const [exportVisibility, setExportVisibility] = useState<ConversationContentVisibility>({
    ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  });
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [importStep, setImportStep] = useState<ConversationImportStep>("idle");
  const [importing, setImporting] = useState(false);
  const [syncProgress, setSyncProgress] = useState<ConversationSyncProgressState | null>(null);
  const [syncProgressDismissed, setSyncProgressDismissed] = useState(false);
  const [query, setQuery] = useState("");
  const [detailQuery, setDetailQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState(
    webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations",
  );
  const [exporting, setExporting] = useState(false);
  const handledSyncTaskIdRef = useRef<string | null>(null);
  const syncRunning = syncTask?.status === "running";
  const [contentQuery, setContentQuery] = useState("");
  const [contentSearchCardTypes, setContentSearchCardTypes] =
    useState<ConversationSearchCardType[]>(CONVERSATION_SEARCH_CARD_TYPES);
  const [sessionSortBy, setSessionSortBy] = useState<ConversationSessionSortBy>("updated");
  const [sessionSortDirection, setSessionSortDirection] = useState<"asc" | "desc">("desc");
  const [questionSortBy, setQuestionSortBy] = useState<ConversationQuestionSortBy>("index");
  const [questionSortDirection, setQuestionSortDirection] = useState<"asc" | "desc">("asc");
  const [contentSearchResult, setContentSearchResult] = useState<ConversationSearchResultState | null>(null);
  const [contentSearchLoading, setContentSearchLoading] = useState(false);
  const [activeSearchTarget, setActiveSearchTarget] = useState<ConversationSearchTarget | null>(null);
  const importedSourceNamesRef = useRef<Map<string, string>>(new Map());

  const sessionQuestionCount = useMemo(() => sessions.reduce((total, session) => total + session.question_count, 0), [sessions]);
  const sortedSessions = useMemo(
    () => sortConversationSessions(sessions, sessionSortBy, sessionSortDirection),
    [sessionSortBy, sessionSortDirection, sessions],
  );
  const appGroups = useMemo(() => groupConversationSessionsByApp(adapters, sortedSessions), [adapters, sortedSessions]);
  const selectedAppGroup = useMemo(
    () => appGroups.find((group) => group.app.id === selectedAppId) ?? null,
    [appGroups, selectedAppId],
  );
  const selectedQuestion = useMemo(
    () => sessionDetail?.questions.find((question) => question.question.id === selectedQuestionId) ?? null,
    [selectedQuestionId, sessionDetail],
  );
  const visibleSessionQuestions = useMemo(
    () =>
      sortConversationQuestions(
        sessionDetail?.questions.filter((question) => questionMatchesQuery(question, detailQuery)) ?? [],
        questionSortBy,
        questionSortDirection,
      ),
    [detailQuery, questionSortBy, questionSortDirection, sessionDetail],
  );
  const selectedQuestionCount = selectedQuestionIds.size;
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
    setSelectedAppId(null);
    setSelectedProjectKey(null);
    setSelectedSessionId(null);
    setSelectedQuestionId(null);
    setSessionDetail(null);
    setSessionView("browser");
    setSelectedQuestionIds(new Set());
    setContentQuery("");
    setContentSearchCardTypes(CONVERSATION_SEARCH_CARD_TYPES);
    setContentSearchResult(null);
    setActiveSearchTarget(null);
    setImportDialogOpen(false);
    setImportStep("idle");
    setImporting(false);
    setSyncProgress(null);
    setSyncProgressDismissed(false);
    handledSyncTaskIdRef.current = null;
    setOutputRoot(
      webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations",
    );
    void refreshCatalog();
  }, [currentRecordKind]);

  useEffect(() => {
    void refreshSessions();
  }, [query, currentRecordKind]);

  useEffect(() => {
    const trimmedQuery = contentQuery.trim();
    if (!trimmedQuery) {
      setContentSearchResult(null);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;
    const contentTypes = contentSearchCardTypes.length > 0
      ? contentSearchCardTypes
      : CONVERSATION_SEARCH_CARD_TYPES;
    setContentSearchLoading(true);
    void searchConversationRecords({
      content_types: contentTypes,
      limit: 50,
      query: trimmedQuery,
      record_kind: currentRecordKind,
    })
      .then((result) => {
        if (cancelled) return;
        setContentSearchResult({
          contentTypes: result.scope?.content_types ?? contentTypes,
          hits: result.hits,
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
  }, [contentQuery, contentSearchCardTypes, currentRecordKind, onNotifyError]);

  const handleShowAllContentSearchCardTypes = useCallback(() => {
    setContentSearchCardTypes(CONVERSATION_SEARCH_CARD_TYPES);
  }, []);

  const handleToggleContentSearchCardType = useCallback((cardType: ConversationSearchCardType) => {
    setContentSearchCardTypes((current) => toggleConversationSearchCardType(current, cardType));
  }, []);

  useEffect(() => {
    setSelectedAppId((current) => {
      if (current && appGroups.some((group) => group.app.id === current)) return current;
      return appGroups.find((group) => group.sessions.length > 0)?.app.id ?? appGroups[0]?.app.id ?? null;
    });
  }, [appGroups]);

  useEffect(() => {
    if (!selectedAppGroup || !selectedSessionId) return;
    if (!selectedAppGroup.sessions.some((session) => session.id === selectedSessionId)) {
      setSelectedSessionId(null);
      setSessionDetail(null);
      setSessionView("browser");
    }
  }, [selectedAppGroup, selectedSessionId]);

  useEffect(() => {
    if (!selectedAppGroup) {
      setSelectedProjectKey(null);
      return;
    }
    setSelectedProjectKey((current) =>
      current && selectedAppGroup.projectGroups.some((group) => group.key === current)
        ? current
        : selectedAppGroup.projectGroups[0]?.key ?? null,
    );
  }, [selectedAppGroup]);

  useEffect(() => {
    if (!selectedSessionId) {
      setSessionDetail(null);
      return;
    }
    void loadSession(selectedSessionId);
  }, [selectedSessionId]);

  useEffect(() => {
    if (!selectedSessionId && sessionView === "detail") {
      setSessionView("browser");
    }
  }, [selectedSessionId, sessionView]);

  useEffect(() => {
    window.scrollTo({ top: 0, behavior: "auto" });
  }, [sessionView]);

  useEffect(() => {
    if (!sessionDetail) {
      setSelectedQuestionId(null);
      setSelectedQuestionIds(new Set());
      return;
    }
    setSelectedQuestionId((current) => preferredConversationQuestionId(sessionDetail.questions, current));
    setSelectedQuestionIds((current) => {
      const availableIds = new Set(sessionDetail.questions.map((question) => question.question.id));
      const next = new Set([...current].filter((questionId) => availableIds.has(questionId)));
      return next.size === current.size ? current : next;
    });
  }, [sessionDetail]);

  useEffect(() => {
    if (!activeSearchTarget || sessionDetail?.session.id !== activeSearchTarget.sessionId) {
      return;
    }
    if (sessionDetail.questions.some((question) => question.question.id === activeSearchTarget.questionId)) {
      setSelectedQuestionId(activeSearchTarget.questionId);
    }
  }, [activeSearchTarget, sessionDetail]);

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
      const nextAdapters = (await listConversationAdapters()).filter(
        (adapter) => isWebRecordAdapter(adapter) === webRecordMode,
      );
      setAdapters(nextAdapters);
      await refreshSessions({ rethrow: true });
    } catch (error) {
      if (options.rethrow) throw error;
      onNotifyError(errorMessage(error));
    }
  }

  async function refreshSessions(options: { rethrow?: boolean } = {}) {
    try {
      const listSessions = webRecordMode ? listWebRecordSessions : listConversationSessions;
      const nextSessions = await loadAllConversationSessionPages(listSessions, query || null);
      setSessions(nextSessions);
      setSelectedSessionId((current) => current && nextSessions.some((session) => session.id === current) ? current : null);
    } catch (error) {
      if (options.rethrow) throw error;
      onNotifyError(errorMessage(error));
    }
  }

  async function loadSession(sessionId: string) {
    try {
      const getSession = webRecordMode ? getWebRecordSession : getConversationSession;
      setSessionDetail(await getSession(sessionId));
    } catch (error) {
      onNotifyError(errorMessage(error));
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

  async function handleImport(values: ConversationImportFormValues) {
    setImporting(true);
    setImportStep("validating");
    try {
      const result = await importConversationSource(
        {
          config_json: values.config_json,
          manifest_path: values.manifest_path,
          record_kind: currentRecordKind,
          source_kind: values.source_kind,
          source_location: values.source_location,
          source_name: values.source_name,
        },
        (step) => setImportStep(step === "validating" ? "validating" : step === "source" ? "source" : "sync"),
        startSync,
      );
      importedSourceNamesRef.current.set(result.source.id, result.source.name);
      const sourceLabel = result.source.name;
      const summaryCounts = summarizeConversationSyncTask(result.task);
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
      const failureItems = formatConversationSyncFailureItems(result.task, syncSourceLabel, t);
      setSyncProgressDismissed(false);
      setSyncProgress({
        advice,
        failedStep: result.task.status === "failed" ? 3 : undefined,
        failureItems,
        phase:
          result.task.status === "failed"
            ? "failed"
            : result.task.status === "completed"
              ? "refreshing"
              : "importing",
        sourceLabel,
        summary,
        taskId: result.task.id,
      });
      onNotify({
        messageKey: "conversation.status.importStarted",
        messageParams: { source: result.source.name },
        tone: "info",
      });
      setImportDialogOpen(false);
      setImportStep("idle");
    } catch (error) {
      setImportStep("failed");
      onNotifyError(errorMessage(error));
    } finally {
      setImporting(false);
    }
  }

  async function pickImportManifest() {
    return selectFilePath(t("conversation.import.pickManifest"), ["json"]);
  }

  async function pickImportSourceLocation(kind: ConversationSourceKind) {
    if (kind === "directory" || kind === "custom") {
      return selectTargetDirectory(t("conversation.import.pickSourceDirectory"));
    }
    return selectFilePath(
      t("conversation.import.pickSourceFile"),
      kind === "sqlite" ? ["db", "sqlite", "sqlite3"] : undefined,
    );
  }

  async function handleMerge(previous: ConversationQuestionDetail, current: ConversationQuestionDetail) {
    try {
      await mergeConversationQuestions([previous.question.id, current.question.id], false);
      setSelectedQuestionId(previous.question.id);
      setSelectedQuestionIds((selectedIds) => {
        const next = new Set(selectedIds);
        if (next.delete(current.question.id)) {
          next.add(previous.question.id);
        }
        return next;
      });
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
    setSelectedSessionId(sessionId);
    setDetailQuery("");
    setActiveSearchTarget(null);
    setSelectedQuestionIds(new Set());
    setSessionView("detail");
  }, []);

  const handleAppSelect = useCallback((appId: string) => {
    setSelectedAppId(appId);
    setSelectedProjectKey(null);
  }, []);

  const handleOpenSearchHit = useCallback((hit: ConversationSearchHit) => {
    setSelectedAppId(hit.session.adapter_id);
    setSelectedProjectKey(normalizedProjectPath(hit.session) ?? NO_PROJECT_GROUP_KEY);
    setSelectedSessionId(hit.session.id);
    setSelectedQuestionId(hit.question_id);
    setDetailQuery("");
    setSelectedQuestionIds(new Set());
    setSessionView("detail");
    setActiveSearchTarget({
      blockId: hit.block_id,
      cardType: hit.card_type,
      questionId: hit.question_id,
      sessionId: hit.session.id,
    });
    if (hit.card_type !== "question") {
      setContentVisibility((current) => ({ ...current, [hit.card_type]: true }));
    }
  }, []);

  function handleQuestionSelectionChange(questionId: string, checked: boolean) {
    setSelectedQuestionIds((current) => {
      const next = new Set(current);
      if (checked) {
        next.add(questionId);
      } else {
        next.delete(questionId);
      }
      return next;
    });
  }

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
                disabled={syncRunning || importing}
                icon={<UploadCloud size={17} />}
                label={t("conversation.toolbar.import")}
                onClick={() => {
                  setImportStep("idle");
                  setImportDialogOpen(true);
                }}
                text={t("conversation.toolbar.import")}
              />
              <ToolbarActionButton
                disabled={syncRunning || importing}
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
              <ToolbarSearch
                className="w-[min(22rem,100%)] max-[980px]:w-64"
                onChange={setQuery}
                placeholder={t("conversation.toolbar.searchPlaceholder")}
                value={query}
              />
              <DebouncedToolbarSearch
                className="w-[min(24rem,100%)] max-[980px]:w-64"
                commitDelayMs={CONTENT_SEARCH_COMMIT_DELAY_MS}
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
        <div className="sticky top-[calc(var(--app-toolbar-top)+var(--app-notification-offset,0px))] z-10 -mx-[var(--app-page-x)] border-b border-theme-card-border bg-theme-toolbar/85 shadow-[0_12px_28px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur">
          <section
            aria-label={t("conversation.content.filterAria")}
            className="flex min-w-0 flex-nowrap items-center gap-3 overflow-hidden border-b border-theme-card-border/70 px-[var(--app-page-x)] py-3"
          >
            <ToolbarTextButton
              icon={<ArrowLeft size={16} />}
              label={t("conversation.session.backToBrowser")}
              onClick={() => setSessionView("browser")}
            />
            <ConversationContentFilter
              colors={appSettings.conversations.contentCardColors}
              onChange={(type, checked) =>
                setContentVisibility((current) => ({ ...current, [type]: checked }))
              }
              t={t}
              visibility={contentVisibility}
            />
          </section>
          <DataToolbar
            actions={
              <>
                <ToolbarActionButton
                  icon={<Download size={17} />}
                  label={t("conversation.session.exportMarkdown")}
                  onClick={() => openExportDialog("session")}
                  text={t("toolbar.export")}
                />
                <ToolbarActionButton
                  disabled={selectedQuestionCount === 0}
                  icon={<Download size={17} />}
                  label={t("conversation.toolbar.batchExportSelected", { count: selectedQuestionCount })}
                  onClick={handleBulkExport}
                  text={t("conversation.toolbar.batchExport")}
                />
                <ToolbarTextButton
                  icon={<Settings size={16} />}
                  label={t("toolbar.settings")}
                  onClick={() => onOpenSettings("conversations.sessions")}
                />
                <ToolbarActionButton
                  disabled={syncRunning || importing}
                  icon={<UploadCloud size={17} />}
                  label={t("conversation.toolbar.import")}
                  onClick={() => {
                    setImportStep("idle");
                    setImportDialogOpen(true);
                  }}
                  text={t("conversation.toolbar.import")}
                />
                <ToolbarActionButton
                  disabled={syncRunning || importing}
                  icon={<RefreshCw size={17} />}
                  label={syncRunning ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
                  onClick={() => void handleSync()}
                  primary
                  text={syncRunning ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
                />
              </>
            }
            ariaLabel={t("conversation.toolbar.aria")}
            className="px-[var(--app-page-x)] py-[var(--app-toolbar-y)]"
            compact={appSettings.conversations.sessionToolbarCompact}
            leading={
              <>
                <ToolbarSearch
                  className="w-[min(22rem,100%)] max-[980px]:w-64"
                  onChange={setDetailQuery}
                  placeholder={t("conversation.question.searchPlaceholder")}
                  value={detailQuery}
                />
                <ToolbarSingleSelectDropdown
                  ariaLabel={t("conversation.toolbar.questionSort")}
                  icon={<ArrowDownWideNarrow size={15} />}
                  onChange={setQuestionSortBy}
                  options={[
                    { label: t("conversation.toolbar.sort.original"), value: "index" },
                    { label: t("conversation.toolbar.sort.updated"), value: "updated" },
                    { label: t("toolbar.sort.name"), value: "title" },
                  ]}
                  value={questionSortBy}
                />
                <ToolbarSortDirectionButton
                  direction={questionSortDirection}
                  label={t("toolbar.sort.direction.label")}
                  onClick={() => setQuestionSortDirection((current) => (current === "desc" ? "asc" : "desc"))}
                  title={t(questionSortDirection === "desc" ? "toolbar.sort.direction.descTitle" : "toolbar.sort.direction.ascTitle")}
                />
              </>
            }
          />
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
          contentCardColors={appSettings.conversations.contentCardColors}
          loading={contentSearchLoading}
          onCardTypeToggle={handleToggleContentSearchCardType}
          onShowAllCardTypes={handleShowAllContentSearchCardTypes}
          onOpenHit={handleOpenSearchHit}
          result={contentSearchResult}
          selectedCardTypes={contentSearchCardTypes}
          t={t}
        />
      ) : null}
      {exportDialog ? (
        <ConversationExportDialog
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
          busy={importing}
          onClose={() => {
            setImportDialogOpen(false);
            setImportStep("idle");
          }}
          onImport={handleImport}
          onNotify={onNotify}
          onNotifyError={onNotifyError}
          onPickManifest={pickImportManifest}
          onPickSourceLocation={pickImportSourceLocation}
          onScriptInstalled={() => refreshCatalog()}
          recordKind={currentRecordKind}
          step={importStep}
        />
      ) : null}

      {sessionView === "browser" ? (
        <AppSessionBrowser
          appShortcuts={appShortcuts}
          columnMinWidth={appSettings.columnMinWidth}
          groups={appGroups}
          onAppSelect={handleAppSelect}
          onProjectSelect={setSelectedProjectKey}
          onSessionOpen={handleOpenSession}
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
          onQuestionSelect={setSelectedQuestionId}
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
          translationSettings={appSettings.conversationTranslation}
          visibility={contentVisibility}
        />
      )}
    </ConversationShell>
  );
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

export function DebouncedToolbarSearch({
  className,
  commitDelayMs,
  onChange,
  placeholder,
  resetSignal,
  searching = false,
  submitLabel,
  value,
}: {
  className?: string;
  commitDelayMs: number;
  onChange: (value: string) => void;
  placeholder: string;
  resetSignal?: string;
  searching?: boolean;
  submitLabel?: string;
  value: string;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const committedValueRef = useRef(value);
  const composingRef = useRef(false);
  const draftRef = useRef(value);
  const resetSignalRef = useRef(resetSignal);
  const onChangeRef = useRef(onChange);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    const resetSignalChanged = resetSignalRef.current !== resetSignal;
    resetSignalRef.current = resetSignal;
    if (!resetSignalChanged && value === committedValueRef.current) return;
    clearDebouncedSearchTimer(timerRef);
    composingRef.current = false;
    committedValueRef.current = value;
    draftRef.current = value;
    if (inputRef.current && inputRef.current.value !== value) {
      inputRef.current.value = value;
    }
  }, [resetSignal, value]);

  useEffect(() => {
    return () => clearDebouncedSearchTimer(timerRef);
  }, []);

  function commitDraft(nextValue: string) {
    if (nextValue === committedValueRef.current) return;
    committedValueRef.current = nextValue;
    onChangeRef.current(nextValue);
  }

  function commitCurrentDraft() {
    clearDebouncedSearchTimer(timerRef);
    commitDraft(draftRef.current);
  }

  function scheduleCommit(nextValue: string) {
    clearDebouncedSearchTimer(timerRef);
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      if (!composingRef.current) {
        commitDraft(nextValue);
      }
    }, commitDelayMs);
  }

  function handleChange(nextValue: string, event: ChangeEvent<HTMLInputElement>) {
    draftRef.current = nextValue;
    if (composingRef.current || inputEventIsComposing(event)) {
      clearDebouncedSearchTimer(timerRef);
      return;
    }
    scheduleCommit(nextValue);
  }

  function handleCompositionStart() {
    composingRef.current = true;
    clearDebouncedSearchTimer(timerRef);
  }

  function handleCompositionEnd(event: CompositionEvent<HTMLInputElement>) {
    composingRef.current = false;
    const nextValue = event.currentTarget.value;
    draftRef.current = nextValue;
    scheduleCommit(nextValue);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Enter" || composingRef.current) return;
    event.preventDefault();
    commitCurrentDraft();
  }

  return (
    <ToolbarSearch
      className={className}
      defaultValue={value}
      inputRef={inputRef}
      onChange={handleChange}
      onCompositionEnd={handleCompositionEnd}
      onCompositionStart={handleCompositionStart}
      onKeyDown={handleKeyDown}
      placeholder={placeholder}
      trailing={
        submitLabel ? (
          <button
            aria-label={submitLabel}
            className="grid size-7 shrink-0 place-items-center rounded-lg text-on-surface-muted transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-wait disabled:opacity-70"
            disabled={searching}
            onClick={commitCurrentDraft}
            title={submitLabel}
            type="button"
          >
            {searching ? <RefreshCw className="animate-spin" size={15} /> : <Search size={15} />}
          </button>
        ) : undefined
      }
    />
  );
}

function clearDebouncedSearchTimer(timerRef: { current: number | null }) {
  if (timerRef.current === null) return;
  window.clearTimeout(timerRef.current);
  timerRef.current = null;
}

function inputEventIsComposing(event: ChangeEvent<HTMLInputElement>) {
  return Boolean((event.nativeEvent as InputEvent).isComposing);
}

function ColumnPanel({ children, className = "", icon, title }: { children: ReactNode; className?: string; icon: ReactNode; title: string }) {
  return (
    <section className={`flex min-h-0 flex-col border-r border-theme-card-border last:border-r-0 max-[860px]:border-r-0 max-[860px]:border-b ${className}`}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-theme-card-border bg-theme-card-header/72 px-4">
        <span className="text-primary">{icon}</span>
        <h2 className="text-label-caps text-on-surface-variant">{title}</h2>
      </header>
      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
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

type ConversationExportMode = "session" | "questions";

interface ConversationExportDialogState {
  mode: ConversationExportMode;
  questionIds: string[];
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
        <div className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-control/55 px-3 py-3">
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
  selectedAppId: string | null;
  selectedProjectKey: string | null;
  t: Translator;
}) {
  const selectedGroup = groups.find((group) => group.app.id === selectedAppId) ?? null;
  const selectedProjectGroup =
    selectedGroup?.projectGroups.find((group) => group.key === selectedProjectKey) ??
    selectedGroup?.projectGroups[0] ??
    null;
  const selectedShortcut = selectedGroup ? findConversationAppShortcut(appShortcuts, selectedGroup.app) : null;

  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-session-browser mt-5 min-h-[620px] rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
      columns={[
        { defaultWeight: 0.3 },
        { defaultWeight: 0.62 },
        { defaultWeight: 1.08, minWidthScale: 1.25 },
      ]}
      handleClassName="max-[1040px]:hidden"
      minimumWidth={columnMinWidth}
      responsiveClassName="max-[1040px]:w-full max-[1040px]:grid-cols-1"
      scrollBarLabel={t("layout.scrollColumns")}
      scrollLeftLabel={t("layout.scrollColumnsLeft")}
      scrollRightLabel={t("layout.scrollColumnsRight")}
      storageKey="assetiweave.conversationBrowserColumns.v2"
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
      <section className="flex min-h-0 flex-col">
        <header className="flex min-h-16 shrink-0 items-center justify-between gap-4 border-b border-theme-card-border bg-theme-card-header/72 px-5 py-3">
          <div className="flex min-w-0 items-center gap-3">
            {selectedGroup ? <ConversationAppIcon appName={selectedGroup.app.name} shortcut={selectedShortcut} /> : null}
            <div className="min-w-0">
              <p className="text-label-caps text-primary">{t("conversation.column.sessions")}</p>
              <h2 className="mt-1 truncate text-title-sm text-on-surface">
                {selectedProjectGroup ? projectGroupLabel(selectedProjectGroup, t) : t("conversation.project.select")}
              </h2>
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
          ) : selectedGroup.projectGroups.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : !selectedProjectGroup ? (
            <EmptyPanel>{t("conversation.project.select")}</EmptyPanel>
          ) : selectedProjectGroup.sessions.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForProject")}</EmptyPanel>
          ) : (
            <div className="grid gap-3">
              {selectedProjectGroup.sessions.map((session) => (
                <SessionCard
                  key={session.id}
                  onOpen={() => onSessionOpen(session.id)}
                  session={session}
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
      className={`grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 border-l-2 border-b border-theme-card-border px-4 py-3 text-left transition-colors ${
        selected ? "border-l-primary bg-primary/10" : "border-l-transparent hover:bg-theme-card-header/70"
      }`}
      onClick={onSelect}
      type="button"
    >
      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary">
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

function toggleConversationSearchCardType(
  selectedCardTypes: ConversationSearchCardType[],
  cardType: ConversationSearchCardType,
) {
  if (allConversationSearchCardTypesSelected(selectedCardTypes)) {
    return [cardType];
  }
  if (selectedCardTypes.includes(cardType)) {
    if (selectedCardTypes.length === 1) return selectedCardTypes;
    return CONVERSATION_SEARCH_CARD_TYPES.filter((candidate) => (
      candidate !== cardType && selectedCardTypes.includes(candidate)
    ));
  }
  return CONVERSATION_SEARCH_CARD_TYPES.filter((candidate) => (
    candidate === cardType || selectedCardTypes.includes(candidate)
  ));
}

function allConversationSearchCardTypesSelected(selectedCardTypes: ConversationSearchCardType[]) {
  return searchResultMatchesSelectedCardTypes(CONVERSATION_SEARCH_CARD_TYPES, selectedCardTypes);
}

function ConversationContentSearchResults({
  contentCardColors,
  loading,
  onCardTypeToggle,
  onOpenHit,
  onShowAllCardTypes,
  result,
  selectedCardTypes,
  t,
}: {
  contentCardColors: ConversationContentCardColorSettings;
  loading: boolean;
  onCardTypeToggle: (cardType: ConversationSearchCardType) => void;
  onOpenHit: (hit: ConversationSearchHit) => void;
  onShowAllCardTypes: () => void;
  result: ConversationSearchResultState | null;
  selectedCardTypes: ConversationSearchCardType[];
  t: Translator;
}) {
  const hits = result?.hits ?? [];
  const allCardTypesSelected = allConversationSearchCardTypesSelected(selectedCardTypes);
  const visibleHits = allCardTypesSelected
    ? hits
    : hits.filter((hit) => selectedCardTypes.includes(hit.card_type));
  const query = result?.query ?? "";
  const resultMatchesActiveFilter = result
    ? searchResultMatchesSelectedCardTypes(result.contentTypes, selectedCardTypes)
    : false;
  const displayedTotalCount = resultMatchesActiveFilter && result ? result.totalCount : visibleHits.length;
  const groupedHits = CONVERSATION_SEARCH_CARD_TYPES
    .map((cardType) => ({
      cardType,
      hits: visibleHits.filter((hit) => hit.card_type === cardType),
    }))
    .filter((group) => group.hits.length > 0);

  return (
    <section
      aria-live="polite"
      className="mt-4 overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/72 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.14)]"
    >
      <header className="grid gap-3 border-b border-theme-card-border bg-theme-card-header/72 px-4 py-3 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
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
            className={`inline-flex h-8 shrink-0 items-center rounded-lg border px-2.5 text-label-caps transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 ${
              allCardTypesSelected
                ? "border-primary/50 bg-primary/12 text-primary"
                : "border-theme-control-border bg-theme-control/80 text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface"
            }`}
            onClick={onShowAllCardTypes}
            type="button"
          >
            {t("conversation.search.type.all")}
          </button>
          {CONVERSATION_SEARCH_CARD_TYPES.map((cardType) => (
            <SearchCardTypeFilterButton
              active={selectedCardTypes.includes(cardType)}
              cardType={cardType}
              colors={contentCardColors}
              disabled={selectedCardTypes.length === 1 && selectedCardTypes[0] === cardType}
              key={cardType}
              onClick={() => onCardTypeToggle(cardType)}
              t={t}
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
        <div className="grid divide-y divide-theme-card-border">
          {groupedHits.map((group) => (
            <section className="grid" key={group.cardType}>
              <header className="flex min-w-0 flex-wrap items-center justify-between gap-2 bg-theme-card-header/35 px-4 py-2">
                <SearchCardTypeBadge cardType={group.cardType} colors={contentCardColors} t={t} />
                <span className="text-code-sm text-on-surface-muted">
                  {t("conversation.search.groupCount", { count: group.hits.length })}
                </span>
              </header>
              <div className="grid divide-y divide-theme-card-border/70">
                {group.hits.map((hit) => (
                  <button
                    aria-label={t("conversation.search.openHit", {
                      title: hit.session.title,
                      type: conversationSearchCardTypeLabel(hit.card_type, t),
                    })}
                    className="grid gap-2 px-4 py-3 text-left transition-colors hover:bg-theme-card-header/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
                    key={`${hit.session.id}-${hit.block_id}-${hit.question_id}`}
                    onClick={() => onOpenHit(hit)}
                    type="button"
                  >
                    <span className="flex min-w-0 flex-wrap items-center gap-2">
                      <SearchCardTypeBadge cardType={hit.card_type} colors={contentCardColors} t={t} />
                      <span className="min-w-0 truncate text-body-sm font-semibold text-on-surface">
                        {hit.session.title}
                      </span>
                      <span className="min-w-0 truncate text-code-sm text-on-surface-muted">
                        {hit.question_title}
                      </span>
                    </span>
                    <span className="line-clamp-2 text-body-sm text-on-surface-variant">{hit.snippet}</span>
                    {hit.session.project_path ? (
                      <span className="truncate font-mono text-code-sm text-on-surface-muted">
                        {abbreviateHomePath(hit.session.project_path)}
                      </span>
                    ) : null}
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </section>
  );
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
  const palette = searchCardTypePalette(cardType, colors);
  return (
    <button
      aria-pressed={active}
      className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-theme-control-border px-2.5 text-label-caps text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55 disabled:cursor-default disabled:hover:bg-transparent"
      disabled={disabled}
      onClick={onClick}
      style={{
        backgroundColor: active ? palette.backgroundColor : undefined,
        borderColor: active ? palette.borderColor : undefined,
        color: active ? palette.accentColor : undefined,
      }}
      type="button"
    >
      <span className="size-2 rounded-full" style={{ backgroundColor: palette.accentColor }} />
      <span>{conversationSearchCardTypeLabel(cardType, t)}</span>
    </button>
  );
}

function searchResultMatchesSelectedCardTypes(
  contentTypes: ConversationSearchCardType[],
  selectedCardTypes: ConversationSearchCardType[],
) {
  if (contentTypes.length !== selectedCardTypes.length) return false;
  return CONVERSATION_SEARCH_CARD_TYPES.every((cardType) => (
    contentTypes.includes(cardType) === selectedCardTypes.includes(cardType)
  ));
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
  const palette = searchCardTypePalette(cardType, colors);
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
      {conversationSearchCardTypeLabel(cardType, t)}
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
  const accentColor = colors[cardType];
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
  return t(`conversation.content.${cardType}` as TranslationKey);
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
      className={`grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 border-l-2 border-b border-theme-card-border px-4 py-3 text-left transition-colors ${
        selected ? "border-l-primary bg-primary/10" : "border-l-transparent hover:bg-theme-card-header/70"
      }`}
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
      <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary">
        <AppWindow size={17} />
      </span>
    );
  }

  return (
    <span
      aria-label={appName}
      className="grid size-9 shrink-0 place-items-center rounded-lg border text-[13px] font-bold"
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
  t,
}: {
  onOpen: () => void;
  session: ConversationSessionListItem;
  t: Translator;
}) {
  const hashId = shortSessionHashId(session.id);

  return (
    <button
      aria-label={t("conversation.session.open", { title: session.title })}
      className="group grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-4 rounded-xl border border-theme-card-border bg-theme-card/75 px-4 py-4 text-left transition-all hover:-translate-y-0.5 hover:border-primary/45 hover:bg-theme-card"
      onClick={onOpen}
      type="button"
    >
      <span className="min-w-0">
        <span className="block truncate text-body-sm font-semibold text-on-surface">{session.title}</span>
        <span className="mt-1 block truncate font-mono text-code-sm text-on-surface-variant">
          {session.project_path ? abbreviateHomePath(session.project_path) : t("conversation.session.noProject")}
        </span>
        <span className="mt-3 inline-flex rounded-full bg-theme-control px-2 py-1 text-code-sm text-on-surface-muted">
          <span className="font-mono">{hashId}</span>
          <span className="px-1.5">·</span>
          <span>{t("conversation.session.counts", { questions: session.question_count, turns: session.turn_count })}</span>
        </span>
      </span>
      <span className="grid size-9 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-on-surface-variant transition-colors group-hover:text-primary">
        <ChevronRight size={17} />
      </span>
    </button>
  );
}

function shortSessionHashId(sessionId: string) {
  const trimmed = sessionId.trim();
  const hashMatch = trimmed.match(/-([a-f0-9]{12,})$/i);
  const hashId = hashMatch?.[1] ?? trimmed;
  return hashId.slice(0, 8);
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
  translationSettings?: ConversationTranslationSettings;
  visibility: ConversationContentVisibility;
}) {
  return (
    <ResizableColumns
      ariaLabel={t("layout.resizeColumns")}
      className="conversation-readable mt-5 min-h-[680px] rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)]"
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
        className="max-[920px]:border-r-0 max-[920px]:border-b"
        title={t("conversation.column.questions")}
        icon={<Layers3 size={16} />}
      >
        {!session ? (
          <EmptyPanel>{t("conversation.session.loading")}</EmptyPanel>
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
      <section className="min-h-0 min-w-0">
        {session && question ? (
          <QuestionPreview
            activeSearchTarget={activeSearchTarget}
            contentCardColors={contentCardColors}
            onExport={onExport}
            onCopyError={onCopyError}
            onPickOutputRoot={onPickOutputRoot}
            onSplit={onSplit}
            outputRoot={outputRoot}
            question={question}
            resultPreviewLineLimit={resultPreviewLineLimit}
            recordKind={recordKind}
            session={session}
            setOutputRoot={setOutputRoot}
            t={t}
            translationSettings={translationSettings}
            visibility={visibility}
          />
        ) : (
          <EmptyPanel>{t("conversation.question.noSelection")}</EmptyPanel>
        )}
      </section>
    </ResizableColumns>
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
    <article className={`flex h-48 flex-col overflow-hidden border-b border-theme-card-border ${selected ? "bg-primary/10" : "hover:bg-theme-card-header/70"}`}>
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
  recordKind?: ConversationRecordKind;
  resultPreviewLineLimit?: number;
  session: ConversationSessionDetail;
  setOutputRoot: (value: string) => void;
  t: Translator;
  translationSettings?: ConversationTranslationSettings;
  visibility?: ConversationContentVisibility;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);
  const [copiedPromptTurnId, setCopiedPromptTurnId] = useState<string | null>(null);
  const [pickingOutputRoot, setPickingOutputRoot] = useState(false);
  const copiedPromptResetTimerRef = useRef<number | null>(null);
  const activeBlockId = activeSearchTarget?.questionId === question.question.id ? activeSearchTarget.blockId : null;

  useEffect(
    () => () => {
      clearCopiedResetTimer(copiedPromptResetTimerRef);
    },
    [],
  );

  useEffect(() => {
    if (!activeBlockId) return;
    const frameId = window.requestAnimationFrame(() => {
      document
        .getElementById(conversationCardDomId(activeBlockId))
        ?.scrollIntoView({ behavior: "auto", block: "center" });
    });
    return () => window.cancelAnimationFrame(frameId);
  }, [activeBlockId]);

  async function handleCopyUserPrompt(turnId: string, value: string) {
    try {
      await writeClipboardText(value);
      clearCopiedResetTimer(copiedPromptResetTimerRef);
      setCopiedPromptTurnId(turnId);
      copiedPromptResetTimerRef.current = window.setTimeout(() => {
        setCopiedPromptTurnId((current) => (current === turnId ? null : current));
        copiedPromptResetTimerRef.current = null;
      }, 1400);
    } catch (error) {
      onCopyError?.(
        t("conversation.content.copyFailed", { message: errorMessage(error) }),
      );
    }
  }

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
      <header className="border-b border-theme-card-border bg-theme-card/74 px-5 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-label-caps text-primary">{questionOriginLabel(question.question.grouping_origin, t)}</p>
            <h2 className="mt-1 text-title-md text-on-surface">{title}</h2>
            <p className="mt-1 text-body-sm text-on-surface-variant">
              {session.session.project_path ? abbreviateHomePath(session.session.project_path) : t("conversation.session.noProject")}
            </p>
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
      <div className="min-h-0 flex-1 overflow-auto px-5 py-5">
        {question.turns.map((turn, index) => {
          const blocks = buildConversationContentBlocks(
            question.parts.filter((part) => part.turn_id === turn.id),
          );
          const promptBlockId = `${turn.id}-question`;
          const promptHighlighted = activeBlockId === promptBlockId;
          return (
            <section className="mb-6" key={turn.id}>
              <div
                className={`scroll-mt-32 rounded-xl border border-primary/30 bg-primary/[0.055] px-4 py-3 transition-shadow ${
                  promptHighlighted ? "ring-2 ring-primary/70 shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)]" : ""
                }`}
                data-conversation-card-id={promptBlockId}
                id={conversationCardDomId(promptBlockId)}
              >
                <div className="mb-2 flex items-center justify-between gap-3">
                  <h3 className="flex items-center gap-2 text-label-caps text-primary">
                    <span className="size-2 rounded-full bg-primary" />
                    {t("conversation.question.userPrompt")}
                  </h3>
                  <div className="flex items-center gap-2">
                    {index > 0 && onSplit ? (
                      <ToolbarTextButton icon={<Scissors size={15} />} label={t("conversation.question.splitHere")} onClick={() => void onSplit(question, turn.id)} />
                    ) : null}
                    <PromptCopyButton
                      copied={copiedPromptTurnId === turn.id}
                      onClick={() => void handleCopyUserPrompt(turn.id, turn.user_text)}
                      t={t}
                    />
                  </div>
                </div>
                <MarkdownContent value={turn.user_text} />
              </div>
              <div className="mt-3 pl-3">
                <h3 className="mb-3 text-label-caps text-on-surface-muted">{t("conversation.question.parts")}</h3>
                {blocks.length === 0 ? (
                  <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
                ) : (
                  <ConversationContentCards
                    activeBlockId={activeBlockId}
                    blocks={blocks}
                    colors={contentCardColors}
                    onCopyError={onCopyError}
                    recordKind={recordKind}
                    resultPreviewLineLimit={resultPreviewLineLimit}
                    t={t}
                    translationSettings={translationSettings}
                    visibility={visibility}
                  />
                )}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}

function PromptCopyButton({
  copied,
  onClick,
  t,
}: {
  copied: boolean;
  onClick: () => void;
  t: Translator;
}) {
  const label = copied
    ? t("conversation.content.copied")
    : t("conversation.question.copyPrompt");

  return (
    <button
      aria-label={label}
      className="inline-grid size-[1em] shrink-0 place-items-center rounded-[3px] text-label-caps text-primary/80 transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
      onClick={onClick}
      title={label}
      type="button"
    >
      {copied ? <Check className="size-[1em]" /> : <Copy className="size-[1em]" />}
    </button>
  );
}

function EmptyPanel({ children }: { children: ReactNode }) {
  return <div className="m-4 rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">{children}</div>;
}

function questionOriginLabel(origin: string, t: Translator) {
  const key = `conversation.question.origin.${origin}` as TranslationKey;
  return t(key);
}

function firstLine(value: string, t: Translator) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? t("conversation.markdown.untitledQuestion");
}

function questionMatchesQuery(question: ConversationQuestionDetail, query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;

  const searchable = [
    question.question.title,
    question.question.question_text,
    question.question.answer_text,
    question.question.code_text,
    question.question.command_text,
    ...question.turns.map((turn) => turn.user_text),
    ...question.parts.flatMap((part) => [part.text, part.command, part.cwd, part.language]),
  ];

  return searchable.some((value) => value?.toLowerCase().includes(normalized));
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

function clearCopiedResetTimer(timerRef: { current: number | null }) {
  if (timerRef.current === null) return;
  window.clearTimeout(timerRef.current);
  timerRef.current = null;
}

async function writeClipboardText(value: string) {
  if (typeof navigator === "undefined" || !navigator.clipboard?.writeText) {
    throw new Error("Clipboard API is unavailable");
  }
  await navigator.clipboard.writeText(value);
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
      ? summary.errorCount > 0
        ? "conversation.sync.web.summaryWithErrors"
        : "conversation.sync.web.summary"
      : summary.errorCount > 0
        ? "conversation.sync.summaryWithErrors"
        : "conversation.sync.summary",
    {
      errors: summary.errorCount,
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
