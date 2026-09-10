import {
  Brain,
  CheckCircle2,
  Clock,
  LoaderCircle,
  MessageCircle,
  Plus,
  Search,
  Square,
  XCircle,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  cancelMemoryRecallTurn,
  createMemoryRecallSession,
  emptyMemoryScope,
  getMemoryRecallSession,
  searchMemoryRecall,
  sendMemoryRecallTurn,
} from "../../services/memory";
import type {
  MemoryNavigationTarget,
  MemoryRecallContentReference,
  MemoryRecallSession,
  MemoryRecallTurn,
} from "../../types/memory";
import type { Translator } from "../../i18n/I18nProvider";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export function MemoryRecallWorkspace({
  onNavigate,
  t,
}: {
  onNavigate?: (target: MemoryNavigationTarget) => void;
  t: Translator;
}) {
  const [session, setSession] = useState<MemoryRecallSession | null>(null);
  const [projectPath, setProjectPath] = useState("");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void createMemoryRecallSession(emptyMemoryScope())
      .then((nextSession) => {
        if (!cancelled) setSession(nextSession);
      })
      .catch((loadError) => {
        if (!cancelled) setError(errorMessage(loadError));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!session?.activeTurnId) return;
    let polling = false;
    const interval = window.setInterval(() => {
      if (polling) return;
      polling = true;
      void getMemoryRecallSession(session.id)
        .then(setSession)
        .catch((pollError) => setError(errorMessage(pollError)))
        .finally(() => {
          polling = false;
        });
    }, 1000);
    return () => window.clearInterval(interval);
  }, [session?.activeTurnId, session?.id]);

  async function startSession() {
    setBusy(true);
    setError(null);
    try {
      const nextSession = await createMemoryRecallSession({
        ...emptyMemoryScope(),
        project_path: projectPath.trim() || null,
      });
      setSession(nextSession);
      setQuery("");
    } catch (startError) {
      setError(errorMessage(startError));
    } finally {
      setBusy(false);
    }
  }

  async function sendTurn() {
    if (!session || !query.trim() || session.activeTurnId) return;
    setBusy(true);
    setError(null);
    try {
      setSession(await sendMemoryRecallTurn(session.id, query));
      setQuery("");
    } catch (sendError) {
      setError(errorMessage(sendError));
    } finally {
      setBusy(false);
    }
  }

  async function cancelTurn() {
    const turnId = session?.activeTurnId;
    if (!turnId) return;
    setBusy(true);
    setError(null);
    try {
      setSession(await cancelMemoryRecallTurn(turnId));
    } catch (cancelError) {
      setError(errorMessage(cancelError));
    } finally {
      setBusy(false);
    }
  }

  if (loading) {
    return (
      <EmptyState
        description={t("common.loading")}
        icon={<LoaderCircle className="animate-spin" size={20} />}
        title={t("common.loading")}
      />
    );
  }
  if (!session) {
    return (
      <EmptyState
        description={t("memory.recall.browserDescription")}
        icon={<Search size={20} />}
        title={t("memory.recall.browserTitle")}
      />
    );
  }

  const activeTurn = session.turns.find(
    (turn) => turn.id === session.activeTurnId,
  );

  return (
    <div className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(20rem,0.75fr)_minmax(32rem,1.65fr)]">
      {/* 左栏：会话范围控制面板 */}
      <Panel className="flex min-h-0 flex-col overflow-y-auto p-4" variant="default">
        <div className="flex items-center gap-2 border-b border-theme-control-border/60 pb-3 text-title-sm font-semibold text-on-surface">
          <Brain className="size-4 text-primary" />
          <span>{t("memory.recall.session")}</span>
        </div>

        <div className="mt-4 flex flex-col gap-4">
          <label className="flex flex-col gap-1.5 text-body-sm">
            <span className="font-medium text-on-surface">
              {t("memory.recall.projectPath")}
            </span>
            <Input
              aria-label={t("memory.recall.projectPath")}
              disabled={session.turns.length > 0}
              onChange={(event) => setProjectPath(event.target.value)}
              placeholder={t("memory.recall.projectPlaceholder")}
              value={projectPath}
            />
          </label>

          <div className="flex flex-wrap gap-2">
            <Button
              disabled={busy || Boolean(session.activeTurnId)}
              onClick={() => void startSession()}
              size="sm"
              variant="outline"
            >
              <Plus className="mr-1.5 size-3.5" />
              {t("memory.recall.newSession")}
            </Button>
            {activeTurn ? (
              <Button
                disabled={busy}
                onClick={() => void cancelTurn()}
                size="sm"
                variant="destructive"
              >
                <Square className="mr-1.5 size-3.5" />
                {t("common.cancel")}
              </Button>
            ) : null}
          </div>

          <div className="flex flex-col gap-2 rounded-xl border border-theme-control-border/60 bg-theme-control/25 p-3.5 text-body-sm">
            <div className="flex items-center justify-between text-caption">
              <span className="text-on-surface-variant">{t("memory.recall.turnCount")}:</span>
              <strong className="font-mono text-on-surface">{session.turnCount}</strong>
            </div>
            <div className="flex items-center justify-between text-caption">
              <span className="text-on-surface-variant">{t("memory.recall.agent")}:</span>
              <span className="font-mono text-on-surface">
                {session.agentId}
                {session.model ? ` · ${session.model}` : ""}
              </span>
            </div>
            <div className="flex items-center justify-between text-caption">
              <span className="text-on-surface-variant">{t("memory.recall.status")}:</span>
              <span className="font-medium text-primary">
                {turnStatusLabel(session, t)}
              </span>
            </div>
          </div>

          {error ? (
            <div className="rounded-xl border border-status-remove/40 bg-status-remove/10 p-3 text-body-sm text-status-remove">
              {error}
            </div>
          ) : null}
        </div>
      </Panel>

      {/* 右栏：对话回溯工作区 */}
      <Panel className="flex min-h-0 flex-1 flex-col overflow-hidden p-4" variant="default">
        <div className="flex items-center gap-2 border-b border-theme-control-border/60 pb-3 text-title-sm font-semibold text-on-surface">
          <MessageCircle className="size-4 text-primary" />
          <span>{t("memory.recall.conversation")}</span>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-4 pt-3">
          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
            {session.turns.length === 0 ? (
              <EmptyState
                description={t("memory.recall.emptyDescription")}
                icon={<MessageCircle size={20} />}
                title={t("memory.recall.emptyTitle")}
              />
            ) : (
              session.turns.map((turn) => (
                <RecallTurn
                  key={turn.id}
                  onNavigate={(reference) =>
                    void openReference(
                      reference,
                      turn,
                      session,
                      onNavigate,
                      setError,
                      t,
                    )
                  }
                  t={t}
                  turn={turn}
                />
              ))
            )}
          </div>

          <div className="flex flex-col gap-2.5 border-t border-theme-control-border/60 pt-3">
            <textarea
              aria-label={t("memory.recall.query")}
              className="min-h-20 w-full resize-none rounded-xl border border-theme-control-border bg-theme-control/40 p-3 text-body-sm text-on-surface outline-none placeholder:text-outline transition-all focus:border-primary focus:bg-theme-control/60"
              disabled={Boolean(session.activeTurnId) || busy}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                  event.preventDefault();
                  void sendTurn();
                }
              }}
              placeholder={t("memory.recall.queryPlaceholder")}
              value={query}
            />
            <div className="flex items-center justify-between gap-3">
              <span className="text-caption text-on-surface-variant">
                {t("memory.recall.sendHint")}
              </span>
              <Button
                disabled={busy || Boolean(session.activeTurnId) || !query.trim()}
                onClick={() => void sendTurn()}
                size="sm"
              >
                {busy ? (
                  <LoaderCircle className="mr-1.5 size-3.5 animate-spin" />
                ) : (
                  <Search className="mr-1.5 size-3.5" />
                )}
                {t("memory.recall.send")}
              </Button>
            </div>
          </div>
        </div>
      </Panel>
    </div>
  );
}

function RecallTurn({
  onNavigate,
  t,
  turn,
}: {
  onNavigate: (reference: MemoryRecallContentReference) => void;
  t: Translator;
  turn: MemoryRecallTurn;
}) {
  return (
    <article className="flex flex-col gap-2.5 rounded-xl border border-theme-control-border/60 bg-surface-elevated/40 p-4 shadow-sm">
      <div className="flex items-center justify-between gap-3 text-caption text-on-surface-variant">
        <span className="font-semibold text-on-surface">
          {t("memory.recall.turn")} {turn.sequence + 1}
        </span>
        <span className="inline-flex items-center gap-1">
          {turn.status === "running" ? (
            <LoaderCircle className="size-3 animate-spin text-primary" />
          ) : turn.status === "completed" ? (
            <CheckCircle2 className="size-3 text-status-create" />
          ) : turn.status === "failed" ? (
            <XCircle className="size-3 text-status-remove" />
          ) : (
            <Clock className="size-3 text-outline" />
          )}
          <span>{turnStatusLabel(turn, t)}</span>
        </span>
      </div>

      <div className="rounded-lg bg-theme-control/40 px-3 py-2 text-body-sm text-on-surface">
        {turn.userText}
      </div>

      {turn.structuredOutput ? (
        <div className="flex flex-col gap-2.5 pt-1">
          <div className="whitespace-pre-wrap text-body-sm text-on-surface">
            {turn.structuredOutput.answer}
          </div>
          {turn.structuredOutput.contentReferences.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {turn.structuredOutput.contentReferences.map((reference, index) => (
                <Button
                  key={`${reference.blockId}:${index}`}
                  onClick={() => onNavigate(reference)}
                  size="sm"
                  variant="outline"
                >
                  {t("memory.recall.openReference")} {index + 1}
                </Button>
              ))}
            </div>
          ) : null}
          {turn.structuredOutput.followUpSuggestions.length > 0 ? (
            <div className="flex flex-col gap-1 rounded-lg border border-theme-control-border/40 bg-theme-control/20 p-2.5 text-caption text-on-surface-variant">
              <span className="font-medium text-on-surface">
                {t("memory.recall.followUps")}
              </span>
              {turn.structuredOutput.followUpSuggestions.map((suggestion) => (
                <span key={suggestion}>· {suggestion}</span>
              ))}
            </div>
          ) : null}
        </div>
      ) : turn.lastError ? (
        <div className="rounded-lg border border-status-remove/30 bg-status-remove/10 p-2.5 text-caption text-status-remove">
          {turn.lastError}
        </div>
      ) : null}
    </article>
  );
}

function turnStatusLabel(
  value: MemoryRecallSession | MemoryRecallTurn,
  t?: Translator,
) {
  const status =
    "turns" in value
      ? (value.turns.find((turn) => turn.id === value.activeTurnId)?.status ??
        value.status)
      : value.status;
  if (!t) return status;
  return t(`memory.recall.status.${status}` as never);
}

async function openReference(
  reference: MemoryRecallContentReference,
  turn: MemoryRecallTurn,
  session: MemoryRecallSession,
  onNavigate: ((target: MemoryNavigationTarget) => void) | undefined,
  setError: (message: string) => void,
  t: Translator,
) {
  if (!onNavigate) return;
  try {
    const result = await searchMemoryRecall({
      query: turn.userText,
      scope: session.scope,
      limit: 50,
    });
    const hit = result?.hits.find(
      (item) =>
        item.block_id === reference.blockId &&
        item.question_id === reference.questionId,
    );
    if (!hit) {
      setError(t("memory.recall.referenceUnavailable"));
      return;
    }
    onNavigate({
      record_kind: hit.record_kind,
      source_id: hit.source_id,
      session_id: hit.session_id,
      question_id: hit.question_id,
      turn_id: hit.turn_id,
      part_id: hit.part_id,
      block_id: hit.block_id,
    });
  } catch (referenceError) {
    setError(errorMessage(referenceError));
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
