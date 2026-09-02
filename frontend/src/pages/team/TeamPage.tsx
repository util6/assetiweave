import {
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  Edit2,
  MessageSquare,
  Play,
  Plus,
  RotateCcw,
  Shield,
  Trash2,
  Users,
} from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { EmptyState } from "../../components/foundation/EmptyState";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { PageHeader } from "../../components/foundation/PageHeader";
import { Panel } from "../../components/foundation/Panel";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { useI18n } from "../../i18n/I18nProvider";
import { useOptionalTeamTasks } from "../../app/backgroundTasks/TeamTaskProvider";
import { TeamSessionProvider } from "../../app/backgroundTasks/TeamSessionProvider";
import {
  listAgentCatalog,
  listAgentMarket,
  listAgentModels,
  type AgentMarketItem,
  type AgentModelOption,
  type AgentRuntimeCatalogEntry,
} from "../../services/agentRuntime";
import {
  createTeam,
  deleteTeam,
  listTeams,
  updateTeam,
} from "../../services/team";
import {
  confirmTeamRun,
  cancelTeamRun,
  draftTeam,
  getLatestTeamRun,
  getTeamRun,
  restoreTeamRun,
  reviewTeamRun,
  teamLeaderChat,
} from "../../services/teamWorkflow";
import { teamRestoreTaskResultSchema } from "../../schemas/teamWorkflow";
import type {
  TeamDetail,
  TeamMemberInput,
  TeamRunSnapshot,
  TeamRestoreSnapshot,
  TeamRole,
} from "../../types/team";

interface MemberDraft extends TeamMemberInput {
  role: TeamRole;
  agent_id: string;
  model?: string | null;
}

const selectClassName =
  "h-9 w-full rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3 text-body-sm text-on-surface shadow-[var(--theme-shadow-control-inset)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/35 disabled:cursor-not-allowed disabled:opacity-50";

export function TeamPage() {
  const { t } = useI18n();
  const [teams, setTeams] = useState<TeamDetail[]>([]);
  const [selectedTeamId, setSelectedTeamId] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<AgentRuntimeCatalogEntry[]>([]);
  const [market, setMarket] = useState<AgentMarketItem[]>([]);
  const [models, setModels] = useState<Record<string, AgentModelOption[]>>({});
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<TeamDetail | "new" | null>(null);
  const [deleting, setDeleting] = useState<TeamDetail | null>(null);
  const [formName, setFormName] = useState("");
  const [formDescription, setFormDescription] = useState("");
  const [formMembers, setFormMembers] = useState<MemberDraft[]>([]);
  const [formError, setFormError] = useState<string | null>(null);
  const [runSnapshot, setRunSnapshot] = useState<TeamRunSnapshot | null>(null);
  const [leaderMessage, setLeaderMessage] = useState("");
  const [workflowBusy, setWorkflowBusy] = useState(false);
  const [workflowError, setWorkflowError] = useState<string | null>(null);
  const [leaderChatMessage, setLeaderChatMessage] = useState("");
  const [leaderChatResult, setLeaderChatResult] = useState<string | null>(null);
  const [leaderChatBusy, setLeaderChatBusy] = useState(false);
  const [restoreResult, setRestoreResult] = useState<TeamRestoreSnapshot | null>(null);
  const [restoreTaskId, setRestoreTaskId] = useState<string | null>(null);
  const teamTasks = useOptionalTeamTasks();

  const installedAgents = useMemo(() => {
    const installed = new Map(
      market
        .filter((item) => item.installed?.enabled && item.installed.executionReady)
        .map((item) => [item.id, item]),
    );
    const activeCatalog = catalog.filter((entry) => {
      const item = installed.get(entry.id);
      return market.length === 0 || Boolean(item);
    });
    return activeCatalog;
  }, [catalog, market]);

  const agentCanFillTeam = (agentId: string) => {
    const marketItem = market.find((item) => item.id === agentId);
    if (!marketItem) return market.length === 0;
    const capabilities = marketItem.installed?.capabilities ?? marketItem.capabilities;
    return Boolean(
      capabilities.resume
      && capabilities.historyReplay
      && capabilities.liveEvents,
    );
  };

  const selectableAgents = () => installedAgents.filter((agent) => agentCanFillTeam(agent.id));

  const selectedTeam = teams.find((team) => team.id === selectedTeamId) ?? null;

  const runTeammates = runSnapshot?.run.roster_snapshot.filter((member) => member.role === "teammate") ?? [];
  const runState = runSnapshot?.run.state;
  const orderedRunTasks = runSnapshot ? [...runSnapshot.tasks].sort((left, right) => left.sort_order - right.sort_order) : [];
  const completedRunTasks = orderedRunTasks.filter((task) => ["succeeded", "failed", "canceled"].includes(task.state)).length;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void Promise.all([listTeams(), listAgentCatalog(), listAgentMarket({ installedOnly: true })])
      .then(([loadedTeams, loadedCatalog, loadedMarket]) => {
        if (cancelled) return;
        setTeams(loadedTeams);
        setCatalog(loadedCatalog);
        setMarket(loadedMarket);
        setSelectedTeamId((current) =>
          current && loadedTeams.some((team) => team.id === current)
            ? current
            : loadedTeams[0]?.id ?? null,
        );
      })
      .catch((loadError: unknown) => {
        if (!cancelled) setError(t("team.error.load", { message: errorMessage(loadError) }));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  useEffect(() => {
    setRunSnapshot(null);
    setRestoreResult(null);
    setRestoreTaskId(null);
    setWorkflowError(null);
    setLeaderChatMessage("");
    setLeaderChatResult(null);
    if (!selectedTeamId) return;
    let cancelled = false;
    void getLatestTeamRun(selectedTeamId)
      .then((snapshot) => {
        if (!cancelled) setRunSnapshot(snapshot);
      })
      .catch((loadError: unknown) => {
        if (!cancelled) setWorkflowError(errorMessage(loadError));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedTeamId]);

  const restoreTask = restoreTaskId ? teamTasks?.getTask(restoreTaskId) : undefined;

  useEffect(() => {
    if (!restoreTask || !["Succeeded", "Failed", "Canceled"].includes(restoreTask.state)) return;
    if (restoreTask.state === "Succeeded") {
      const parsed = teamRestoreTaskResultSchema.safeParse(restoreTask.result);
      if (parsed.success) {
        const currentRun = runSnapshot;
        if (!currentRun || currentRun.run.id !== parsed.data.run_id) {
          setWorkflowError(t("team.workflow.error", { message: t("team.workflow.restoreInvalid") }));
          setRestoreTaskId(null);
          setWorkflowBusy(false);
          return;
        }
        setRestoreResult({
          run: currentRun,
          leader: null,
          leader_error_code: parsed.data.leader_error_code,
          members: parsed.data.members,
        });
        void teamLeaderChat({ team_id: currentRun.run.team_id, message: "", replay: true })
          .then((result) => {
            setLeaderChatResult(result.text);
            setRestoreResult((current) => current ? { ...current, leader: result } : current);
          })
          .catch(() => undefined);
      } else {
        setWorkflowError(t("team.workflow.error", { message: t("team.workflow.restoreInvalid") }));
      }
    } else {
      setWorkflowError(t("team.workflow.error", {
        message: restoreTask.error?.message ?? t("team.workflow.restoreFailed"),
      }));
    }
    setRestoreTaskId(null);
    setWorkflowBusy(false);
  }, [restoreTask, runSnapshot, t]);

  useEffect(() => {
    if (!runSnapshot || runSnapshot.run.state === "terminal") return;
    let cancelled = false;
    const refresh = () => {
      void getTeamRun(runSnapshot.run.id)
        .then((snapshot) => {
          if (!cancelled && snapshot) setRunSnapshot(snapshot);
        })
        .catch((loadError: unknown) => {
          if (!cancelled) setWorkflowError(errorMessage(loadError));
        });
    };
    const intervalId = window.setInterval(refresh, 1000);
    refresh();
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [runSnapshot?.run.id, runSnapshot?.run.state]);

  const loadModels = async (agentId: string) => {
    if (!agentId || models[agentId]) return;
    try {
      const result = await listAgentModels(agentId);
      setModels((current) => ({ ...current, [agentId]: result.available ? result.models : [] }));
    } catch (loadError: unknown) {
      setError(t("team.error.models", { message: errorMessage(loadError) }));
    }
  };

  const openCreate = () => {
    const first = selectableAgents()[0];
    const second = selectableAgents()[0] ?? first;
    setFormName("");
    setFormDescription("");
    setFormMembers([
      { role: "leader", agent_id: first?.id ?? "", model: null },
      { role: "teammate", agent_id: second?.id ?? "", model: null },
    ]);
    setFormError(null);
    setEditor("new");
    if (first) void loadModels(first.id);
    if (second) void loadModels(second.id);
  };

  const openEdit = (team: TeamDetail) => {
    setFormName(team.name);
    setFormDescription(team.description ?? "");
    setFormMembers(
      [...team.members]
        .sort((left, right) => left.sort_order - right.sort_order)
        .map((member) => ({
          id: member.id,
          role: member.role,
          sort_order: member.sort_order,
          agent_id: member.agent_id,
          model: member.model,
        })),
    );
    setFormError(null);
    setEditor(team);
    for (const member of team.members) void loadModels(member.agent_id);
  };

  const updateMember = (index: number, patch: Partial<MemberDraft>) => {
    setFormMembers((current) => current.map((member, memberIndex) =>
      memberIndex === index ? { ...member, ...patch } : member,
    ));
    if (patch.agent_id) void loadModels(patch.agent_id);
  };

  const moveMember = (index: number, direction: -1 | 1) => {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= formMembers.length) return;
    setFormMembers((current) => {
      const next = [...current];
      [next[index], next[nextIndex]] = [next[nextIndex], next[index]];
      return next;
    });
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    const name = formName.trim();
    const leaderCount = formMembers.filter((member) => member.role === "leader").length;
    if (!name) {
      setFormError(t("team.validation.name"));
      return;
    }
    if (leaderCount !== 1 || formMembers.length - leaderCount < 1) {
      setFormError(t("team.validation.roster"));
      return;
    }
    if (installedAgents.length === 0 || formMembers.some((member) => !installedAgents.some((agent) => agent.id === member.agent_id))) {
      setFormError(t("team.validation.agent"));
      return;
    }
    for (const member of formMembers) {
      const availableModels = models[member.agent_id] ?? [];
      const marketModel = market.find((item) => item.id === member.agent_id)?.installed?.selectedModelId;
      if (member.model && availableModels.length > 0 && !availableModels.some((model) => model.id === member.model)) {
        setFormError(t("team.validation.model"));
        return;
      }
      if (member.model && availableModels.length === 0 && marketModel && marketModel !== member.model) {
        setFormError(t("team.validation.model"));
        return;
      }
    }
    const members = formMembers.map((member, sort_order) => ({
      ...member,
      sort_order,
      model: member.model || null,
    }));
    setBusy(true);
    setFormError(null);
    try {
      const saved = editor && editor !== "new"
        ? await updateTeam({ team_id: editor.id, name, description: formDescription.trim() || null, members })
        : await createTeam({ name, description: formDescription.trim() || null, members });
      setTeams((current) => editor && editor !== "new"
        ? current.map((team) => team.id === saved.id ? saved : team)
        : [saved, ...current]);
      setSelectedTeamId(saved.id);
      setEditor(null);
    } catch (saveError: unknown) {
      setFormError(t("team.error.save", { message: errorMessage(saveError) }));
    } finally {
      setBusy(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    setBusy(true);
    try {
      await deleteTeam(deleting.id);
      const nextTeams = teams.filter((team) => team.id !== deleting.id);
      setTeams(nextTeams);
      setSelectedTeamId((current) => current === deleting.id ? nextTeams[0]?.id ?? null : current);
      setDeleting(null);
    } catch (deleteError: unknown) {
      setError(t("team.error.delete", { message: errorMessage(deleteError) }));
    } finally {
      setBusy(false);
    }
  };

  const startTeamDraft = async () => {
    if (!selectedTeam || !leaderMessage.trim()) return;
    setWorkflowBusy(true);
    setWorkflowError(null);
    try {
      setRunSnapshot(await draftTeam({ team_id: selectedTeam.id, leader_message: leaderMessage.trim() }));
      setRestoreResult(null);
    } catch (draftError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(draftError) }));
    } finally {
      setWorkflowBusy(false);
    }
  };

  const sendLeaderMessage = async (replay = false) => {
    if (!selectedTeam || (!replay && !leaderChatMessage.trim())) return;
    setLeaderChatBusy(true);
    setWorkflowError(null);
    try {
      const result = await teamLeaderChat({
        team_id: selectedTeam.id,
        message: replay ? "" : leaderChatMessage.trim(),
        replay,
      });
      setLeaderChatResult(result.text);
      if (!replay) setLeaderChatMessage("");
    } catch (chatError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(chatError) }));
    } finally {
      setLeaderChatBusy(false);
    }
  };

  const saveRunReview = async () => {
    if (!runSnapshot) return;
    const tasks = orderedRunTasks.map((task, sort_order) => ({
      task_id: task.id,
      owner_member_id: task.owner_member_id ?? task.recommended_member_id,
      sort_order,
    }));
    if (tasks.some((task) => !runTeammates.some((member) => member.member_id === task.owner_member_id))) {
      setWorkflowError(t("team.workflow.noTasks"));
      return;
    }
    setWorkflowBusy(true);
    setWorkflowError(null);
    try {
      setRunSnapshot(await reviewTeamRun({ run_id: runSnapshot.run.id, revision: runSnapshot.run.revision, tasks }));
    } catch (reviewError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(reviewError) }));
    } finally {
      setWorkflowBusy(false);
    }
  };

  const startTeamExecution = async () => {
    if (!runSnapshot) return;
    setWorkflowBusy(true);
    setWorkflowError(null);
    try {
      setRunSnapshot(await confirmTeamRun({ run_id: runSnapshot.run.id, revision: runSnapshot.run.revision }));
    } catch (confirmError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(confirmError) }));
    } finally {
      setWorkflowBusy(false);
    }
  };

  const restoreTeamExecution = async () => {
    if (!runSnapshot) return;
    setWorkflowBusy(true);
    setWorkflowError(null);
    try {
      const task = await restoreTeamRun(runSnapshot.run.id);
      setRestoreTaskId(task.task_id);
      if (task.state === "Succeeded") {
        const parsed = teamRestoreTaskResultSchema.safeParse(task.result);
        if (parsed.success) {
          setRestoreResult({
            run: runSnapshot,
            leader: null,
            leader_error_code: parsed.data.leader_error_code,
            members: parsed.data.members,
          });
        }
      }
    } catch (restoreError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(restoreError) }));
    } finally {
      setWorkflowBusy(false);
    }
  };

  const cancelTeamExecution = async () => {
    if (!runSnapshot) return;
    setWorkflowBusy(true);
    setWorkflowError(null);
    try {
      await cancelTeamRun(runSnapshot.run.id);
    } catch (cancelError: unknown) {
      setWorkflowError(t("team.workflow.error", { message: errorMessage(cancelError) }));
    } finally {
      setWorkflowBusy(false);
    }
  };

  const moveRunTask = (index: number, direction: -1 | 1) => {
    if (!runSnapshot) return;
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= orderedRunTasks.length) return;
    const nextTasks = [...orderedRunTasks];
    [nextTasks[index], nextTasks[nextIndex]] = [nextTasks[nextIndex], nextTasks[index]];
    setRunSnapshot({
      ...runSnapshot,
      tasks: nextTasks.map((task, sort_order) => ({ ...task, sort_order })),
    });
  };

  return (
    <TeamSessionProvider teamId={selectedTeamId}>
      <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        actions={
          <Button disabled={loading || installedAgents.length === 0} onClick={openCreate} size="sm" type="button">
            <Plus size={16} />
            {t("team.action.create")}
          </Button>
        }
        description={t("team.page.description")}
        eyebrow={t("team.page.eyebrow")}
        icon={<Users size={17} />}
        title={t("team.page.title")}
      />
      {error && <Panel className="border-status-remove/35 bg-status-remove/10 text-body-sm text-status-remove" padding="sm">{error}</Panel>}
      {loading ? (
        <EmptyState className="min-h-0 flex-1" description={t("team.list.loading")} icon={<Users size={22} />} title={t("team.list.title")} />
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(15rem,20rem)_minmax(0,1fr)] gap-4 overflow-hidden">
          <Panel className="min-h-0 overflow-y-auto" padding="sm" variant="muted">
            <div className="flex items-center justify-between px-2 pb-2 text-label-caps uppercase text-on-surface-variant">
              <span>{t("team.list.title")}</span>
              <span>{t("team.list.count", { count: teams.length })}</span>
            </div>
            {teams.length === 0 ? (
              <EmptyState className="min-h-52 border-0 bg-transparent shadow-none" description={t("team.list.emptyDescription")} icon={<Users size={20} />} title={t("team.list.empty")} />
            ) : (
              <div className="grid gap-2">
                {teams.map((team) => {
                  const leader = team.members.find((member) => member.role === "leader");
                  const selected = team.id === selectedTeamId;
                  return (
                    <button
                      aria-pressed={selected}
                      className={`rounded-xl border p-3 text-left transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/45 ${selected ? "border-primary/50 bg-theme-nav-active/75" : "border-theme-card-border/60 bg-theme-card/40 hover:border-primary/35 hover:bg-theme-control-hover/50"}`}
                      key={team.id}
                      onClick={() => setSelectedTeamId(team.id)}
                      type="button"
                    >
                      <div className="flex items-start justify-between gap-2">
                        <span className="truncate text-body-sm font-semibold text-on-surface">{team.name}</span>
                        <span className="shrink-0 text-label-caps text-on-surface-variant">{team.members.length}</span>
                      </div>
                      {leader && <div className="mt-2 flex items-center gap-1.5 text-caption text-primary"><Shield size={13} />{leader.agent_id}</div>}
                    </button>
                  );
                })}
              </div>
            )}
          </Panel>
          {selectedTeam ? (
            <Panel className="min-h-0 overflow-y-auto" padding="lg">
              <div className="flex flex-wrap items-start justify-between gap-4 border-b border-theme-card-border/60 pb-4">
                <div className="min-w-0">
                  <h2 className="truncate text-h3 text-on-surface">{selectedTeam.name}</h2>
                  {selectedTeam.description && <p className="mt-1 text-body-sm text-on-surface-variant">{selectedTeam.description}</p>}
                  <p className="mt-2 text-caption text-on-surface-variant">{t("team.detail.teamId")}: <code>{selectedTeam.id}</code></p>
                </div>
                <div className="flex gap-2">
                  <Button onClick={() => openEdit(selectedTeam)} size="sm" type="button" variant="outline"><Edit2 size={14} />{t("team.action.edit")}</Button>
                  <Button onClick={() => setDeleting(selectedTeam)} size="sm" type="button" variant="destructive"><Trash2 size={14} />{t("team.action.delete")}</Button>
                </div>
              </div>
              <div className="mt-5">
                <div className="mb-3 flex items-center gap-2"><Users className="text-primary" size={16} /><h3 className="text-title-sm font-bold">{t("team.detail.roster")}</h3><span className="text-caption text-on-surface-variant">{t("team.detail.memberCount", { count: selectedTeam.members.length })}</span></div>
                <div className="divide-y divide-theme-card-border/60 overflow-hidden rounded-xl border border-theme-card-border/70">
                  {selectedTeam.members.slice().sort((left, right) => left.sort_order - right.sort_order).map((member) => (
                    <div className="flex flex-wrap items-center justify-between gap-3 bg-theme-card/35 px-4 py-3" key={member.id}>
                      <div className="flex min-w-0 items-center gap-3"><span className="grid size-7 shrink-0 place-items-center rounded-lg bg-theme-control text-caption font-semibold text-on-surface-variant">{member.sort_order + 1}</span><div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate text-body-sm font-semibold">{member.agent_id}</span><span className="text-label-caps text-primary">{member.role === "leader" ? t("team.detail.leader") : t("team.detail.teammate")}</span></div><p className="truncate text-caption text-on-surface-variant">{member.model || t("team.detail.defaultModel")}</p></div></div>
                      <code className="text-caption text-on-surface-variant">{member.execution_context_key.slice(0, 12)}…</code>
                    </div>
                  ))}
                </div>
              </div>
              <div className="mt-6 border-t border-theme-card-border/60 pt-5">
                <div className="mb-3 flex items-center gap-2">
                  <MessageSquare className="text-primary" size={16} />
                  <h3 className="text-title-sm font-bold">{t("team.leader.title")}</h3>
                </div>
                <div className="grid gap-2 rounded-xl border border-theme-card-border/70 bg-theme-card/30 p-3">
                  <label className="grid gap-1.5 text-body-sm font-semibold">
                    {t("team.leader.message")}
                    <Input
                      disabled={leaderChatBusy}
                      onChange={(event) => setLeaderChatMessage(event.target.value)}
                      placeholder={t("team.leader.placeholder")}
                      value={leaderChatMessage}
                    />
                  </label>
                  <div className="flex flex-wrap gap-2">
                    <Button disabled={leaderChatBusy || !leaderChatMessage.trim()} onClick={() => void sendLeaderMessage()} size="sm" type="button">
                      <MessageSquare size={14} />
                      {leaderChatBusy ? t("team.leader.sending") : t("team.leader.send")}
                    </Button>
                    <Button disabled={leaderChatBusy} onClick={() => void sendLeaderMessage(true)} size="sm" type="button" variant="outline">
                      <RotateCcw size={14} />
                      {t("team.leader.replay")}
                    </Button>
                  </div>
                  {leaderChatResult ? <p className="whitespace-pre-wrap rounded-lg border border-theme-card-border/60 bg-theme-control/25 p-3 text-body-sm text-on-surface">{leaderChatResult}</p> : null}
                </div>
              </div>
              <div className="mt-6 border-t border-theme-card-border/60 pt-5">
                <div className="mb-3 flex items-center gap-2">
                  <CheckCircle2 className="text-primary" size={16} />
                  <h3 className="text-title-sm font-bold">{t("team.workflow.title")}</h3>
                </div>
                <div className="grid gap-2 rounded-xl border border-theme-card-border/70 bg-theme-card/30 p-3">
                  <label className="grid gap-1.5 text-body-sm font-semibold">
                    {t("team.workflow.message")}
                    <Input
                      disabled={workflowBusy}
                      onChange={(event) => setLeaderMessage(event.target.value)}
                      placeholder={t("team.workflow.messagePlaceholder")}
                      value={leaderMessage}
                    />
                  </label>
                  <div className="flex flex-wrap items-center gap-2">
                    <Button disabled={workflowBusy || !leaderMessage.trim()} onClick={() => void startTeamDraft()} size="sm" type="button">
                      <Play size={14} />
                      {t("team.workflow.draft")}
                    </Button>
                    {runSnapshot && (
                      <>
                        <span className="text-caption text-on-surface-variant">
                          {t("team.workflow.state", { state: runSnapshot.run.state })} · {t("team.workflow.revision", { revision: runSnapshot.run.revision })}
                        </span>
                        {runSnapshot.tasks.length > 0 && (
                          <span className="text-caption text-on-surface-variant">
                            {t("team.workflow.progress", { completed: completedRunTasks, total: runSnapshot.tasks.length })}
                          </span>
                        )}
                        <Button disabled={workflowBusy || runSnapshot.run.state !== "awaiting_review"} onClick={() => void saveRunReview()} size="sm" type="button" variant="outline">
                          {t("team.workflow.review")}
                        </Button>
                        <Button disabled={workflowBusy || runSnapshot.run.state !== "awaiting_review"} onClick={() => void startTeamExecution()} size="sm" type="button">
                          {t("team.workflow.confirm")}
                        </Button>
                        <Button disabled={workflowBusy || Boolean(restoreTaskId) || runSnapshot.run.state === "drafting"} onClick={() => void restoreTeamExecution()} size="sm" type="button" variant="ghost">
                          <RotateCcw size={14} />
                          {t("team.workflow.restore")}
                        </Button>
                        <Button disabled={workflowBusy || runSnapshot.run.state === "terminal"} onClick={() => void cancelTeamExecution()} size="sm" type="button" variant="outline">
                          {t("team.workflow.cancel")}
                        </Button>
                        {restoreTask && restoreTask.state !== "Succeeded" && restoreTask.state !== "Failed" && restoreTask.state !== "Canceled" && (
                          <span className="text-caption text-on-surface-variant">
                            {t("team.workflow.restoreProgress", { state: restoreTask.state })}
                          </span>
                        )}
                      </>
                    )}
                  </div>
                  {runSnapshot && runSnapshot.tasks.length === 0 && <p className="text-caption text-on-surface-variant">{t("team.workflow.noTasks")}</p>}
                  {orderedRunTasks.length > 0 && (
                    <div className="grid gap-2">
                      {orderedRunTasks.map((task, index) => (
                        <div className="grid gap-2 rounded-lg border border-theme-card-border/60 bg-theme-control/35 p-3" key={task.id}>
                          <div className="flex items-start justify-between gap-3">
                            <div className="min-w-0">
                              <p className="truncate text-body-sm font-semibold">{task.title}</p>
                              <p className="mt-1 text-caption text-on-surface-variant">{task.description}</p>
                            </div>
                            <span className="shrink-0 text-label-caps text-primary">{task.state}</span>
                          </div>
                          <div className="flex flex-wrap items-center gap-2">
                            <label className="flex items-center gap-2 text-caption text-on-surface-variant">
                              {t("team.workflow.owner")}
                              <select
                                className={selectClassName}
                                disabled={workflowBusy || runState !== "awaiting_review"}
                                onChange={(event) => setRunSnapshot((current) => current ? {
                                  ...current,
                                  tasks: current.tasks.map((currentTask) => currentTask.id === task.id ? { ...currentTask, owner_member_id: event.target.value } : currentTask),
                                } : current)}
                                value={task.owner_member_id ?? ""}
                              >
                                <option value="">{t("team.workflow.unassigned")}</option>
                                {runTeammates.map((member) => <option key={member.member_id} value={member.member_id}>{member.member_id} · {member.agent_id}</option>)}
                              </select>
                            </label>
                            <Button aria-label={t("team.action.moveUp")} disabled={workflowBusy || runState !== "awaiting_review" || index === 0} onClick={() => moveRunTask(index, -1)} size="icon-sm" type="button" variant="ghost"><ArrowUp size={14} /></Button>
                            <Button aria-label={t("team.action.moveDown")} disabled={workflowBusy || runState !== "awaiting_review" || index === orderedRunTasks.length - 1} onClick={() => moveRunTask(index, 1)} size="icon-sm" type="button" variant="ghost"><ArrowDown size={14} /></Button>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                  {restoreResult && (
                    <div className="grid gap-1 rounded-lg border border-theme-card-border/60 bg-theme-control/25 p-3 text-caption">
                      <span className="font-semibold text-on-surface">{t("team.workflow.restoreStatus")}</span>
                      {restoreResult.members.map((member) => <span className="text-on-surface-variant" key={member.member_id}>{member.member_id}: {member.state === "ready" ? t("team.workflow.ready") : t("team.workflow.unavailable")}</span>)}
                    </div>
                  )}
                  {workflowError && <p className="rounded-lg border border-status-remove/35 bg-status-remove/10 p-2 text-caption text-status-remove">{workflowError}</p>}
                </div>
              </div>
            </Panel>
          ) : (
            <EmptyState className="min-h-0" description={t("team.list.emptyDescription")} icon={<Users size={22} />} title={t("team.detail.select")} />
          )}
        </div>
      )}
      {editor && <DialogFrame
        busy={busy}
        contentClassName="grid gap-4"
        footer={<><Button disabled={busy} onClick={() => setEditor(null)} type="button" variant="outline">{t("team.action.cancel")}</Button><Button disabled={busy} form="team-editor-form" type="submit">{busy ? t("common.saving") : t("team.action.save")}</Button></>}
        icon={<Users size={18} />}
        onClose={() => setEditor(null)}
        title={editor === "new" ? t("team.dialog.createTitle") : t("team.dialog.editTitle")}
      >
        <form className="grid gap-4" id="team-editor-form" onSubmit={(event) => void save(event)}>
          <label className="grid gap-1.5 text-body-sm font-semibold">{t("team.dialog.name")}<Input autoFocus disabled={busy} onChange={(event) => setFormName(event.target.value)} placeholder={t("team.dialog.namePlaceholder")} value={formName} /></label>
          <label className="grid gap-1.5 text-body-sm font-semibold">{t("team.dialog.description")}<Input disabled={busy} onChange={(event) => setFormDescription(event.target.value)} placeholder={t("team.dialog.descriptionPlaceholder")} value={formDescription} /></label>
          <div className="grid gap-2"><div className="flex items-center justify-between gap-3"><span className="text-body-sm font-semibold">{t("team.dialog.members")}</span><Button disabled={busy} onClick={() => setFormMembers((current) => [...current, { role: "teammate", agent_id: selectableAgents()[0]?.id ?? "", model: null }])} size="sm" type="button" variant="link"><Plus size={14} />{t("team.action.addMember")}</Button></div><div className="grid max-h-72 gap-2 overflow-y-auto pr-1">{formMembers.map((member, index) => { const memberModels = models[member.agent_id] ?? []; const roleAgents = selectableAgents(); return <div className="grid gap-2 rounded-xl border border-theme-card-border/65 bg-theme-card/40 p-3" key={member.id ?? `new-${index}`}><div className="grid grid-cols-[auto_minmax(0,1fr)_minmax(0,1fr)_auto] items-end gap-2"><label className="grid gap-1 text-caption text-on-surface-variant">{t("team.dialog.agent")}<select aria-label={`${t("team.dialog.agent")} ${index + 1}`} className={selectClassName} disabled={busy || roleAgents.length === 0} onChange={(event) => updateMember(index, { agent_id: event.target.value, model: null })} value={roleAgents.some((agent) => agent.id === member.agent_id) ? member.agent_id : ""}><option value="">{roleAgents.length ? t("team.dialog.selectAgent") : t("team.dialog.noAgents")}</option>{roleAgents.map((agent) => <option key={agent.id} value={agent.id}>{agent.display_name} ({agent.id})</option>)}</select></label><label className="grid gap-1 text-caption text-on-surface-variant">{t("team.dialog.model")}<select aria-label={`${t("team.dialog.model")} ${index + 1}`} className={selectClassName} disabled={busy || !member.agent_id} onChange={(event) => updateMember(index, { model: event.target.value || null })} value={member.model ?? ""}><option value="">{memberModels.length ? t("team.dialog.selectModel") : t("team.dialog.noModels")}</option>{memberModels.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}</select></label><label className="grid gap-1 text-caption text-on-surface-variant">{t("team.dialog.members")}<select aria-label={`Role ${index + 1}`} className={selectClassName} disabled={busy} onChange={(event) => updateMember(index, { role: event.target.value as TeamRole, agent_id: selectableAgents()[0]?.id ?? "", model: null })} value={member.role}><option value="leader">{t("team.detail.leader")}</option><option value="teammate">{t("team.detail.teammate")}</option></select></label><div className="flex gap-1"><Button aria-label={t("team.action.moveUp")} disabled={busy || index === 0} onClick={() => moveMember(index, -1)} size="icon-sm" type="button" variant="ghost"><ArrowUp size={14} /></Button><Button aria-label={t("team.action.moveDown")} disabled={busy || index === formMembers.length - 1} onClick={() => moveMember(index, 1)} size="icon-sm" type="button" variant="ghost"><ArrowDown size={14} /></Button><Button aria-label={t("team.action.removeMember")} disabled={busy || formMembers.length <= 2} onClick={() => setFormMembers((current) => current.filter((_, memberIndex) => memberIndex !== index))} size="icon-sm" type="button" variant="ghost"><Trash2 size={14} /></Button></div></div></div>; })}</div></div>
          {formError && <p className="rounded-xl border border-status-remove/35 bg-status-remove/10 p-3 text-body-sm text-status-remove">{formError}</p>}
        </form>
      </DialogFrame>}
      <ConfirmDialog busy={busy} confirmLabel={t("team.action.delete")} message={deleting ? t("team.confirm.deleteMessage", { name: deleting.name }) : ""} onClose={() => setDeleting(null)} onConfirm={() => void confirmDelete()} open={Boolean(deleting)} title={t("team.confirm.deleteTitle")} tone="danger" />
      </section>
    </TeamSessionProvider>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
