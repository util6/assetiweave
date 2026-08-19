import { useCallback, useEffect, useState } from "react";

export type BackgroundTaskUnsubscribe = () => void;

export interface BackgroundTaskRuntimeAdapter<TState, TEvent> {
  initialState: TState;
  isRunning: (state: TState) => boolean;
  merge: (current: TState, incoming: TState | TEvent) => TState;
  refresh: () => Promise<TState>;
  subscribe: (listener: (event: TEvent) => void) => Promise<BackgroundTaskUnsubscribe>;
  pollIntervalMs?: number;
  reconnectDelayMs?: number;
}

export function useBackgroundTaskRuntime<TState, TEvent>(
  adapter: BackgroundTaskRuntimeAdapter<TState, TEvent>,
) {
  const [state, setState] = useState(adapter.initialState);

  const merge = useCallback((incoming: TState | TEvent) => {
    setState((current) => adapter.merge(current, incoming));
  }, [adapter]);

  const refresh = useCallback(async () => {
    const incoming = await adapter.refresh();
    setState((current) => adapter.merge(current, incoming));
    return incoming;
  }, [adapter]);

  useEffect(() => {
    let cancelled = false;
    let cleanupSubscription: BackgroundTaskUnsubscribe | undefined;
    let reconnectTimer: number | undefined;
    void refresh().catch(() => undefined);

    function connect() {
      void adapter.subscribe((event) => {
        if (!cancelled) {
          merge(event);
        }
      }).then((unsubscribe) => {
        if (cancelled) {
          unsubscribe();
        } else {
          cleanupSubscription = unsubscribe;
        }
      }).catch(() => {
        if (!cancelled) {
          reconnectTimer = window.setTimeout(connect, adapter.reconnectDelayMs ?? 1000);
        }
      });
    }

    connect();

    return () => {
      cancelled = true;
      if (reconnectTimer !== undefined) {
        window.clearTimeout(reconnectTimer);
      }
      cleanupSubscription?.();
    };
  }, [adapter, merge, refresh]);

  useEffect(() => {
    if (!adapter.isRunning(state)) {
      return;
    }

    let polling = false;
    const intervalId = window.setInterval(() => {
      if (polling) {
        return;
      }
      polling = true;
      void refresh()
        .catch(() => undefined)
        .finally(() => {
          polling = false;
        });
    }, adapter.pollIntervalMs ?? 1000);

    return () => window.clearInterval(intervalId);
  }, [adapter, refresh, state]);

  return { merge, refresh, state };
}
