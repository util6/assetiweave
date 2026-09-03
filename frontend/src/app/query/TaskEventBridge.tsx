import { useEffect, useRef } from "react";
import { useQueryClient, type QueryKey } from "@tanstack/react-query";

export interface TaskEventBridgeProps<State, Event> {
  queryKey: QueryKey;
  subscribe(listener: (event: Event) => void): Promise<() => void>;
  merge?(current: State | undefined, event: Event): State | undefined;
}

export function TaskEventBridge<State, Event>({
  queryKey,
  subscribe,
  merge,
}: TaskEventBridgeProps<State, Event>): null {
  const queryClient = useQueryClient();
  const queryKeyRef = useRef(queryKey);
  queryKeyRef.current = queryKey;
  const mergeRef = useRef(merge);
  mergeRef.current = merge;
  const subscribeRef = useRef(subscribe);
  subscribeRef.current = subscribe;

  const keySerialized = JSON.stringify(queryKey);

  useEffect(() => {
    let unmounted = false;
    let cleanupFn: (() => void) | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const listener = (event: Event) => {
      if (unmounted) return;
      const currentMerge = mergeRef.current;
      const currentKey = queryKeyRef.current;
      if (currentMerge) {
        queryClient.setQueryData<State>(currentKey, (current) =>
          currentMerge(current, event),
        );
      } else {
        void queryClient.invalidateQueries(
          { queryKey: currentKey, exact: true },
          { cancelRefetch: false },
        );
      }
    };

    const connect = () => {
      if (unmounted) return;
      subscribeRef
        .current(listener)
        .then((cleanup) => {
          if (unmounted) {
            cleanup();
            return;
          }
          cleanupFn = cleanup;
        })
        .catch(() => {
          if (unmounted) return;
          reconnectTimer = setTimeout(connect, 1000);
        });
    };

    connect();

    return () => {
      unmounted = true;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
      }
      if (cleanupFn) {
        cleanupFn();
        cleanupFn = null;
      }
    };
  }, [queryClient, keySerialized]);

  return null;
}
