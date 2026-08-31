import { createContext, useContext, useMemo, type ReactNode } from "react";
import {
  listTeamRunTasks,
  subscribeTeamRunTasks,
} from "../../services/teamWorkflow";
import type { TeamRuntimeTaskSnapshot } from "../../types/team";
import {
  useBackgroundTaskRuntime,
  type BackgroundTaskRuntimeAdapter,
} from "./BackgroundTaskRuntime";

interface TeamTaskContextValue {
  tasks: TeamRuntimeTaskSnapshot[];
  refresh: () => Promise<void>;
  getTask: (taskId: string) => TeamRuntimeTaskSnapshot | undefined;
}

const TeamTaskContext = createContext<TeamTaskContextValue | null>(null);

export function TeamTaskProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<
    BackgroundTaskRuntimeAdapter<TeamRuntimeTaskSnapshot[], TeamRuntimeTaskSnapshot>
  >(
    () => ({
      initialState: [],
      isRunning: (tasks) => tasks.some((task) => task.state === "Pending" || task.state === "Running" || task.state === "Cancelling"),
      merge: (current, incoming) => {
        const snapshots = Array.isArray(incoming) ? incoming : [incoming];
        const byId = new Map(current.map((task) => [task.task_id, task]));
        for (const task of snapshots) byId.set(task.task_id, task);
        return [...byId.values()];
      },
      refresh: listTeamRunTasks,
      subscribe: subscribeTeamRunTasks,
      pollIntervalMs: 1000,
    }),
    [],
  );
  const runtime = useBackgroundTaskRuntime(adapter);
  const value = useMemo<TeamTaskContextValue>(() => ({
    tasks: runtime.state,
    refresh: async () => {
      await runtime.refresh();
    },
    getTask: (taskId) => runtime.state.find((task) => task.task_id === taskId),
  }), [runtime.refresh, runtime.state]);
  return <TeamTaskContext.Provider value={value}>{children}</TeamTaskContext.Provider>;
}

export function useTeamTasks() {
  const context = useContext(TeamTaskContext);
  if (!context) throw new Error("useTeamTasks must be used inside TeamTaskProvider");
  return context;
}

export function useOptionalTeamTasks() {
  return useContext(TeamTaskContext);
}
