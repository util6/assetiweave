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
} from "lucide-react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarMetric,
  ToolbarSearch,
  ToolbarTextButton,
} from "../../components/common/DataToolbar";
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
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  resolveFontFamilyCss,
  useAppSettings,
  type SettingsPanelId,
} from "../../store/settings/AppSettingsProvider";
import {
  exportConversationSession,
  getConversationSession,
  listConversationAdapters,
  listConversationSessions,
  listConversationSources,
  mergeConversationQuestions,
  splitConversationQuestion,
  syncConversations,
} from "../../services/conversations";
import type {
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSessionDetail,
  ConversationSessionListItem,
  ConversationSource,
} from "../../types";
import { abbreviateHomePath } from "../../utils/path";

export { MarkdownContent } from "../../components/conversations/ConversationMarkdown";

export function ConversationsPage({
  activeSubNavId,
  onNotifyError,
  onOpenSettings,
}: {
  activeSubNavId: string;
  onNotifyError: (message: string) => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
}) {
  const { t } = useI18n();
  const { settings: appSettings } = useAppSettings();
  const [adapters, setAdapters] = useState<ConversationAdapter[]>([]);
  const [sources, setSources] = useState<ConversationSource[]>([]);
  const [sessions, setSessions] = useState<ConversationSessionListItem[]>([]);
  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedQuestionId, setSelectedQuestionId] = useState<string | null>(null);
  const [sessionDetail, setSessionDetail] = useState<ConversationSessionDetail | null>(null);
  const [sessionView, setSessionView] = useState<"browser" | "detail">("browser");
  const [contentVisibility, setContentVisibility] = useState<ConversationContentVisibility>({
    ...DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
  });
  const [syncProgress, setSyncProgress] = useState<ConversationSyncProgressState | null>(null);
  const [query, setQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState("~/Desktop/assetiweave-conversations");
  const [loading, setLoading] = useState(false);
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
  const conversationStyle = useMemo(
    () =>
      ({
        "--conversation-session-browser-font-family":
          resolveFontFamilyCss(appSettings.conversations.sessionBrowserFontFamily),
        "--conversation-session-browser-font-size": `${appSettings.conversations.sessionBrowserFontSize}px`,
        "--conversation-content-font-family": resolveFontFamilyCss(appSettings.conversations.contentFontFamily),
        "--conversation-content-font-size": `${appSettings.conversations.contentFontSize}px`,
        "--conversation-code-font-size": `${appSettings.conversations.codeFontSize}px`,
      }) as CSSProperties,
    [appSettings.conversations],
  );

  useEffect(() => {
    void refreshCatalog();
  }, []);

  useEffect(() => {
    void refreshSessions();
  }, [query]);

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
      return;
    }
    setSelectedQuestionId((current) =>
      current && sessionDetail.questions.some((question) => question.question.id === current)
        ? current
        : sessionDetail.questions[0]?.question.id ?? null,
    );
  }, [sessionDetail]);

  async function refreshCatalog(options: { rethrow?: boolean } = {}) {
    try {
      const [nextAdapters, nextSources] = await Promise.all([listConversationAdapters(), listConversationSources()]);
      setAdapters(nextAdapters);
      setSources(nextSources);
      await refreshSessions({ rethrow: true });
    } catch (error) {
      if (options.rethrow) throw error;
      onNotifyError(errorMessage(error));
    }
  }

  async function refreshSessions(options: { rethrow?: boolean } = {}) {
    try {
      const nextSessions = await listConversationSessions({ query: query || null, limit: 100, offset: 0 });
      setSessions(nextSessions);
      setSelectedSessionId((current) => current && nextSessions.some((session) => session.id === current) ? current : null);
    } catch (error) {
      if (options.rethrow) throw error;
      onNotifyError(errorMessage(error));
    }
  }

  async function loadSession(sessionId: string) {
    try {
      setSessionDetail(await getConversationSession(sessionId));
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleSync(sourceId?: string) {
    const sourceLabel = sourceId
      ? sources.find((source) => source.id === sourceId)?.name ?? sourceId
      : t("conversation.sync.allSources");
    let failedStep: 1 | 2 | 3 = 2;

    setLoading(true);
    setStatus(null);
    setSyncProgress({ phase: "preparing", sourceLabel });
    await waitForNextPaint();

    try {
      setSyncProgress({ phase: "importing", sourceLabel });
      const importingStartedAt = Date.now();
      await syncConversations({ source_id: sourceId ?? null, dry_run: false });
      await waitForMinimumDuration(importingStartedAt, 450);

      failedStep = 3;
      setSyncProgress({ phase: "refreshing", sourceLabel });
      const refreshingStartedAt = Date.now();
      await refreshCatalog({ rethrow: true });
      await waitForMinimumDuration(refreshingStartedAt, 250);

      setSyncProgress({ phase: "completed", sourceLabel });
      setStatus(sourceId ? t("conversation.status.syncedSource", { sourceId }) : t("conversation.status.syncedAll"));
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

  async function handleExport() {
    if (!selectedSessionId) return;
    try {
      await exportConversationSession(selectedSessionId, outputRoot, false);
      setStatus(t("conversation.status.exported"));
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  function handleOpenSession(sessionId: string) {
    setSelectedSessionId(sessionId);
    setSessionView("detail");
  }

  if (activeSubNavId === "sources") {
    return (
      <ConversationShell
        style={conversationStyle}
        title={t("conversation.sources.title")}
        subtitle={t("conversation.sources.subtitle")}
        t={t}
      >
        <DataToolbar
          actions={
            <>
              <ToolbarTextButton
                icon={<Settings size={16} />}
                label={t("toolbar.settings")}
                onClick={() => onOpenSettings("conversations.adapters")}
              />
              <ToolbarActionButton
                disabled={loading}
                icon={<RefreshCw size={17} />}
                label={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.syncAll")}
                onClick={() => void handleSync()}
                primary
                text={loading ? t("conversation.toolbar.syncing") : t("conversation.toolbar.syncAll")}
              />
            </>
          }
          ariaLabel={t("conversation.toolbar.aria")}
          compact={appSettings.conversations.sessionToolbarCompact}
          leading={<ToolbarMetric label={t("metric.sources")} value={sources.length} />}
          sticky
          stickyBleed
        />
        {syncProgress ? <ConversationSyncProgress state={syncProgress} t={t} /> : null}
        <div className="mt-5 grid gap-3">
          {sources.map((source) => (
            <div className="rounded-xl border border-theme-card-border bg-theme-card/80 p-4 shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.10)]" key={source.id}>
              <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 max-[760px]:grid-cols-1">
                <div className="min-w-0">
                  <h3 className="text-title-sm text-on-surface">{source.name}</h3>
                  <p className="mt-1 truncate font-mono text-code-sm text-on-surface-variant">{abbreviateHomePath(source.location)}</p>
                  <p className="mt-2 text-label-caps text-on-surface-muted">
                    {source.adapter_id} · {source.kind} · {source.enabled ? t("conversation.source.enabled") : t("conversation.source.disabled")}
                  </p>
                </div>
                <ToolbarTextButton
                  disabled={loading}
                  icon={<RefreshCw size={16} />}
                  label={loading ? t("conversation.toolbar.syncing") : t("conversation.source.sync")}
                  onClick={() => void handleSync(source.id)}
                />
              </div>
            </div>
          ))}
        </div>
      </ConversationShell>
    );
  }

  if (activeSubNavId === "adapters") {
    return (
      <ConversationShell
        style={conversationStyle}
        title={t("conversation.adapters.title")}
        subtitle={t("conversation.adapters.subtitle")}
        t={t}
      >
        <DataToolbar
          actions={
            <>
              <div className="text-body-sm text-on-surface-variant">{t("conversation.adapter.workflowDescription")}</div>
              <ToolbarTextButton
                icon={<Settings size={16} />}
                label={t("toolbar.settings")}
                onClick={() => onOpenSettings("conversations.adapters")}
              />
            </>
          }
          ariaLabel={t("conversation.toolbar.aria")}
          compact={appSettings.conversations.sessionToolbarCompact}
          leading={<ToolbarMetric label={t("conversation.toolbar.adapters")} value={adapters.length} />}
          sticky
          stickyBleed
        />
        <div className="mt-5 grid gap-3">
          {adapters.map((adapter) => (
            <div className="rounded-xl border border-theme-card-border bg-theme-card/80 p-4 shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.10)]" key={adapter.id}>
              <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 max-[760px]:grid-cols-1">
                <div className="min-w-0">
                  <h3 className="text-title-sm text-on-surface">{adapter.name}</h3>
                  <p className="mt-1 text-body-sm text-on-surface-variant">
                    {adapter.kind} · {adapter.trust_state} · v{adapter.version}
                  </p>
                  {adapter.manifest_path ? <p className="mt-2 truncate font-mono text-code-sm text-on-surface-muted">{abbreviateHomePath(adapter.manifest_path)}</p> : null}
                </div>
                <span className="rounded-full bg-theme-control px-3 py-1 text-label-caps text-on-surface-variant">
                  {adapter.enabled ? t("conversation.adapter.enabled") : t("conversation.adapter.disabled")}
                </span>
              </div>
            </div>
          ))}
        </div>
        <section className="mt-5 rounded-xl border border-theme-card-border bg-theme-control/70 p-4">
          <h2 className="text-title-sm text-on-surface">{t("conversation.adapter.workflowTitle")}</h2>
          <pre className="mt-3 overflow-auto rounded-lg bg-theme-card/80 p-4 text-code-sm text-on-surface-variant">
{`assetiweave-cli conversation adapter scaffold --directory ./my-adapter --id my-app --name "My App"
assetiweave-cli conversation adapter validate ./my-adapter/conversation-adapter.json
assetiweave-cli conversation adapter register ./my-adapter/conversation-adapter.json --yes
assetiweave-cli conversation adapter try-run ./my-adapter/conversation-adapter.json --method read_session --yes`}
          </pre>
        </section>
      </ConversationShell>
    );
  }

  return (
    <ConversationShell
      style={conversationStyle}
      title={t("conversation.sessions.title")}
      subtitle={t("conversation.sessions.subtitle")}
      t={t}
    >
      <DataToolbar
        actions={
          <>
            {sessionView === "detail" ? (
              <ConversationContentFilter
                onChange={(type, checked) =>
                  setContentVisibility((current) => ({ ...current, [type]: checked }))
                }
                t={t}
                visibility={contentVisibility}
              />
            ) : null}
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
          sessionView === "browser" ? (
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
          ) : (
            <>
              <ToolbarTextButton
                icon={<ArrowLeft size={16} />}
                label={t("conversation.session.backToBrowser")}
                onClick={() => setSessionView("browser")}
              />
              <div className="min-w-0">
                <p className="text-label-caps text-on-surface-muted">
                  {selectedAppGroup?.app.name ?? t("conversation.column.apps")}
                </p>
                <p className="truncate text-body-sm font-semibold text-on-surface">
                  {sessionDetail?.session.title ?? t("conversation.session.loading")}
                </p>
              </div>
              <ToolbarMetric
                label={t("conversation.toolbar.questions")}
                value={sessionDetail?.questions.length ?? 0}
              />
            </>
          )
        }
        sticky
        stickyBleed
      />

      {syncProgress ? <ConversationSyncProgress state={syncProgress} t={t} /> : null}
      {status ? <div className="mt-4 rounded-xl border border-theme-card-border bg-theme-control px-4 py-2 text-body-sm text-on-surface">{status}</div> : null}

      {sessionView === "browser" ? (
        <AppSessionBrowser
          groups={appGroups}
          onAppSelect={setSelectedAppId}
          onSessionOpen={handleOpenSession}
          selectedAppId={selectedAppId}
          t={t}
        />
      ) : (
        <SessionQuestionWorkspace
          onExport={handleExport}
          onMerge={handleMerge}
          onQuestionSelect={setSelectedQuestionId}
          onSplit={handleSplit}
          outputRoot={outputRoot}
          question={selectedQuestion}
          selectedQuestionId={selectedQuestionId}
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
  style,
  subtitle,
  t,
  title,
}: {
  children: ReactNode;
  style?: CSSProperties;
  subtitle: string;
  t: Translator;
  title: string;
}) {
  return (
    <div className="mx-auto flex w-full max-w-7xl flex-1 flex-col px-[var(--app-page-x)] py-6" style={style}>
      <div className="mb-5">
        <p className="text-label-caps text-primary">{t("conversation.eyebrow")}</p>
        <h1 className="mt-1 text-display-sm text-on-surface">{title}</h1>
        <p className="mt-2 max-w-3xl text-body-sm text-on-surface-variant">{subtitle}</p>
      </div>
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
  id: string;
  name: string;
}

export interface ConversationAppSessionGroup {
  app: ConversationAppSummary;
  sessions: ConversationSessionListItem[];
  questionCount: number;
  turnCount: number;
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
        { id: appId, name: appId },
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

function AppSessionBrowser({
  groups,
  onAppSelect,
  onSessionOpen,
  selectedAppId,
  t,
}: {
  groups: ConversationAppSessionGroup[];
  onAppSelect: (appId: string) => void;
  onSessionOpen: (sessionId: string) => void;
  selectedAppId: string | null;
  t: Translator;
}) {
  const selectedGroup = groups.find((group) => group.app.id === selectedAppId) ?? null;

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
              t={t}
            />
          ))
        )}
      </ColumnPanel>
      <section className="flex min-h-0 flex-col">
        <header className="flex min-h-16 shrink-0 items-center justify-between gap-4 border-b border-theme-card-border bg-theme-card-header/72 px-5 py-3">
          <div className="min-w-0">
            <p className="text-label-caps text-primary">{t("conversation.column.sessions")}</p>
            <h2 className="mt-1 truncate text-title-sm text-on-surface">
              {selectedGroup?.app.name ?? t("conversation.app.select")}
            </h2>
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
  t,
}: {
  group: ConversationAppSessionGroup;
  onSelect: () => void;
  selected: boolean;
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
      <span className="grid size-9 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary">
        <AppWindow size={17} />
      </span>
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

function SessionQuestionWorkspace({
  onExport,
  onMerge,
  onQuestionSelect,
  onSplit,
  outputRoot,
  question,
  selectedQuestionId,
  session,
  setOutputRoot,
  t,
  visibility,
}: {
  onExport: () => Promise<void>;
  onMerge: (previous: ConversationQuestionDetail, current: ConversationQuestionDetail) => Promise<void>;
  onQuestionSelect: (questionId: string) => void;
  onSplit: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail | null;
  selectedQuestionId: string | null;
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
        ) : (
          session.questions.map((item, index) => (
            <QuestionListItem
              key={item.question.id}
              onMergeWithPrevious={
                index > 0 ? () => void onMerge(session.questions[index - 1], item) : undefined
              }
              onSelect={() => onQuestionSelect(item.question.id)}
              question={item}
              selected={item.question.id === selectedQuestionId}
              t={t}
            />
          ))
        )}
      </ColumnPanel>
      <section className="min-h-0 min-w-0">
        {session && question ? (
          <QuestionPreview
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
  question,
  selected,
  t,
}: {
  onMergeWithPrevious?: () => void;
  onSelect: () => void;
  question: ConversationQuestionDetail;
  selected: boolean;
  t: Translator;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);
  const answerPreview = firstLine(question.question.answer_text || question.question.command_text || question.question.code_text, t);

  return (
    <article className={`border-b border-theme-card-border ${selected ? "bg-primary/10" : "hover:bg-theme-card-header/70"}`}>
      <button
        aria-label={t("conversation.question.select", { title })}
        className="w-full px-4 py-3 text-left"
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
      {onMergeWithPrevious ? (
        <div className="px-4 pb-3">
          <ToolbarTextButton icon={<GitMerge size={15} />} label={t("conversation.question.mergePrevious")} onClick={onMergeWithPrevious} />
        </div>
      ) : null}
    </article>
  );
}

export function QuestionPreview({
  onExport,
  onSplit,
  outputRoot,
  question,
  session,
  setOutputRoot,
  t,
  visibility = DEFAULT_CONVERSATION_CONTENT_VISIBILITY,
}: {
  onExport: () => Promise<void>;
  onSplit: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
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
            <ToolbarActionButton icon={<Download size={17} />} label={t("conversation.session.exportMarkdown")} onClick={() => void onExport()} text={t("toolbar.export")} />
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
                  {index > 0 ? (
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
                  <ConversationContentCards blocks={blocks} t={t} visibility={visibility} />
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
