import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import {
  getSkillBackupTask,
  startSkillBackupTask,
  subscribeSkillBackupTasks,
  type SkillBackupTaskSnapshot,
} from "../../services/catalog";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

interface SkillBackupRuntimeEvent {
  snapshot: SkillBackupTaskSnapshot;
}

interface SkillBackupContextValue {
  startBackup: (assetIds: string[]) => Promise<SkillBackupTaskSnapshot>;
  task: SkillBackupTaskSnapshot | null;
}

const SkillBackupContext = createContext<SkillBackupContextValue | null>(null);

export function SkillBackupProvider({ children }: { children: ReactNode }) {
  const adapter = useMemo<BackgroundTaskRuntimeAdapter<SkillBackupTaskSnapshot | null, SkillBackupRuntimeEvent>>(
    () => ({
      initialState: null,
      isRunning: (state) => state?.status === "running",
      merge: (current, incoming) => {
        if (isSkillBackupRuntimeEvent(incoming)) {
          return incoming.snapshot;
        }
        return current?.status === "running" && !incoming ? current : incoming;
      },
      refresh: () => getSkillBackupTask(),
      subscribe: (listener) => subscribeSkillBackupTasks((snapshot) => listener({ snapshot })),
    }),
    [],
  );
  const { merge, state: task } = useBackgroundTaskRuntime(adapter);

  const startBackup = useCallback(async (assetIds: string[]) => {
    const snapshot = await startSkillBackupTask(assetIds);
    merge({ snapshot });
    return snapshot;
  }, [merge]);

  const value = useMemo<SkillBackupContextValue>(
    () => ({ startBackup, task }),
    [startBackup, task],
  );

  return <SkillBackupContext.Provider value={value}>{children}</SkillBackupContext.Provider>;
}

export function useSkillBackup() {
  const context = useContext(SkillBackupContext);
  if (!context) {
    throw new Error("useSkillBackup must be used inside SkillBackupProvider");
  }
  return context;
}

function isSkillBackupRuntimeEvent(
  incoming: SkillBackupTaskSnapshot | SkillBackupRuntimeEvent | null,
): incoming is SkillBackupRuntimeEvent {
  return Boolean(incoming && "snapshot" in incoming);
}
