import type {
  SessionItemSnapshot,
  SessionSnapshot,
  TeamMemberExecutionProjection,
  TeamMemberRestoreState,
  TeamMemberSessionProjection,
  TeamMemberStreamSnapshot,
  TeamMemberTaskSnapshot,
  TeamSessionStoreState,
} from "../../types/team";

export const MAX_TEAM_SESSION_ITEMS = 256;
export const MAX_TEAM_SESSION_EXECUTIONS = 32;

const EMPTY_SESSION: SessionSnapshot = {
  revision: 0,
  event_count: 0,
  items: [],
};

export function createTeamSessionStoreState(teamId: string | null = null): TeamSessionStoreState {
  return { team_id: teamId, members: {} };
}

export function applyTeamMemberStreamSnapshot(
  current: TeamSessionStoreState,
  snapshot: TeamMemberStreamSnapshot,
): TeamSessionStoreState {
  if (current.team_id !== null && current.team_id !== snapshot.team_id) {
    return current;
  }

  const previous = current.members[snapshot.member_id];
  const execution = mergeExecution(previous?.executions[snapshot.execution_id], snapshot);
  const executions = retainExecutions({
    ...(previous?.executions ?? {}),
    [snapshot.execution_id]: execution,
  });
  const member = buildMemberProjection(snapshot.team_id, snapshot.member_id, executions);
  const receivedNewLiveExecution = !snapshot.replay
    && (!previous?.executions[snapshot.execution_id]
      || execution.sequence > (previous.executions[snapshot.execution_id]?.sequence ?? 0));
  member.unread = (previous?.unread ?? false) || receivedNewLiveExecution;

  return {
    team_id: current.team_id,
    members: { ...current.members, [snapshot.member_id]: member },
  };
}

export function markTeamMemberSessionSeen(
  current: TeamSessionStoreState,
  teamId: string,
  memberId: string,
): TeamSessionStoreState {
  const member = current.members[memberId];
  if (!member || member.team_id !== teamId || !member.unread) return current;
  return {
    ...current,
    members: {
      ...current.members,
      [memberId]: { ...member, unread: false },
    },
  };
}

export function mergeTeamSessionState(
  current: TeamSessionStoreState,
  incoming: TeamSessionStoreState,
): TeamSessionStoreState {
  if (
    current.team_id !== null
    && incoming.team_id !== null
    && current.team_id !== incoming.team_id
  ) {
    return current;
  }

  let next = current;
  for (const member of Object.values(incoming.members)) {
    for (const execution of Object.values(member.executions)) {
      next = applyTeamMemberStreamSnapshot(next, {
        team_id: execution.team_id,
        member_id: execution.member_id,
        execution_id: execution.execution_id,
        sequence: execution.sequence,
        replay: execution.replay,
        task: execution.task,
        stream: execution.stream,
      });
    }
  }
  return next;
}

export function teamSessionStateFromSnapshots(
  teamId: string | null,
  snapshots: TeamMemberStreamSnapshot[],
): TeamSessionStoreState {
  return snapshots.reduce(
    applyTeamMemberStreamSnapshot,
    createTeamSessionStoreState(teamId),
  );
}

export function selectTeamMemberSession(
  state: TeamSessionStoreState,
  teamId: string,
  memberId: string,
): TeamMemberSessionProjection | null {
  const member = state.members[memberId];
  return member?.team_id === teamId ? member : null;
}

export function selectTeamMemberSessions(
  state: TeamSessionStoreState,
  teamId: string,
): TeamMemberSessionProjection[] {
  return Object.values(state.members)
    .filter((member) => member.team_id === teamId)
    .sort((left, right) => left.member_id.localeCompare(right.member_id));
}

function mergeExecution(
  current: TeamMemberExecutionProjection | undefined,
  incoming: TeamMemberStreamSnapshot,
): TeamMemberExecutionProjection {
  return {
    team_id: incoming.team_id,
    member_id: incoming.member_id,
    execution_id: incoming.execution_id,
    sequence: Math.max(current?.sequence ?? 0, incoming.sequence, incoming.stream.revision),
    replay: incoming.replay,
    task: mergeTaskSnapshot(current?.task, incoming.task),
    stream: mergeSessionSnapshot(current?.stream, incoming.stream),
  };
}

function mergeSessionSnapshot(
  current: SessionSnapshot | undefined,
  incoming: SessionSnapshot,
): SessionSnapshot {
  if (!current) return boundedSessionSnapshot(incoming);
  if (incoming.revision < current.revision) return current;

  const isEmptyReset = incoming.revision > current.revision
    && incoming.event_count === 0
    && incoming.items.length === 0;
  const isAuthoritativeSnapshot = incoming.revision > current.revision
    && (incoming.event_count >= current.event_count || isEmptyReset);
  const items = isAuthoritativeSnapshot
    ? incoming.items
    : mergeSessionItems(current.items, incoming.items);

  return boundedSessionSnapshot({
    revision: Math.max(current.revision, incoming.revision),
    event_count: Math.max(current.event_count, incoming.event_count),
    items,
  });
}

function mergeTaskSnapshot(
  current: TeamMemberTaskSnapshot | undefined,
  incoming: TeamMemberTaskSnapshot,
): TeamMemberTaskSnapshot {
  if (!current || shouldReplaceTask(current, incoming)) return incoming;
  return current;
}

function shouldReplaceTask(
  current: TeamMemberTaskSnapshot,
  incoming: TeamMemberTaskSnapshot,
): boolean {
  const stateOrder = (state: TeamMemberTaskSnapshot["state"]) => ({
    Pending: 0,
    Running: 1,
    Cancelling: 2,
    Succeeded: 3,
    Failed: 3,
    Canceled: 3,
  }[state]);
  const currentOrder = stateOrder(current.state);
  const incomingOrder = stateOrder(incoming.state);
  if (incomingOrder !== currentOrder) return incomingOrder > currentOrder;

  const currentFinishedAt = current.finished_at ?? "";
  const incomingFinishedAt = incoming.finished_at ?? "";
  if (incomingFinishedAt !== currentFinishedAt) return incomingFinishedAt > currentFinishedAt;

  const currentProgress = current.progress?.current ?? 0;
  const incomingProgress = incoming.progress?.current ?? 0;
  if (incomingProgress !== currentProgress) return incomingProgress > currentProgress;
  if (incoming.result !== null && current.result === null) return true;
  if (incoming.error !== null && current.error === null) return true;
  return false;
}

function buildMemberProjection(
  teamId: string,
  memberId: string,
  executions: Record<string, TeamMemberExecutionProjection>,
): TeamMemberSessionProjection {
  const executionList = Object.values(executions);
  const items = mergeSessionItems(
    [],
    executionList.flatMap((execution) => execution.stream.items),
  );
  const latestExecution = selectCurrentExecution(executionList);
  const task = latestExecution?.task ?? null;
  const replayExecution = [...executionList]
    .filter((execution) => execution.replay)
    .sort(compareExecutions)
    .slice(-1)[0];
  const restore = deriveRestoreState(replayExecution, executionList);

  return {
    team_id: teamId,
    member_id: memberId,
    execution_id: latestExecution?.execution_id ?? null,
    sequence: executionList.reduce(
      (latest, execution) => Math.max(latest, execution.sequence),
      0,
    ),
    replay: latestExecution?.replay ?? false,
    stream: boundedSessionSnapshot({
      revision: executionList.reduce(
        (latest, execution) => Math.max(latest, execution.stream.revision),
        0,
      ),
      event_count: executionList.reduce(
        (latest, execution) => Math.max(latest, execution.stream.event_count),
        0,
      ),
      items,
    }),
    task,
    unread: false,
    restore_state: restore.state,
    restore_error_code: restore.errorCode,
    executions,
  };
}

function deriveRestoreState(
  latestReplay: TeamMemberExecutionProjection | undefined,
  executions: TeamMemberExecutionProjection[],
): { state: TeamMemberRestoreState; errorCode: string | null } {
  const activeReplay = executions.find(
    (execution) => execution.replay && isActiveTask(execution.task),
  );
  if (activeReplay) return { state: "restoring", errorCode: null };
  if (!latestReplay) {
    return executions.length > 0
      ? { state: "ready", errorCode: null }
      : { state: "not-started", errorCode: null };
  }
  if (latestReplay.task.state === "Succeeded") {
    return latestReplay.stream.items.length > 0
      ? { state: "ready", errorCode: null }
      : { state: "partial", errorCode: null };
  }
  if (latestReplay.task.state === "Failed" || latestReplay.task.state === "Canceled") {
    return { state: "unavailable", errorCode: latestReplay.task.error?.code ?? null };
  }
  return { state: "restoring", errorCode: null };
}

function retainExecutions(
  executions: Record<string, TeamMemberExecutionProjection>,
): Record<string, TeamMemberExecutionProjection> {
  const entries = Object.entries(executions)
    .sort(([, left], [, right]) => compareExecutions(left, right));
  return Object.fromEntries(entries.slice(-MAX_TEAM_SESSION_EXECUTIONS));
}

function mergeSessionItems(
  current: SessionItemSnapshot[],
  incoming: SessionItemSnapshot[],
): SessionItemSnapshot[] {
  const byIdentity = new Map(current.map((item) => {
    const normalized = sanitizeSessionItem(item);
    return [itemIdentity(normalized), normalized];
  }));
  for (const rawItem of incoming) {
    const item = sanitizeSessionItem(rawItem);
    const key = itemIdentity(item);
    const previous = byIdentity.get(key);
    if (!previous || shouldReplaceItem(previous, item)) {
      byIdentity.set(key, item);
    }
  }
  return [...byIdentity.values()]
    .sort(compareItems)
    .slice(-MAX_TEAM_SESSION_ITEMS);
}

function sanitizeSessionItem(item: SessionItemSnapshot): SessionItemSnapshot {
  return item.kind === "tool" && item.text !== null
    ? { ...item, text: null }
    : item;
}

function boundedSessionSnapshot(snapshot: SessionSnapshot): SessionSnapshot {
  return {
    ...snapshot,
    items: mergeSessionItems([], snapshot.items),
  };
}

function shouldReplaceItem(current: SessionItemSnapshot, incoming: SessionItemSnapshot): boolean {
  if (incoming.sequence !== current.sequence) return incoming.sequence > current.sequence;
  if (incoming.delivery !== current.delivery) return incoming.delivery === "live";
  if (incoming.state !== current.state) return itemStateOrder(incoming.state) > itemStateOrder(current.state);
  return true;
}

function itemStateOrder(state: SessionItemSnapshot["state"]): number {
  return {
    pending: 0,
    streaming: 1,
    completed: 2,
    succeeded: 3,
    failed: 3,
    cancelled: 3,
  }[state];
}

function compareItems(left: SessionItemSnapshot, right: SessionItemSnapshot): number {
  return left.sequence - right.sequence || itemIdentity(left).localeCompare(itemIdentity(right));
}

function itemIdentity(item: SessionItemSnapshot): string {
  const identity = item.identity;
  return [
    identity.session_id,
    identity.member_id,
    identity.execution_id,
    identity.turn_id,
    identity.item_id,
  ].join("\u0000");
}

function compareExecutions(
  left: TeamMemberExecutionProjection,
  right: TeamMemberExecutionProjection,
): number {
  return left.task.started_at.localeCompare(right.task.started_at)
    || left.sequence - right.sequence
    || left.execution_id.localeCompare(right.execution_id);
}

function selectCurrentExecution(
  executions: TeamMemberExecutionProjection[],
): TeamMemberExecutionProjection | undefined {
  const active = executions.filter((execution) => isActiveTask(execution.task));
  return [...(active.length > 0 ? active : executions)].sort(compareExecutions).slice(-1)[0];
}

function isActiveTask(task: TeamMemberTaskSnapshot): boolean {
  return task.state === "Pending" || task.state === "Running" || task.state === "Cancelling";
}
