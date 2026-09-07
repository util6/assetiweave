import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
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
  markTeamMemberSessionUnavailable,
  markTeamMemberSessionSeen,
  mergeTeamSessionState,
  selectTeamMemberSession,
  selectTeamMemberSessions,
  teamSessionStateFromSnapshots,
} from "./TeamSessionStore";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

const MAX_REPLAY_CONCURRENCY = 2;

interface ReplaySchedulerState {
  teamId: string | null;
  scheduled: Set<string>;
  running: Set<string>;
}

interface TeamSessionContextValue {
  scopeTeamId: string | null;
  state: TeamSessionStoreState;
  refresh: () => Promise<void>;
  getMember: (
    teamId: string,
    memberId: string,
  ) => TeamMemberSessionProjection | null;
  markSeen: (teamId: string, memberId: string) => void;
  startTurn: (
    teamId: string,
    memberId: string,
    message: string,
  ) => Promise<TeamMemberStreamSnapshot>;
  startReplay: (
    teamId: string,
    memberId: string,
  ) => Promise<TeamMemberStreamSnapshot>;
  cancelTurn: (
    teamId: string,
    memberId: string,
    executionId: string,
  ) => Promise<TeamMemberStreamSnapshot>;
}

export interface TeamSessionView {
  state: TeamSessionStoreState;
  refresh: () => Promise<void>;
  teamId: string | null;
  members: TeamMemberSessionProjection[];
  getMember: (memberId: string) => TeamMemberSessionProjection | null;
  markSeen: (memberId: string) => void;
  startTurn: (
    memberId: string,
    message: string,
  ) => Promise<TeamMemberStreamSnapshot>;
  startReplay: (memberId: string) => Promise<TeamMemberStreamSnapshot>;
  cancelTurn: (
    memberId: string,
    executionId: string,
  ) => Promise<TeamMemberStreamSnapshot>;
}

const TeamSessionContext = createContext<TeamSessionContextValue | null>(null);

export function teamSessionQueryKey(scope: QueryScope, teamId: string | null) {
  return [
    ...taskKeys.resource(scope, "team-session"),
    teamId ?? "all",
  ] as const;
}

const BACKEND_STATE_FLAG = Symbol("BACKEND_STATE_FLAG");

interface BackendTeamSessionStoreState extends TeamSessionStoreState {
  [BACKEND_STATE_FLAG]?: boolean;
}

export function teamSessionQueryOptions(
  scope: QueryScope,
  teamId: string | null,
) {
  return queryOptions<TeamSessionStoreState>({
    queryKey: teamSessionQueryKey(scope, teamId),
    queryFn: async () => {
      const state = (await loadTeamSessionState(
        teamId,
      )) as BackendTeamSessionStoreState;
      state[BACKEND_STATE_FLAG] = true;
      return state;
    },
    structuralSharing: (oldData, newData) => {
      if (!oldData) return newData;
      if (!newData) return oldData;
      if ((newData as BackendTeamSessionStoreState)[BACKEND_STATE_FLAG]) {
        return mergeTeamSessionState(
          oldData as TeamSessionStoreState,
          newData as TeamSessionStoreState,
        );
      }
      return newData;
    },
    staleTime: 1000,
  });
}

export function TeamSessionProvider({
  activeMemberId = null,
  autoRestore = false,
  children,
  memberIds = [],
  teamId = null,
}: {
  activeMemberId?: string | null;
  autoRestore?: boolean;
  children: ReactNode;
  memberIds?: string[];
  teamId?: string | null;
}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = useMemo(
    () => teamSessionQueryKey(activeScope, teamId),
    [activeScope.tenantId, activeScope.epoch, teamId],
  );
  const queryClient = useQueryClient();

  const query = useQuery({
    ...teamSessionQueryOptions(activeScope, teamId),
    enabled: true,
    refetchInterval: (q) =>
      q.state.data && isTeamSessionRunning(q.state.data) ? 1000 : 10000,
    refetchIntervalInBackground: true,
  });

  const state = query.data ?? createTeamSessionStoreState(teamId);

  const startTurn = useCallback(
    async (currentTeamId: string, memberId: string, message: string) => {
      const snapshot = await startTeamMemberTurn({
        team_id: currentTeamId,
        member_id: memberId,
        message,
        replay: false,
      });
      queryClient.setQueryData<TeamSessionStoreState>(queryKey, (current) =>
        applyTeamMemberStreamSnapshot(
          current ?? createTeamSessionStoreState(teamId),
          snapshot,
        ),
      );
      return snapshot;
    },
    [queryClient, queryKey, teamId],
  );

  const startReplay = useCallback(
    async (currentTeamId: string, memberId: string) => {
      const snapshot = await startTeamMemberReplay(currentTeamId, memberId);
      if (!snapshot)
        throw new Error("Team member replay did not return a snapshot.");
      queryClient.setQueryData<TeamSessionStoreState>(queryKey, (current) =>
        applyTeamMemberStreamSnapshot(
          current ?? createTeamSessionStoreState(teamId),
          snapshot,
        ),
      );
      return snapshot;
    },
    [queryClient, queryKey, teamId],
  );

  const cancelTurn = useCallback(
    async (currentTeamId: string, memberId: string, executionId: string) => {
      const snapshot = await cancelTeamMemberTurn(
        currentTeamId,
        memberId,
        executionId,
      );
      queryClient.setQueryData<TeamSessionStoreState>(queryKey, (current) =>
        applyTeamMemberStreamSnapshot(
          current ?? createTeamSessionStoreState(teamId),
          snapshot,
        ),
      );
      return snapshot;
    },
    [queryClient, queryKey, teamId],
  );

  const markSeen = useCallback(
    (currentTeamId: string, memberId: string) => {
      queryClient.setQueryData<TeamSessionStoreState>(queryKey, (current) =>
        markTeamMemberSessionSeen(
          current ?? createTeamSessionStoreState(teamId),
          currentTeamId,
          memberId,
        ),
      );
    },
    [queryClient, queryKey, teamId],
  );

  const replaySchedulerRef = useRef<ReplaySchedulerState>({
    teamId: null,
    scheduled: new Set(),
    running: new Set(),
  });

  useEffect(() => {
    const scheduler = replaySchedulerRef.current;
    if (scheduler.teamId !== teamId) {
      scheduler.teamId = teamId;
      scheduler.scheduled.clear();
      scheduler.running.clear();
    }
    if (!autoRestore || !teamId) return;

    for (const memberId of scheduler.running) {
      const projection = state.members[memberId];
      if (projection && projection.restore_state !== "restoring") {
        scheduler.running.delete(memberId);
      }
    }

    const orderedMemberIds = [
      ...new Set(
        [activeMemberId, ...memberIds].filter((memberId): memberId is string =>
          Boolean(memberId?.trim()),
        ),
      ),
    ];
    for (const memberId of orderedMemberIds) {
      if (scheduler.running.size >= MAX_REPLAY_CONCURRENCY) break;
      if (scheduler.scheduled.has(memberId)) continue;
      const projection = state.members[memberId];
      if (
        projection &&
        ["ready", "partial", "unavailable"].includes(projection.restore_state)
      ) {
        scheduler.scheduled.add(memberId);
        continue;
      }
      scheduler.scheduled.add(memberId);
      scheduler.running.add(memberId);
      void startReplay(teamId, memberId)
        .then((snapshot) => {
          if (scheduler.teamId === teamId && !isActiveTask(snapshot.task)) {
            scheduler.running.delete(memberId);
          }
        })
        .catch(() => {
          if (scheduler.teamId !== teamId) return;
          scheduler.running.delete(memberId);
          queryClient.setQueryData<TeamSessionStoreState>(queryKey, (current) =>
            markTeamMemberSessionUnavailable(
              current ?? createTeamSessionStoreState(teamId),
              teamId,
              memberId,
              "team_member_restore_unavailable",
            ),
          );
        });
    }
  }, [
    activeMemberId,
    autoRestore,
    memberIds,
    queryClient,
    queryKey,
    startReplay,
    state,
    teamId,
  ]);

  const value = useMemo<TeamSessionContextValue>(
    () => ({
      scopeTeamId: teamId,
      state,
      refresh: async () => {
        await queryClient.refetchQueries({ exact: true, queryKey });
      },
      getMember: (currentTeamId, memberId) =>
        selectTeamMemberSession(state, currentTeamId, memberId),
      markSeen,
      startTurn,
      startReplay,
      cancelTurn,
    }),
    [
      cancelTurn,
      markSeen,
      queryClient,
      queryKey,
      startReplay,
      startTurn,
      state,
      teamId,
    ],
  );

  return (
    <TeamSessionContext.Provider value={value}>
      <TaskEventBridge<TeamSessionStoreState, TeamMemberStreamSnapshot>
        key={queryKey.join(":")}
        merge={(current, snapshot) =>
          applyTeamMemberStreamSnapshot(
            current ?? createTeamSessionStoreState(teamId),
            snapshot,
          )
        }
        queryKey={queryKey}
        subscribe={(listener) =>
          subscribeTeamMemberSessions((snapshot) => {
            if (teamId === null || snapshot.team_id === teamId) {
              listener(snapshot);
            }
          })
        }
      />
      {children}
    </TeamSessionContext.Provider>
  );
}

export function useTeamSession(teamId?: string | null): TeamSessionView {
  const context = useContext(TeamSessionContext);
  if (!context)
    throw new Error("useTeamSession must be used inside TeamSessionProvider");
  const selectedTeamId = teamId === undefined ? context.scopeTeamId : teamId;
  return useMemo(
    () => buildTeamSessionView(context, selectedTeamId),
    [context, selectedTeamId],
  );
}

export function useOptionalTeamSession(
  teamId?: string | null,
): TeamSessionView | null {
  const context = useContext(TeamSessionContext);
  const selectedTeamId =
    teamId === undefined ? (context?.scopeTeamId ?? null) : teamId;
  return useMemo(
    () => (context ? buildTeamSessionView(context, selectedTeamId) : null),
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
    members: selectedTeamId
      ? selectTeamMemberSessions(context.state, selectedTeamId)
      : [],
    getMember: (memberId: string) =>
      selectedTeamId ? context.getMember(selectedTeamId, memberId) : null,
    markSeen: (memberId: string) => {
      if (selectedTeamId) context.markSeen(selectedTeamId, memberId);
    },
    startTurn: (memberId: string, message: string) =>
      requireTeamId(selectedTeamId, (currentTeamId) =>
        context.startTurn(currentTeamId, memberId, message),
      ),
    startReplay: (memberId: string) =>
      requireTeamId(selectedTeamId, (currentTeamId) =>
        context.startReplay(currentTeamId, memberId),
      ),
    cancelTurn: (memberId: string, executionId: string) =>
      requireTeamId(selectedTeamId, (currentTeamId) =>
        context.cancelTurn(currentTeamId, memberId, executionId),
      ),
  };
}

async function loadTeamSessionState(
  teamId: string | null,
): Promise<TeamSessionStoreState> {
  const tasks = (await listTeamMemberTasks()).filter(
    (task) => teamId === null || task.detail.team_id === teamId,
  );
  const snapshots = await Promise.all(
    tasks.map(async (task) => {
      try {
        return (
          (await getTeamMemberStreamSnapshot(
            task.detail.team_id,
            task.detail.member_id,
            task.detail.execution_id,
          )) ?? snapshotFromTask(task)
        );
      } catch {
        return snapshotFromTask(task);
      }
    }),
  );
  return teamSessionStateFromSnapshots(teamId, snapshots);
}

function snapshotFromTask(
  task: TeamMemberTaskSnapshot,
): TeamMemberStreamSnapshot {
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

function isTeamSessionRunning(state: TeamSessionStoreState): boolean {
  return Object.values(state.members).some((member) =>
    Object.values(member.executions).some((execution) =>
      ["Pending", "Running", "Cancelling"].includes(execution.task.state),
    ),
  );
}

function isActiveTask(task: TeamMemberTaskSnapshot): boolean {
  return (
    task.state === "Pending" ||
    task.state === "Running" ||
    task.state === "Cancelling"
  );
}

function requireTeamId<T>(
  teamId: string | null,
  callback: (teamId: string) => Promise<T>,
): Promise<T> {
  if (!teamId)
    return Promise.reject(
      new Error("A Team must be selected for member Session actions."),
    );
  return callback(teamId);
}
