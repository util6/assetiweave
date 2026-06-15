import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from "react";
import {
  AppWindow,
  ArrowLeft,
  ChevronRight,
  Download,
  GitMerge,
  Layers3,
  RefreshCw,
  Scissors,
  Settings,
  X,
} from "lucide-react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarMetric,
  ToolbarSearch,
  ToolbarTextButton,
} from "../../components/common/DataToolbar";
import { AppShortcutIconForShortcut } from "../../components/apps/AppShortcutIcon";
import {
  buildConversationContentBlocks,
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
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { PageHeader } from "../../components/foundation/PageHeader";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  resolveFontFamilyCss,
  useAppSettings,
  type ConversationContentCardColorSettings,
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
  splitConversationQuestion,
  syncConversations,
} from "../../services/conversations";
import type {
  AppKind,
  AppShortcut,
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import { abbreviateHomePath } from "../../utils/path";

export { MarkdownContent } from "../../components/conversations/ConversationMarkdown";

export function ConversationsPage({
  appShortcuts,
  onManualOpen,
  onNotifyError,
  onOpenSettings,
  recordKind = "session",
}: {
  activeSubNavId?: string;
  appShortcuts: AppShortcut[];
  onManualOpen: () => void;
  onNotifyError: (message: string) => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
  recordKind?: "session" | "web";
}) {
  const { t } = useI18n();
  const { settings: appSettings } = useAppSettings();
  const webRecordMode = recordKind === "web";
  const [adapters, setAdapters] = useState<ConversationAdapter[]>([]);
  const [sessions, setSessions] = useState<ConversationSessionListItem[]>([]);
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
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
  const [syncProgress, setSyncProgress] = useState<ConversationSyncProgressState | null>(null);
  const [query, setQuery] = useState("");
  const [detailQuery, setDetailQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState(
    webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations",
  );
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  const sessionQuestionCount = useMemo(() => sessions.reduce((total, session) => total + session.question_count, 0), [sessions]);
  const appGroups = useMemo(() => groupConversationSessionsByApp(adapters, sessions), [adapters, sessions]);
  const selectedAppGroup = useMemo(
    () => appGroups.find((group) => group.app.id === selectedAppId) ?? null,
    [appGroups, selectedAppId],
  );
  const selectedQuestion = useMemo(
    () => sessionDetail?.questions.find((question) => question.question.id === selectedQuestionId) ?? null,
    [selectedQuestionId, sessionDetail],
  );
  const visibleSessionQuestions = useMemo(
    () => sessionDetail?.questions.filter((question) => questionMatchesQuery(question, detailQuery)) ?? [],
    [detailQuery, sessionDetail],
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
    setSelectedSessionId(null);
    setSelectedQuestionId(null);
    setSessionDetail(null);
    setSessionView("browser");
    setSelectedQuestionIds(new Set());
    setOutputRoot(
      webRecordMode ? "~/Desktop/assetiweave-web-records" : "~/Desktop/assetiweave-conversations",
    );
    void refreshCatalog();
  }, [recordKind]);

  useEffect(() => {
    void refreshSessions();
  }, [query, recordKind]);

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
    setSelectedQuestionId((current) =>
      current && sessionDetail.questions.some((question) => question.question.id === current)
        ? current
        : sessionDetail.questions[0]?.question.id ?? null,
    );
    setSelectedQuestionIds((current) => {
      const availableIds = new Set(sessionDetail.questions.map((question) => question.question.id));
      const next = new Set([...current].filter((questionId) => availableIds.has(questionId)));
      return next.size === current.size ? current : next;
    });
  }, [sessionDetail]);

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
      const nextSessions = await listSessions({ query: query || null, limit: 100, offset: 0 });
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
    let failedStep: 1 | 2 | 3 = 2;

    setLoading(true);
    setStatus(null);
    setSyncProgress({ phase: "preparing", sourceLabel });
    await waitForNextPaint();

    try {
      setSyncProgress({ phase: "importing", sourceLabel });
      const importingStartedAt = Date.now();
      await syncConversations({ source_id: null, dry_run: false });
      await waitForMinimumDuration(importingStartedAt, 450);

      failedStep = 3;
      setSyncProgress({ phase: "refreshing", sourceLabel });
      const refreshingStartedAt = Date.now();
      await refreshCatalog({ rethrow: true });
      await waitForMinimumDuration(refreshingStartedAt, 250);

      setSyncProgress({ phase: "completed", sourceLabel });
      setStatus(t(webRecordMode ? "conversation.webRecords.status.syncedAll" : "conversation.status.syncedAll"));
    } catch (error) {
      setSyncProgress({ failedStep, phase: "failed", sourceLabel });
      onNotifyError(errorMessage(error));
    } finally {
      setLoading(false);
    }
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
      setStatus(t("conversation.status.merged"));
      if (selectedSessionId) await loadSession(selectedSessionId);
      await refreshSessions();
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleSplit(question: ConversationQuestionDetail, turnId: string) {
    try {
      await splitConversationQuestion(question.question.id, turnId, false);
      setStatus(t("conversation.status.split"));
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
      setStatus(
        questionIds.length > 0
          ? t("conversation.status.exportedSelected", { count: questionIds.length })
          : t("conversation.status.exported"),
      );
      setExportDialog(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setExporting(false);
    }
  }

  function handleOpenSession(sessionId: string) {
    setSelectedSessionId(sessionId);
    setDetailQuery("");
    setSelectedQuestionIds(new Set());
    setSessionView("detail");
  }

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

  return (
    <ConversationShell
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
                disabled={loading}
                icon={<RefreshCw size={17} />}
                label={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
                onClick={() => void handleSync()}
                primary
                text={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
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
              <ToolbarMetric label={t("conversation.toolbar.apps")} value={appGroups.length} />
              <ToolbarMetric label={t("conversation.toolbar.sessions")} value={sessions.length} />
              <ToolbarMetric label={t("conversation.toolbar.questions")} value={sessionQuestionCount} />
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
                  disabled={loading}
                  icon={<RefreshCw size={17} />}
                  label={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
                  onClick={() => void handleSync()}
                  primary
                  text={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.sync")}
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
                <ToolbarMetric label={t("conversation.toolbar.questions")} value={sessionDetail?.questions.length ?? 0} />
                <ToolbarMetric label={t("conversation.toolbar.selected")} value={selectedQuestionCount} />
              </>
            }
          />
        </div>
      )}

      {syncProgress ? <ConversationSyncProgress state={syncProgress} t={t} /> : null}
      {status ? <div className="mt-4 rounded-xl border border-theme-card-border bg-theme-control px-4 py-2 text-body-sm text-on-surface">{status}</div> : null}
      {exportDialog ? (
        <ConversationExportDialog
          contentCardColors={appSettings.conversations.contentCardColors}
          exporting={exporting}
          mode={exportDialog.mode}
          onClose={() => setExportDialog(null)}
          onConfirm={handleConfirmExport}
          onOutputRootChange={setOutputRoot}
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

      {sessionView === "browser" ? (
        <AppSessionBrowser
          appShortcuts={appShortcuts}
          groups={appGroups}
          onAppSelect={setSelectedAppId}
          onSessionOpen={handleOpenSession}
          selectedAppId={selectedAppId}
          t={t}
        />
      ) : (
        <SessionQuestionWorkspace
          contentCardColors={appSettings.conversations.contentCardColors}
          onExport={() => openExportDialog("session")}
          onMerge={webRecordMode ? undefined : handleMerge}
          onQuestionSelect={setSelectedQuestionId}
          onQuestionSelectionChange={handleQuestionSelectionChange}
          onSplit={webRecordMode ? undefined : handleSplit}
          outputRoot={outputRoot}
          question={selectedQuestion}
          questions={visibleSessionQuestions}
          selectedQuestionId={selectedQuestionId}
          selectedQuestionIds={selectedQuestionIds}
          session={sessionDetail}
          setOutputRoot={setOutputRoot}
          t={t}
          visibility={contentVisibility}
        />
      )}
    </ConversationShell>
  );
}

function ConversationShell({
  children,
  onManualOpen,
  style,
  subtitle,
  t,
  title,
}: {
  children: ReactNode;
  onManualOpen: () => void;
  style?: CSSProperties;
  subtitle: string;
  t: Translator;
  title: string;
}) {
  return (
    <div className="flex w-full flex-1 flex-col px-[var(--app-page-x)] py-6" style={style}>
      <PageHeader
        className="mb-5"
        description={subtitle}
        eyebrow={t("conversation.eyebrow")}
        icon={<AppWindow size={21} />}
        title={title}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />
      {children}
    </div>
  );
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
        appKind: conversationAdapterKindToAppKind(adapter.kind),
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
    sessions,
    questionCount: sessions.reduce((total, session) => total + session.question_count, 0),
    turnCount: sessions.reduce((total, session) => total + session.turn_count, 0),
  };
}

export function ConversationExportDialog({
  contentCardColors,
  exporting,
  mode,
  onClose,
  onConfirm,
  onOutputRootChange,
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
  onVisibilityChange: (type: keyof ConversationContentVisibility, checked: boolean) => void;
  outputRoot: string;
  questionCount: number;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  const scopeLabel =
    mode === "questions"
      ? t("conversation.export.scopeQuestions", { count: questionCount })
      : t("conversation.export.scopeSession", { count: questionCount });

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
        <label className="grid gap-2">
          <span className="text-label-caps text-on-surface-muted">{t("conversation.session.outputRoot")}</span>
          <input
            className="h-10 w-full rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-body-sm text-on-surface outline-none transition-colors focus:border-primary/60"
            onChange={(event) => onOutputRootChange(event.target.value)}
            value={outputRoot}
          />
        </label>
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

function AppSessionBrowser({
  appShortcuts,
  groups,
  onAppSelect,
  onSessionOpen,
  selectedAppId,
  t,
}: {
  appShortcuts: AppShortcut[];
  groups: ConversationAppSessionGroup[];
  onAppSelect: (appId: string) => void;
  onSessionOpen: (sessionId: string) => void;
  selectedAppId: string | null;
  t: Translator;
}) {
  const selectedGroup = groups.find((group) => group.app.id === selectedAppId) ?? null;
  const selectedShortcut = selectedGroup ? findConversationAppShortcut(appShortcuts, selectedGroup.app) : null;

  return (
    <div className="conversation-session-browser mt-5 grid min-h-[620px] overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)] grid-cols-[minmax(250px,0.34fr)_minmax(0,1.66fr)] max-[860px]:grid-cols-1">
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
      <section className="flex min-h-0 flex-col">
        <header className="flex min-h-16 shrink-0 items-center justify-between gap-4 border-b border-theme-card-border bg-theme-card-header/72 px-5 py-3">
          <div className="flex min-w-0 items-center gap-3">
            {selectedGroup ? <ConversationAppIcon appName={selectedGroup.app.name} shortcut={selectedShortcut} /> : null}
            <div className="min-w-0">
              <p className="text-label-caps text-primary">{t("conversation.column.sessions")}</p>
              <h2 className="mt-1 truncate text-title-sm text-on-surface">
                {selectedGroup?.app.name ?? t("conversation.app.select")}
              </h2>
            </div>
          </div>
          {selectedGroup ? (
            <span className="shrink-0 rounded-full bg-theme-control px-3 py-1 text-code-sm text-on-surface-variant">
              {t("conversation.app.summary", {
                questions: selectedGroup.questionCount,
                sessions: selectedGroup.sessions.length,
                turns: selectedGroup.turnCount,
              })}
            </span>
          ) : null}
        </header>
        <div className="min-h-0 flex-1 overflow-auto p-4">
          {!selectedGroup ? (
            <EmptyPanel>{t("conversation.app.select")}</EmptyPanel>
          ) : selectedGroup.sessions.length === 0 ? (
            <EmptyPanel>{t("conversation.session.emptyForApp")}</EmptyPanel>
          ) : (
            <div className="grid gap-3">
              {selectedGroup.sessions.map((session) => (
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
    </div>
  );
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

function conversationAdapterKindToAppKind(kind: ConversationAdapter["kind"]): AppKind {
  switch (kind) {
    case "codex":
      return "codex";
    case "claude_code":
      return "claude";
    case "opencode":
      return "opencode";
    case "external":
      return "custom";
  }
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
          {t("conversation.session.counts", { questions: session.question_count, turns: session.turn_count })}
        </span>
      </span>
      <span className="grid size-9 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-on-surface-variant transition-colors group-hover:text-primary">
        <ChevronRight size={17} />
      </span>
    </button>
  );
}

export function SessionQuestionWorkspace({
  contentCardColors,
  onExport,
  onMerge,
  onQuestionSelect,
  onQuestionSelectionChange,
  onSplit,
  outputRoot,
  question,
  questions,
  selectedQuestionId,
  selectedQuestionIds,
  session,
  setOutputRoot,
  t,
  visibility,
}: {
  contentCardColors: ConversationContentCardColorSettings;
  onExport: () => void;
  onMerge?: (previous: ConversationQuestionDetail, current: ConversationQuestionDetail) => Promise<void>;
  onQuestionSelect: (questionId: string) => void;
  onQuestionSelectionChange: (questionId: string, checked: boolean) => void;
  onSplit?: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail | null;
  questions: ConversationQuestionDetail[];
  selectedQuestionId: string | null;
  selectedQuestionIds: Set<string>;
  session: ConversationSessionDetail | null;
  setOutputRoot: (value: string) => void;
  t: Translator;
  visibility: ConversationContentVisibility;
}) {
  return (
    <div className="conversation-readable mt-5 grid min-h-[680px] overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)] grid-cols-[minmax(260px,0.42fr)_minmax(0,1.58fr)] max-[920px]:grid-cols-1">
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
            contentCardColors={contentCardColors}
            onExport={onExport}
            onSplit={onSplit}
            outputRoot={outputRoot}
            question={question}
            session={session}
            setOutputRoot={setOutputRoot}
            t={t}
            visibility={visibility}
          />
        ) : (
          <EmptyPanel>{t("conversation.question.noSelection")}</EmptyPanel>
        )}
      </section>
    </div>
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
    <article className={`border-b border-theme-card-border ${selected ? "bg-primary/10" : "hover:bg-theme-card-header/70"}`}>
      <div className="grid grid-cols-[auto_minmax(0,1fr)]">
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
          className="min-w-0 py-3 pr-4 text-left"
          onClick={onSelect}
          type="button"
        >
          <div className="flex items-start justify-between gap-3">
            <h3 className="min-w-0 text-body-sm font-semibold text-on-surface">{title}</h3>
            <span className="shrink-0 rounded-full bg-theme-control px-2 py-1 text-code-sm text-on-surface-muted">
              {t("conversation.question.turnCount", { count: question.turns.length })}
            </span>
          </div>
          <p className="mt-2 line-clamp-2 text-body-sm text-on-surface-variant">{answerPreview}</p>
          <p className="mt-2 text-label-caps text-on-surface-muted">{questionOriginLabel(question.question.grouping_origin, t)}</p>
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
  contentCardColors,
  onExport,
  onSplit,
  outputRoot,
  question,
  session,
  setOutputRoot,
  t,
  visibility = DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
}: {
  contentCardColors?: ConversationContentCardColorSettings;
  onExport: () => void;
  onSplit?: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail;
  session: ConversationSessionDetail;
  setOutputRoot: (value: string) => void;
  t: Translator;
  visibility?: ConversationContentVisibility;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);

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
            <label className="min-w-64">
              <span className="sr-only">{t("conversation.session.outputRoot")}</span>
              <input
                className="h-10 w-full rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-body-sm text-on-surface outline-none"
                onChange={(event) => setOutputRoot(event.target.value)}
                value={outputRoot}
              />
            </label>
            <ToolbarActionButton icon={<Download size={17} />} label={t("conversation.session.exportMarkdown")} onClick={onExport} text={t("toolbar.export")} />
          </div>
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-auto px-5 py-5">
        {question.turns.map((turn, index) => {
          const blocks = buildConversationContentBlocks(
            question.parts.filter((part) => part.turn_id === turn.id),
          );
          return (
            <section className="mb-6" key={turn.id}>
              <div className="rounded-xl border border-primary/30 bg-primary/[0.055] px-4 py-3">
                <div className="mb-2 flex items-center justify-between gap-3">
                  <h3 className="flex items-center gap-2 text-label-caps text-primary">
                    <span className="size-2 rounded-full bg-primary" />
                    {t("conversation.question.userPrompt")}
                  </h3>
                  {index > 0 && onSplit ? (
                    <ToolbarTextButton icon={<Scissors size={15} />} label={t("conversation.question.splitHere")} onClick={() => void onSplit(question, turn.id)} />
                  ) : null}
                </div>
                <MarkdownContent value={turn.user_text} />
              </div>
              <div className="mt-3 pl-3">
                <h3 className="mb-3 text-label-caps text-on-surface-muted">{t("conversation.question.parts")}</h3>
                {blocks.length === 0 ? (
                  <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
                ) : (
                  <ConversationContentCards blocks={blocks} colors={contentCardColors} t={t} visibility={visibility} />
                )}
              </div>
            </section>
          );
        })}
      </div>
    </div>
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

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timeoutId);
      resolve();
    };
    const timeoutId = window.setTimeout(finish, 80);
    requestAnimationFrame(() => requestAnimationFrame(finish));
  });
}

function waitForMinimumDuration(startedAt: number, minimumDuration: number) {
  const remaining = Math.max(0, minimumDuration - (Date.now() - startedAt));
  return remaining > 0
    ? new Promise<void>((resolve) => window.setTimeout(resolve, remaining))
    : Promise.resolve();
}
