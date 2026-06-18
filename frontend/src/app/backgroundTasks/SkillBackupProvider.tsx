import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  getSkillBackupTask,
  startSkillBackupTask,
  type SkillBackupTaskSnapshot,
} from "../../services/catalog";

const SKILL_BACKUP_TASK_UPDATED_EVENT = "skill-backup-task-updated";
const BACKUP_STATUS_POLL_INTERVAL_MS = 1000;

interface SkillBackupContextValue {
  startBackup: (assetIds: string[]) => Promise<SkillBackupTaskSnapshot>;
  task: SkillBackupTaskSnapshot | null;
}

const SkillBackupContext = createContext<SkillBackupContextValue | null>(null);

export function SkillBackupProvider({ children }: { children: ReactNode }) {
  const [task, setTask] = useState<SkillBackupTaskSnapshot | null>(null);

  const refreshTask = useCallback(async () => {
    const snapshot = await getSkillBackupTask();
    setTask(snapshot);
    return snapshot;
  }, []);

  useEffect(() => {
    let cancelled = false;
    void getSkillBackupTask()
      .then((snapshot) => {
        if (!cancelled) {
          setTask((current) => current ?? snapshot);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<SkillBackupTaskSnapshot>(
      SKILL_BACKUP_TASK_UPDATED_EVENT,
      (event) => {
        if (!cancelled) {
          setTask(event.payload);
        }
      },
    )
      .then((removeListener) => {
        if (cancelled) {
          removeListener();
        } else {
          unlisten = removeListener;
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (task?.status !== "running") {
      return;
    }

    let polling = false;
    const intervalId = window.setInterval(() => {
      if (polling) {
        return;
      }
      polling = true;
      void refreshTask()
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, BACKUP_STATUS_POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [refreshTask, task?.id, task?.status]);

  const startBackup = useCallback(async (assetIds: string[]) => {
    const snapshot = await startSkillBackupTask(assetIds);
    setTask(snapshot);
    return snapshot;
  }, []);

  const value = useMemo<SkillBackupContextValue>(
    () => ({
      startBackup,
      task,
    }),
    [startBackup, task],
  );

  return (
    <SkillBackupContext.Provider value={value}>
      {children}
    </SkillBackupContext.Provider>
  );
}

export function useSkillBackup() {
  const context = useContext(SkillBackupContext);
  if (!context) {
    throw new Error("useSkillBackup must be used inside SkillBackupProvider");
  }
  return context;
}
