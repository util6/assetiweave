import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import {
  cancelTeamMemberTurn,
  getTeamMemberStreamSnapshot,
  listTeamMemberTasks,
  startTeamMemberReplay,
  startTeamMemberTurn,
  subscribeTeamMemberSessions,
} from "../../services/teamWorkflow";
import type {
  TeamMemberSessionProjection,
  TeamMemberStreamSnapshot,
  TeamMemberTaskSnapshot,
  TeamSessionStoreState,
} from "../../types/team";
import {
  applyTeamMemberStreamSnapshot,
  createTeamSessionStoreState,
  markTeamMemberSessionSeen,
  mergeTeamSessionState,
  selectTeamMemberSession,
  selectTeamMemberSessions,
  teamSessionStateFromSnapshots,
} from "./TeamSessionStore";
import {
  useBackgroundTaskRuntime,
  type BackgroundTaskRuntimeAdapter,
} from "./BackgroundTaskRuntime";

interface TeamSessionContextValue {
  scopeTeamId: string | null;
  state: TeamSessionStoreState;
  refresh: () => Promise<void>;
  getMember: (teamId: string, memberId: string) => TeamMemberSessionProjection | null;
  markSeen: (teamId: string, memberId: string) => void;
  startTurn: (teamId: string, memberId: string, message: string) => Promise<TeamMemberStreamSnapshot>;
  startReplay: (teamId: string, memberId: string) => Promise<TeamMemberStreamSnapshot>;
  cancelTurn: (teamId: string, memberId: string, executionId: string) => Promise<TeamMemberStreamSnapshot>;
}

export interface TeamSessionView {
  state: TeamSessionStoreState;
  refresh: () => Promise<void>;
  teamId: string | null;
  members: TeamMemberSessionProjection[];
  getMember: (memberId: string) => TeamMemberSessionProjection | null;
  markSeen: (memberId: string) => void;
  startTurn: (memberId: string, message: string) => Promise<TeamMemberStreamSnapshot>;
  startReplay: (memberId: string) => Promise<TeamMemberStreamSnapshot>;
  cancelTurn: (memberId: string, executionId: string) => Promise<TeamMemberStreamSnapshot>;
}

const TeamSessionContext = createContext<TeamSessionContextValue | null>(null);

export function TeamSessionProvider({
  children,
  teamId = null,
}: {
  children: ReactNode;
  teamId?: string | null;
}) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<TeamSessionStoreState, TeamMemberStreamSnapshot>>(
    () => ({
      initialState: createTeamSessionStoreState(teamId),
      isRunning: isTeamSessionRunning,
      merge: (current, incoming) => (
        isTeamSessionStoreState(incoming)
          ? mergeTeamSessionState(current, incoming)
          : applyTeamMemberStreamSnapshot(current, incoming)
      ),
      refresh: () => loadTeamSessionState(teamId),
      subscribe: (listener) => subscribeTeamMemberSessions((snapshot) => {
        if (teamId === null || snapshot.team_id === teamId) listener(snapshot);
      }),
      pollIntervalMs: 1000,
      reconnectDelayMs: 1000,
    }),
    [teamId],
  );
  const { merge, refresh, state, update } = useBackgroundTaskRuntime(adapter);

  const startTurn = useCallback(async (currentTeamId: string, memberId: string, message: string) => {
    const snapshot = await startTeamMemberTurn({
      team_id: currentTeamId,
      member_id: memberId,
      message,
      replay: false,
    });
    merge(snapshot);
    return snapshot;
  }, [merge]);

  const startReplay = useCallback(async (currentTeamId: string, memberId: string) => {
    const snapshot = await startTeamMemberReplay(currentTeamId, memberId);
    merge(snapshot);
    return snapshot;
  }, [merge]);

  const cancelTurn = useCallback(async (currentTeamId: string, memberId: string, executionId: string) => {
    const snapshot = await cancelTeamMemberTurn(currentTeamId, memberId, executionId);
    merge(snapshot);
    return snapshot;
  }, [merge]);

  const markSeen = useCallback((currentTeamId: string, memberId: string) => {
    update((current) => markTeamMemberSessionSeen(current, currentTeamId, memberId));
  }, [update]);

  const value = useMemo<TeamSessionContextValue>(() => ({
    scopeTeamId: teamId,
    state,
    refresh: async () => {
      await refresh();
    },
    getMember: (currentTeamId, memberId) => selectTeamMemberSession(state, currentTeamId, memberId),
    markSeen,
    startTurn,
    startReplay,
    cancelTurn,
  }), [cancelTurn, markSeen, refresh, startReplay, startTurn, state]);

  return <TeamSessionContext.Provider value={value}>{children}</TeamSessionContext.Provider>;
}

export function useTeamSession(teamId?: string | null): TeamSessionView {
  const context = useContext(TeamSessionContext);
  if (!context) throw new Error("useTeamSession must be used inside TeamSessionProvider");
  const selectedTeamId = teamId === undefined ? context.scopeTeamId : teamId;
  return useMemo(
    () => buildTeamSessionView(context, selectedTeamId),
    [context, selectedTeamId],
  );
}

export function useOptionalTeamSession(teamId?: string | null): TeamSessionView | null {
  const context = useContext(TeamSessionContext);
  const selectedTeamId = teamId === undefined ? context?.scopeTeamId ?? null : teamId;
  return useMemo(
    () => context ? buildTeamSessionView(context, selectedTeamId) : null,
    [context, selectedTeamId],
  );
}

function buildTeamSessionView(
  context: TeamSessionContextValue,
  selectedTeamId: string | null,
): TeamSessionView {
  return {
    state: context.state,
    refresh: context.refresh,
    teamId: selectedTeamId,
    members: selectedTeamId ? selectTeamMemberSessions(context.state, selectedTeamId) : [],
    getMember: (memberId: string) => selectedTeamId
      ? context.getMember(selectedTeamId, memberId)
      : null,
    markSeen: (memberId: string) => {
      if (selectedTeamId) context.markSeen(selectedTeamId, memberId);
    },
    startTurn: (memberId: string, message: string) => requireTeamId(
      selectedTeamId,
      (currentTeamId) => context.startTurn(currentTeamId, memberId, message),
    ),
    startReplay: (memberId: string) => requireTeamId(
      selectedTeamId,
      (currentTeamId) => context.startReplay(currentTeamId, memberId),
    ),
    cancelTurn: (memberId: string, executionId: string) => requireTeamId(
      selectedTeamId,
      (currentTeamId) => context.cancelTurn(currentTeamId, memberId, executionId),
    ),
  };
}

async function loadTeamSessionState(teamId: string | null): Promise<TeamSessionStoreState> {
  const tasks = (await listTeamMemberTasks())
    .filter((task) => teamId === null || task.detail.team_id === teamId);
  const snapshots = await Promise.all(tasks.map(async (task) => {
    try {
      return await getTeamMemberStreamSnapshot(
        task.detail.team_id,
        task.detail.member_id,
        task.detail.execution_id,
      ) ?? snapshotFromTask(task);
    } catch {
      return snapshotFromTask(task);
    }
  }));
  return teamSessionStateFromSnapshots(teamId, snapshots);
}

function snapshotFromTask(task: TeamMemberTaskSnapshot): TeamMemberStreamSnapshot {
  return {
    team_id: task.detail.team_id,
    member_id: task.detail.member_id,
    execution_id: task.detail.execution_id,
    sequence: 0,
    replay: task.detail.replay,
    task,
    stream: { revision: 0, event_count: 0, items: [] },
  };
}

function isTeamSessionStoreState(
  incoming: TeamSessionStoreState | TeamMemberStreamSnapshot,
): incoming is TeamSessionStoreState {
  return "members" in incoming;
}

function isTeamSessionRunning(state: TeamSessionStoreState): boolean {
  return Object.values(state.members).some((member) => Object.values(member.executions).some(
    (execution) => ["Pending", "Running", "Cancelling"].includes(execution.task.state),
  ));
}

function requireTeamId<T>(
  teamId: string | null,
  callback: (teamId: string) => Promise<T>,
): Promise<T> {
  if (!teamId) return Promise.reject(new Error("A Team must be selected for member Session actions."));
  return callback(teamId);
}
