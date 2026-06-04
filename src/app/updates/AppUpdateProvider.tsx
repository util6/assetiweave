import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";
import {
  checkForAppUpdate,
  closeAppUpdate,
  getCurrentAppVersion,
  isTauriRuntime,
  openReleasePage,
  relaunchApp,
  toAppUpdateInfo,
  type AppUpdateInfo,
} from "../../services/appUpdater";
import {
  isRetryableUpdaterError,
  retryWithBackoff,
  sanitizeUpdaterError,
  UPDATE_CHECK_RETRY_DELAYS_MS,
  UPDATE_DOWNLOAD_RETRY_DELAYS_MS,
} from "../../utils/updaterRetry";

const AUTO_CHECK_DELAY_MS = 5000;
const AUTO_CHECK_INTERVAL_MS = 60 * 60 * 1000;

export type AppUpdateSource = "auto" | "manual";
export type AppUpdateStatus = "idle" | "checking" | "available" | "upToDate" | "downloading" | "installing" | "ready" | "error";

export interface AppUpdateState {
  currentVersion?: string;
  error?: string;
  info: AppUpdateInfo | null;
  lastCheckedAt?: string;
  progress: number;
  retryAttempt?: number;
  retryTotal?: number;
  source: AppUpdateSource | null;
  status: AppUpdateStatus;
  supported: boolean;
}

interface AppUpdateContextValue {
  checkForUpdates: (source?: AppUpdateSource) => Promise<void>;
  closeDialog: () => void;
  dialogOpen: boolean;
  downloadAndInstall: () => Promise<void>;
  openDialog: () => void;
  openReleases: () => Promise<void>;
  restartApp: () => Promise<void>;
  state: AppUpdateState;
}

const AppUpdateContext = createContext<AppUpdateContextValue | null>(null);

function createInitialState(): AppUpdateState {
  return {
    info: null,
    progress: 0,
    source: null,
    status: "idle",
    supported: isTauriRuntime(),
  };
}

export function AppUpdateProvider({ children }: { children: ReactNode }) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [state, setStateValue] = useState<AppUpdateState>(() => createInitialState());
  const stateRef = useRef(state);
  const updateRef = useRef<Update | null>(null);
  const requestIdRef = useRef(0);

  const setState = useCallback((next: AppUpdateState | ((previous: AppUpdateState) => AppUpdateState)) => {
    setStateValue((previous) => {
      const resolved = typeof next === "function" ? next(previous) : next;
      stateRef.current = resolved;
      return resolved;
    });
  }, []);

  useEffect(() => {
    if (!state.supported) {
      return;
    }

    let cancelled = false;
    void getCurrentAppVersion()
      .then((currentVersion) => {
        if (!cancelled) {
          setState((previous) => ({ ...previous, currentVersion }));
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [setState, state.supported]);

  const checkForUpdates = useCallback(
    async (source: AppUpdateSource = "manual") => {
      if (!stateRef.current.supported || stateRef.current.status === "downloading" || stateRef.current.status === "installing") {
        return;
      }

      const requestId = requestIdRef.current + 1;
      requestIdRef.current = requestId;
      if (source === "manual") {
        setDialogOpen(true);
      }
      setState((previous) => ({
        ...previous,
        error: undefined,
        progress: 0,
        retryAttempt: undefined,
        retryTotal: undefined,
        source,
        status: "checking",
      }));

      try {
        const update = await retryWithBackoff(() => checkForAppUpdate(), {
          delaysMs: UPDATE_CHECK_RETRY_DELAYS_MS,
          shouldRetry: isRetryableUpdaterError,
          onRetry: ({ attempt, totalRetries }) => {
            if (requestIdRef.current !== requestId) {
              return;
            }
            setState((previous) => ({
              ...previous,
              retryAttempt: attempt,
              retryTotal: totalRetries,
            }));
          },
        });

        if (requestIdRef.current !== requestId) {
          await closeAppUpdate(update);
          return;
        }

        if (!update) {
          await closeAppUpdate(updateRef.current);
          updateRef.current = null;
          const currentVersion = await getCurrentAppVersion().catch(() => stateRef.current.currentVersion);
          setState((previous) => ({
            ...previous,
            currentVersion,
            error: undefined,
            info: null,
            lastCheckedAt: new Date().toISOString(),
            progress: 0,
            retryAttempt: undefined,
            retryTotal: undefined,
            source,
            status: "upToDate",
          }));
          return;
        }

        await closeAppUpdate(updateRef.current);
        updateRef.current = update;
        setState((previous) => ({
          ...previous,
          currentVersion: update.currentVersion,
          error: undefined,
          info: toAppUpdateInfo(update),
          lastCheckedAt: new Date().toISOString(),
          progress: 0,
          retryAttempt: undefined,
          retryTotal: undefined,
          source,
          status: "available",
        }));
        setDialogOpen(true);
      } catch (error) {
        if (requestIdRef.current !== requestId) {
          return;
        }
        setState((previous) => ({
          ...previous,
          error: sanitizeUpdaterError(error),
          lastCheckedAt: new Date().toISOString(),
          progress: 0,
          retryAttempt: undefined,
          retryTotal: undefined,
          source,
          status: "error",
        }));
        if (source === "manual") {
          setDialogOpen(true);
        }
      }
    },
    [setState],
  );

  const downloadAndInstall = useCallback(async () => {
    const targetVersion = stateRef.current.info?.version;
    if (!stateRef.current.supported || !targetVersion || stateRef.current.status === "downloading" || stateRef.current.status === "installing") {
      return;
    }

    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;
    setDialogOpen(true);
    setState((previous) => ({
      ...previous,
      error: undefined,
      progress: 0,
      retryAttempt: undefined,
      retryTotal: undefined,
      status: "downloading",
    }));

    let attempt = 0;
    try {
      const installedVersion = await retryWithBackoff(
        async () => {
          attempt += 1;
          const update = attempt === 1 && updateRef.current?.version === targetVersion ? updateRef.current : await checkForAppUpdate();
          if (!update) {
            throw new Error("No update is available from the configured update endpoint.");
          }
          if (update.version !== targetVersion) {
            throw new Error(`Expected update ${targetVersion}, but updater returned ${update.version}.`);
          }

          updateRef.current = update;
          let downloaded = 0;
          let contentLength = 0;
          try {
            await update.downloadAndInstall((event) => {
              if (requestIdRef.current !== requestId) {
                return;
              }
              if (event.event === "Started") {
                contentLength = event.data.contentLength ?? 0;
                downloaded = 0;
              }
              if (event.event === "Progress") {
                downloaded += event.data.chunkLength;
              }
              setState((previous) => applyDownloadEvent(previous, event, downloaded, contentLength));
            });
          } catch (error) {
            await closeAppUpdate(update);
            if (updateRef.current === update) {
              updateRef.current = null;
            }
            throw error;
          }
          return update.version;
        },
        {
          delaysMs: UPDATE_DOWNLOAD_RETRY_DELAYS_MS,
          shouldRetry: isRetryableUpdaterError,
          onRetry: ({ attempt: retryAttempt, totalRetries }) => {
            if (requestIdRef.current !== requestId) {
              return;
            }
            setState((previous) => ({
              ...previous,
              progress: 0,
              retryAttempt,
              retryTotal: totalRetries,
            }));
          },
        },
      );

      await closeAppUpdate(updateRef.current);
      updateRef.current = null;
      if (requestIdRef.current !== requestId) {
        return;
      }
      setState((previous) => ({
        ...previous,
        error: undefined,
        info: previous.info ? { ...previous.info, version: installedVersion } : previous.info,
        progress: 100,
        retryAttempt: undefined,
        retryTotal: undefined,
        status: "ready",
      }));
    } catch (error) {
      await closeAppUpdate(updateRef.current);
      updateRef.current = null;
      if (requestIdRef.current !== requestId) {
        return;
      }
      setState((previous) => ({
        ...previous,
        error: sanitizeUpdaterError(error),
        progress: 0,
        retryAttempt: undefined,
        retryTotal: undefined,
        status: "error",
      }));
      setDialogOpen(true);
    }
  }, [setState]);

  const restartApp = useCallback(async () => {
    if (!stateRef.current.supported) {
      return;
    }
    setState((previous) => ({
      ...previous,
      error: undefined,
      status: "installing",
    }));
    try {
      await relaunchApp();
    } catch (error) {
      setState((previous) => ({
        ...previous,
        error: sanitizeUpdaterError(error),
        status: "ready",
      }));
    }
  }, [setState]);

  useEffect(() => {
    if (!state.supported || isDevRuntime()) {
      return;
    }

    const initialCheckId = window.setTimeout(() => {
      void checkForUpdates("auto");
    }, AUTO_CHECK_DELAY_MS);
    const intervalId = window.setInterval(() => {
      void checkForUpdates("auto");
    }, AUTO_CHECK_INTERVAL_MS);

    return () => {
      window.clearTimeout(initialCheckId);
      window.clearInterval(intervalId);
    };
  }, [checkForUpdates, state.supported]);

  useEffect(() => {
    return () => {
      void closeAppUpdate(updateRef.current);
    };
  }, []);

  const value = useMemo<AppUpdateContextValue>(
    () => ({
      checkForUpdates,
      closeDialog: () => setDialogOpen(false),
      dialogOpen,
      downloadAndInstall,
      openDialog: () => setDialogOpen(true),
      openReleases: openReleasePage,
      restartApp,
      state,
    }),
    [checkForUpdates, dialogOpen, downloadAndInstall, restartApp, state],
  );

  return <AppUpdateContext.Provider value={value}>{children}</AppUpdateContext.Provider>;
}

export function useAppUpdater() {
  const context = useContext(AppUpdateContext);
  if (!context) {
    throw new Error("useAppUpdater must be used inside AppUpdateProvider");
  }
  return context;
}

function isDevRuntime() {
  return Boolean((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV);
}

function applyDownloadEvent(state: AppUpdateState, event: DownloadEvent, downloaded: number, contentLength: number): AppUpdateState {
  if (event.event === "Started") {
    return {
      ...state,
      progress: 0,
      retryAttempt: undefined,
      retryTotal: undefined,
      status: "downloading",
    };
  }

  if (event.event === "Progress") {
    const nextProgress = contentLength > 0 ? Math.min(95, Math.round((downloaded / contentLength) * 100)) : Math.min(95, state.progress + 1);
    return {
      ...state,
      progress: nextProgress,
      status: "downloading",
    };
  }

  return {
    ...state,
    progress: 100,
    status: "installing",
  };
}
