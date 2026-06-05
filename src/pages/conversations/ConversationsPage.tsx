import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Braces, Database, Download, FileText, GitMerge, Layers3, RefreshCw, Scissors } from "lucide-react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarMetric,
  ToolbarSearch,
  ToolbarTextButton,
} from "../../components/common/DataToolbar";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
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
  ConversationPart,
  ConversationPartKind,
  ConversationPartRole,
  ConversationQuestionDetail,
  ConversationSessionDetail,
  ConversationSessionListItem,
  ConversationSource,
} from "../../types";

export function ConversationsPage({
  activeSubNavId,
  onNotifyError,
}: {
  activeSubNavId: string;
  onNotifyError: (message: string) => void;
}) {
  const { t } = useI18n();
  const [adapters, setAdapters] = useState<ConversationAdapter[]>([]);
  const [sources, setSources] = useState<ConversationSource[]>([]);
  const [sessions, setSessions] = useState<ConversationSessionListItem[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedQuestionId, setSelectedQuestionId] = useState<string | null>(null);
  const [sessionDetail, setSessionDetail] = useState<ConversationSessionDetail | null>(null);
  const [query, setQuery] = useState("");
  const [outputRoot, setOutputRoot] = useState("~/Desktop/assetiweave-conversations");
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  const adapterNameById = useMemo(() => new Map(adapters.map((adapter) => [adapter.id, adapter.name])), [adapters]);
  const sessionQuestionCount = useMemo(() => sessions.reduce((total, session) => total + session.question_count, 0), [sessions]);
  const sessionTurnCount = useMemo(() => sessions.reduce((total, session) => total + session.turn_count, 0), [sessions]);
  const selectedQuestion = useMemo(
    () => sessionDetail?.questions.find((question) => question.question.id === selectedQuestionId) ?? null,
    [selectedQuestionId, sessionDetail],
  );

  useEffect(() => {
    void refreshCatalog();
  }, []);

  useEffect(() => {
    void refreshSessions();
  }, [query]);

  useEffect(() => {
    if (!selectedSessionId) {
      setSessionDetail(null);
      return;
    }
    void loadSession(selectedSessionId);
  }, [selectedSessionId]);

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

  async function refreshCatalog() {
    try {
      const [nextAdapters, nextSources] = await Promise.all([listConversationAdapters(), listConversationSources()]);
      setAdapters(nextAdapters);
      setSources(nextSources);
      await refreshSessions();
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function refreshSessions() {
    try {
      const nextSessions = await listConversationSessions({ query: query || null, limit: 100, offset: 0 });
      setSessions(nextSessions);
      setSelectedSessionId((current) => current ?? nextSessions[0]?.id ?? null);
    } catch (error) {
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
    setLoading(true);
    setStatus(null);
    try {
      await syncConversations({ source_id: sourceId ?? null, dry_run: false });
      setStatus(sourceId ? t("conversation.status.syncedSource", { sourceId }) : t("conversation.status.syncedAll"));
      await refreshCatalog();
    } catch (error) {
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

  if (activeSubNavId === "sources") {
    return (
      <ConversationShell title={t("conversation.sources.title")} subtitle={t("conversation.sources.subtitle")} t={t}>
        <DataToolbar
          actions={
            <ToolbarActionButton
              disabled={loading}
              icon={<RefreshCw size={17} />}
              label={t("conversation.toolbar.syncAll")}
              onClick={() => void handleSync()}
              primary
              text={t("conversation.toolbar.syncAll")}
            />
          }
          ariaLabel={t("conversation.toolbar.aria")}
          leading={<ToolbarMetric label={t("metric.sources")} value={sources.length} />}
        />
        <div className="mt-5 grid gap-3">
          {sources.map((source) => (
            <div className="rounded-xl border border-theme-card-border bg-theme-card/80 p-4 shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.10)]" key={source.id}>
              <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 max-[760px]:grid-cols-1">
                <div className="min-w-0">
                  <h3 className="text-title-sm text-on-surface">{source.name}</h3>
                  <p className="mt-1 truncate font-mono text-code-sm text-on-surface-variant">{source.location}</p>
                  <p className="mt-2 text-label-caps text-on-surface-muted">
                    {source.adapter_id} · {source.kind} · {source.enabled ? t("conversation.source.enabled") : t("conversation.source.disabled")}
                  </p>
                </div>
                <ToolbarTextButton
                  disabled={loading}
                  icon={<RefreshCw size={16} />}
                  label={t("conversation.source.sync")}
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
      <ConversationShell title={t("conversation.adapters.title")} subtitle={t("conversation.adapters.subtitle")} t={t}>
        <DataToolbar
          actions={<div className="text-body-sm text-on-surface-variant">{t("conversation.adapter.workflowDescription")}</div>}
          ariaLabel={t("conversation.toolbar.aria")}
          leading={<ToolbarMetric label={t("conversation.toolbar.adapters")} value={adapters.length} />}
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
                  {adapter.manifest_path ? <p className="mt-2 truncate font-mono text-code-sm text-on-surface-muted">{adapter.manifest_path}</p> : null}
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
    <ConversationShell title={t("conversation.sessions.title")} subtitle={t("conversation.sessions.subtitle")} t={t}>
      <DataToolbar
        actions={
          <ToolbarActionButton
            disabled={loading}
            icon={<RefreshCw className={loading ? "animate-spin" : undefined} size={17} />}
            label={t("conversation.toolbar.sync")}
            onClick={() => void handleSync()}
            primary
            text={t("conversation.toolbar.sync")}
          />
        }
        ariaLabel={t("conversation.toolbar.aria")}
        leading={
          <>
            <ToolbarSearch
              className="w-[min(28rem,100%)]"
              onChange={setQuery}
              placeholder={t("conversation.toolbar.searchPlaceholder")}
              value={query}
            />
            <ToolbarMetric label={t("conversation.toolbar.sessions")} value={sessions.length} />
            <ToolbarMetric label={t("conversation.toolbar.questions")} value={sessionQuestionCount} />
            <ToolbarMetric label={t("conversation.toolbar.turns")} value={sessionTurnCount} />
          </>
        }
      />

      {status ? <div className="mt-4 rounded-xl border border-theme-card-border bg-theme-control px-4 py-2 text-body-sm text-on-surface">{status}</div> : null}

      <div className="mt-5 grid min-h-[620px] overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/70 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.18)] grid-cols-[minmax(260px,0.74fr)_minmax(320px,0.95fr)_minmax(420px,1.28fr)] max-[1180px]:grid-cols-[minmax(260px,0.82fr)_minmax(0,1.18fr)] max-[860px]:grid-cols-1">
        <ColumnPanel title={t("conversation.column.sessions")} icon={<Database size={16} />}>
          {sessions.length === 0 ? (
            <EmptyPanel>{t("conversation.session.empty")}</EmptyPanel>
          ) : (
            sessions.map((session) => (
              <SessionListItem
                adapterName={adapterNameById.get(session.adapter_id) ?? session.adapter_id}
                key={session.id}
                onSelect={() => setSelectedSessionId(session.id)}
                selected={session.id === selectedSessionId}
                session={session}
                t={t}
              />
            ))
          )}
        </ColumnPanel>

        <ColumnPanel title={t("conversation.column.questions")} icon={<Layers3 size={16} />}>
          {sessionDetail ? (
            sessionDetail.questions.length === 0 ? (
              <EmptyPanel>{t("conversation.question.empty")}</EmptyPanel>
            ) : (
              sessionDetail.questions.map((question, index) => (
                <QuestionListItem
                  key={question.question.id}
                  onMergeWithPrevious={index > 0 ? () => void handleMerge(sessionDetail.questions[index - 1], question) : undefined}
                  onSelect={() => setSelectedQuestionId(question.question.id)}
                  question={question}
                  selected={question.question.id === selectedQuestionId}
                  t={t}
                />
              ))
            )
          ) : (
            <EmptyPanel>{t("conversation.session.noSelection")}</EmptyPanel>
          )}
        </ColumnPanel>

        <ColumnPanel className="max-[1180px]:col-span-2 max-[860px]:col-span-1" title={t("conversation.column.preview")} icon={<FileText size={16} />}>
          {sessionDetail && selectedQuestion ? (
            <QuestionPreview
              onExport={handleExport}
              onSplit={handleSplit}
              outputRoot={outputRoot}
              question={selectedQuestion}
              session={sessionDetail}
              setOutputRoot={setOutputRoot}
              t={t}
            />
          ) : (
            <EmptyPanel>{t("conversation.question.noSelection")}</EmptyPanel>
          )}
        </ColumnPanel>
      </div>
    </ConversationShell>
  );
}

function ConversationShell({ children, subtitle, t, title }: { children: ReactNode; subtitle: string; t: Translator; title: string }) {
  return (
    <div className="mx-auto flex w-full max-w-7xl flex-1 flex-col px-[var(--app-page-x)] py-6">
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

function SessionListItem({
  adapterName,
  onSelect,
  selected,
  session,
  t,
}: {
  adapterName: string;
  onSelect: () => void;
  selected: boolean;
  session: ConversationSessionListItem;
  t: Translator;
}) {
  return (
    <button
      aria-label={t("conversation.session.select", { title: session.title })}
      className={`grid w-full grid-cols-[minmax(0,1fr)_auto] gap-3 border-l-2 border-b border-theme-card-border px-4 py-3 text-left transition-colors ${
        selected ? "border-l-primary bg-primary/10" : "border-l-transparent hover:bg-theme-card-header/70"
      }`}
      onClick={onSelect}
      type="button"
    >
      <span className="min-w-0">
        <span className="block truncate text-body-sm font-semibold text-on-surface">{session.title}</span>
        <span className="mt-1 block truncate text-code-sm text-on-surface-variant">{adapterName}</span>
      </span>
      <span className="self-start rounded-full bg-theme-control px-2 py-1 text-code-sm text-on-surface-muted">
        {t("conversation.session.counts", { questions: session.question_count, turns: session.turn_count })}
      </span>
    </button>
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
}: {
  onExport: () => Promise<void>;
  onSplit: (question: ConversationQuestionDetail, turnId: string) => Promise<void>;
  outputRoot: string;
  question: ConversationQuestionDetail;
  session: ConversationSessionDetail;
  setOutputRoot: (value: string) => void;
  t: Translator;
}) {
  const title = question.question.title || firstLine(question.question.question_text, t);

  return (
    <div className="flex min-h-full flex-col">
      <header className="border-b border-theme-card-border bg-theme-card/74 px-5 py-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-label-caps text-primary">{questionOriginLabel(question.question.grouping_origin, t)}</p>
            <h2 className="mt-1 text-title-md text-on-surface">{title}</h2>
            <p className="mt-1 text-body-sm text-on-surface-variant">{session.session.project_path ?? t("conversation.session.noProject")}</p>
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
          const parts = question.parts.filter((part) => part.turn_id === turn.id);
          return (
            <section className="mb-5 rounded-xl border border-theme-card-border bg-theme-card/74" key={turn.id}>
              <div className="border-b border-theme-card-border px-4 py-3">
                <div className="mb-2 flex items-center justify-between gap-3">
                  <h3 className="text-label-caps text-on-surface-muted">{t("conversation.question.userPrompt")}</h3>
                  {index > 0 ? (
                    <ToolbarTextButton icon={<Scissors size={15} />} label={t("conversation.question.splitHere")} onClick={() => void onSplit(question, turn.id)} />
                  ) : null}
                </div>
                <MarkdownContent value={turn.user_text} />
              </div>
              <div className="px-4 py-3">
                <h3 className="mb-3 text-label-caps text-on-surface-muted">{t("conversation.question.parts")}</h3>
                {parts.length === 0 ? (
                  <EmptyPanel>{t("conversation.markdown.empty")}</EmptyPanel>
                ) : (
                  <div className="grid gap-3">
                    {parts.map((part) => (
                      <PartPreview key={part.id} part={part} t={t} />
                    ))}
                  </div>
                )}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}

function PartPreview({ part, t }: { part: ConversationPart; t: Translator }) {
  const label = `${partRoleLabel(part.role, t)} · ${partKindLabel(part.kind, t)}`;
  if (part.kind === "code_block") {
    return (
      <section className="overflow-hidden rounded-lg border border-theme-card-border bg-theme-control/70">
        <PartHeader icon={<Braces size={14} />} label={part.language ? `${label} · ${part.language}` : label} />
        <pre className="overflow-auto p-3 text-code-sm text-on-surface">
          <code>{part.text}</code>
        </pre>
      </section>
    );
  }

  if (part.kind === "command") {
    return (
      <section className="overflow-hidden rounded-lg border border-theme-card-border bg-theme-control/70">
        <PartHeader icon={<Braces size={14} />} label={label} />
        <pre className="overflow-auto p-3 text-code-sm text-on-surface">
          <code>{part.command ?? part.text}</code>
        </pre>
      </section>
    );
  }

  return (
    <section className="rounded-lg border border-theme-card-border bg-theme-control/55 p-3">
      <PartHeader icon={<FileText size={14} />} label={label} />
      <div className="mt-2">
        <MarkdownContent value={part.text ?? ""} />
      </div>
    </section>
  );
}

function PartHeader({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-2 border-b border-theme-card-border/70 px-3 py-2 text-label-caps text-on-surface-muted">
      <span className="text-primary">{icon}</span>
      <span>{label}</span>
    </div>
  );
}

function EmptyPanel({ children }: { children: ReactNode }) {
  return <div className="m-4 rounded-xl border border-dashed border-theme-card-border p-6 text-center text-body-sm text-on-surface-variant">{children}</div>;
}

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] }
  | { type: "quote"; text: string }
  | { type: "code"; language: string | null; text: string };

export function MarkdownContent({ value }: { value: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(value), [value]);
  if (blocks.length === 0) {
    return <p className="text-body-sm text-on-surface-muted">{value.trim() ? value : ""}</p>;
  }

  return (
    <div className="space-y-3 text-body-sm leading-6 text-on-surface">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          return renderMarkdownHeading(block, index);
        }
        if (block.type === "list") {
          return (
            <ul className="list-disc space-y-1 pl-5" key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ul>
          );
        }
        if (block.type === "quote") {
          return (
            <blockquote className="border-l-2 border-primary/60 pl-3 text-on-surface-variant" key={index}>
              {renderInlineMarkdown(block.text)}
            </blockquote>
          );
        }
        if (block.type === "code") {
          return (
            <pre className="overflow-auto rounded-lg bg-theme-control p-3 text-code-sm text-on-surface" key={index}>
              <code>{block.text}</code>
            </pre>
          );
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

function renderMarkdownHeading(block: Extract<MarkdownBlock, { type: "heading" }>, key: number) {
  const content = renderInlineMarkdown(block.text);
  if (block.level <= 1) {
    return (
      <h3 className="text-title-sm text-on-surface" key={key}>
        {content}
      </h3>
    );
  }
  if (block.level === 2) {
    return (
      <h4 className="text-body-sm font-semibold text-on-surface" key={key}>
        {content}
      </h4>
    );
  }
  if (block.level === 3) {
    return (
      <h5 className="text-body-sm font-semibold text-on-surface" key={key}>
        {content}
      </h5>
    );
  }
  return (
    <h6 className="text-label-caps text-on-surface-muted" key={key}>
      {content}
    </h6>
  );
}

function parseMarkdownBlocks(value: string): MarkdownBlock[] {
  const lines = value.replace(/\r\n/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let paragraph: string[] = [];
  let list: string[] = [];
  let codeLanguage: string | null = null;
  let codeLines: string[] = [];

  function flushParagraph() {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ").trim() });
      paragraph = [];
    }
  }

  function flushList() {
    if (list.length > 0) {
      blocks.push({ type: "list", items: list });
      list = [];
    }
  }

  for (const line of lines) {
    const fence = line.match(/^```(\w+)?\s*$/);
    if (fence) {
      if (codeLanguage !== null) {
        blocks.push({ type: "code", language: codeLanguage, text: codeLines.join("\n") });
        codeLanguage = null;
        codeLines = [];
      } else {
        flushParagraph();
        flushList();
        codeLanguage = fence[1] ?? "";
      }
      continue;
    }

    if (codeLanguage !== null) {
      codeLines.push(line);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const listItem = line.match(/^\s*[-*]\s+(.+)$/);
    if (listItem) {
      flushParagraph();
      list.push(listItem[1].trim());
      continue;
    }

    const quote = line.match(/^>\s?(.+)$/);
    if (quote) {
      flushParagraph();
      flushList();
      blocks.push({ type: "quote", text: quote[1].trim() });
      continue;
    }

    paragraph.push(line.trim());
  }

  flushParagraph();
  flushList();
  if (codeLanguage !== null) {
    blocks.push({ type: "code", language: codeLanguage, text: codeLines.join("\n") });
  }

  return blocks.filter((block) => ("text" in block ? block.text.trim() : block.items.length > 0));
}

function renderInlineMarkdown(text: string) {
  const parts: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text))) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith("`")) {
      parts.push(
        <code className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary" key={`${match.index}-code`}>
          {token.slice(1, -1)}
        </code>,
      );
    } else {
      parts.push(
        <strong className="font-semibold text-on-surface" key={`${match.index}-strong`}>
          {token.slice(2, -2)}
        </strong>,
      );
    }
    lastIndex = match.index + token.length;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts;
}

function questionOriginLabel(origin: string, t: Translator) {
  const key = `conversation.question.origin.${origin}` as TranslationKey;
  return t(key);
}

function partRoleLabel(role: ConversationPartRole, t: Translator) {
  return t(`conversation.part.role.${role}` as TranslationKey);
}

function partKindLabel(kind: ConversationPartKind, t: Translator) {
  return t(`conversation.part.kind.${kind}` as TranslationKey);
}

function firstLine(value: string, t: Translator) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? t("conversation.markdown.untitledQuestion");
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
