import { useEffect, useRef } from "react";
import { useQueryClient, type QueryKey } from "@tanstack/react-query";

export interface TaskEventBridgeProps<State, Event> {
  queryKey: QueryKey;
  subscribe(listener: (event: Event) => void): Promise<() => void>;
  merge?(current: State | undefined, event: Event): State | undefined;
  validateTenant?(event: Event): boolean | null;
}

export function TaskEventBridge<State, Event>({
  queryKey,
  subscribe,
  merge,
  validateTenant,
}: TaskEventBridgeProps<State, Event>): null {
  const queryClient = useQueryClient();
  const queryKeyRef = useRef(queryKey);
  queryKeyRef.current = queryKey;
  const mergeRef = useRef(merge);
  mergeRef.current = merge;
  const subscribeRef = useRef(subscribe);
  subscribeRef.current = subscribe;
  const validateTenantRef = useRef(validateTenant);
  validateTenantRef.current = validateTenant;

  const keySerialized = JSON.stringify(queryKey);

  useEffect(() => {
    let unmounted = false;
    let cleanupFn: (() => void) | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const listener = (event: Event) => {
      if (unmounted) return;
      const currentMerge = mergeRef.current;
      const currentKey = queryKeyRef.current;
      const validate = validateTenantRef.current;

      if (validate) {
        const match = validate(event);
        if (match === false) {
          // 属于其他租户的任务事件，丢弃
          return;
        }
        if (match === null) {
          // 缺乏租户归属信息的事件，仅触发当前租户重新校验，严禁盲写缓存
          void queryClient.invalidateQueries(
            { queryKey: currentKey, exact: true },
            { cancelRefetch: false },
          );
          return;
        }
      }

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
