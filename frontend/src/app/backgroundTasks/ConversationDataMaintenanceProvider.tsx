import { useCallback, type ReactNode } from "react";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  auditConversationData,
  cancelConversationDataMaintenance,
  listConversationDataMaintenanceTasks,
  repairConversationData,
  subscribeConversationDataMaintenanceTasks,
  type ConversationDataAuditParams,
  type ConversationDataMaintenanceTaskSnapshot,
  type ConversationDataRepairParams,
} from "../../services/conversations";
import { useQueryScope } from "../query/QueryScopeProvider";
import { taskKeys } from "../query/taskKeys";
import { TaskEventBridge } from "../query/TaskEventBridge";
import type { QueryScope } from "../query/catalogQueries";

export interface ConversationDataMaintenanceContextValue {
  task: ConversationDataMaintenanceTaskSnapshot | null;
  tasks: ConversationDataMaintenanceTaskSnapshot[];
  audit: (
    params?: ConversationDataAuditParams,
  ) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  repair: (
    params?: ConversationDataRepairParams,
  ) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  cancel: (taskId: string) => Promise<ConversationDataMaintenanceTaskSnapshot>;
  refresh: () => Promise<void>;
}

export function conversationDataMaintenanceQueryOptions(scope: QueryScope) {
  return queryOptions<ConversationDataMaintenanceTaskSnapshot[]>({
    queryKey: taskKeys.resource(scope, "conversation-maintenance"),
    queryFn: listConversationDataMaintenanceTasks,
    structuralSharing: (oldData, newData) => {
      const current =
        (oldData as ConversationDataMaintenanceTaskSnapshot[] | undefined) ?? [];
      const incoming =
        (newData as ConversationDataMaintenanceTaskSnapshot[] | undefined) ?? [];
      return mergeMaintenanceTask(current, incoming);
    },
    staleTime: 1000,
  });
}

export function ConversationDataMaintenanceProvider({
  children,
}: {
  children?: ReactNode;
} = {}) {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryKey = taskKeys.resource(activeScope, "conversation-maintenance");

  useQuery({
    ...conversationDataMaintenanceQueryOptions(activeScope),
    enabled: true,
    refetchInterval: (query) => {
      const tasks = query.state.data;
      const isRunning =
        tasks &&
        tasks.some(
          (task) => task.status === "running" || task.status === "cancelling",
        );
      return isRunning ? 1000 : 10000;
    },
    refetchIntervalInBackground: true,
  });

  return (
    <>
      <TaskEventBridge<
        ConversationDataMaintenanceTaskSnapshot[],
        ConversationDataMaintenanceTaskSnapshot
      >
        merge={(current, task) => mergeMaintenanceTask(current ?? [], task)}
        queryKey={queryKey}
        subscribe={subscribeConversationDataMaintenanceTasks}
      />
      {children ?? null}
    </>
  );
}

export function useConversationDataMaintenance(): ConversationDataMaintenanceContextValue {
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const queryClient = useQueryClient();
  const queryKey = taskKeys.resource(activeScope, "conversation-maintenance");

  const query = useQuery({
    ...conversationDataMaintenanceQueryOptions(activeScope),
  });

  const tasks = query.data ?? [];

  const audit = useCallback(
    async (params: ConversationDataAuditParams = {}) => {
      const task = await auditConversationData(params);
      queryClient.setQueryData<ConversationDataMaintenanceTaskSnapshot[]>(
        queryKey,
        (current) => mergeMaintenanceTask(current ?? [], task),
      );
      return task;
    },
    [queryClient, queryKey],
  );

  const repair = useCallback(
    async (params: ConversationDataRepairParams = {}) => {
      const task = await repairConversationData(params);
      queryClient.setQueryData<ConversationDataMaintenanceTaskSnapshot[]>(
        queryKey,
        (current) => mergeMaintenanceTask(current ?? [], task),
      );
      return task;
    },
    [queryClient, queryKey],
  );

  const cancel = useCallback(
    async (taskId: string) => {
      const task = await cancelConversationDataMaintenance(taskId);
      queryClient.setQueryData<ConversationDataMaintenanceTaskSnapshot[]>(
        queryKey,
        (current) => mergeMaintenanceTask(current ?? [], task),
      );
      return task;
    },
    [queryClient, queryKey],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ exact: true, queryKey });
  }, [queryClient, queryKey]);

  return {
    audit,
    cancel,
    refresh,
    repair,
    task: tasks[tasks.length - 1] ?? null,
    tasks,
  };
}

export function mergeMaintenanceTask(
  current: ConversationDataMaintenanceTaskSnapshot[],
  incoming:
    | ConversationDataMaintenanceTaskSnapshot
    | ConversationDataMaintenanceTaskSnapshot[],
) {
  const snapshots = Array.isArray(incoming) ? incoming : [incoming];
  const byId = new Map(current.map((task) => [task.id, task]));
  for (const snapshot of snapshots) {
    byId.set(snapshot.id, snapshot);
  }
  return [...byId.values()].sort((left, right) =>
    left.started_at.localeCompare(right.started_at),
  );
}
