import { useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { Skeleton, SkeletonSurface, SkeletonText } from "../../foundation/skeleton";
import { cn } from "../../../lib/utils";
import { useRenderActivity, useRenderVisibilityRegistry, useScrollActivitySnapshot } from "./RenderActivityProvider";
import type { RenderPriority } from "./RenderScheduler";
import {
  SKELETON_BLOCK_SIZE_PX,
  type DeferredRenderState,
  type SkeletonBlockSize,
} from "./renderingTypes";

export interface DeferredSkeletonBoundaryProps {
  children: ReactNode;
  className?: string;
  contentVisibilityContainment?: boolean;
  enabled?: boolean;
  fallback?: ReactNode;
  forceReady?: boolean;
  itemKey: string;
  onReady?: (itemKey: string) => void;
  priority?: RenderPriority;
  size?: SkeletonBlockSize;
}

function DefaultDeferredSkeleton({ size }: { size: SkeletonBlockSize }): React.ReactElement {
  return (
    <SkeletonSurface className="deferred-render-skeleton grid gap-3 p-3">
      <Skeleton className="h-4 w-2/5" />
      <SkeletonText lines={3} />
      {size === "tall" ? <Skeleton className="h-24 w-full" /> : null}
    </SkeletonSurface>
  );
}

export function DeferredSkeletonBoundary({
  children,
  className,
  contentVisibilityContainment = true,
  enabled = true,
  fallback,
  forceReady = false,
  itemKey,
  onReady,
  priority: explicitPriority,
  size = "regular",
}: DeferredSkeletonBoundaryProps): React.ReactElement {
  const { scheduler } = useRenderActivity();
  const visibility = useRenderVisibilityRegistry();
  const { phase } = useScrollActivitySnapshot();
  const [state, setState] = useState<DeferredRenderState>(
    enabled && !forceReady ? "skeleton" : "ready",
  );
  const [observedPriority, setObservedPriority] = useState<RenderPriority | null>(null);
  const boundaryRef = useRef<HTMLDivElement | null>(null);
  const itemKeyRef = useRef(itemKey);
  const stateRef = useRef(state);
  const priorityRef = useRef<RenderPriority | null>(explicitPriority ?? null);
  const cancelTaskRef = useRef<(() => void) | null>(null);
  const mountedRef = useRef(true);
  const readyNotifiedRef = useRef(false);
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  stateRef.current = state;
  priorityRef.current = explicitPriority ?? observedPriority;

  const itemChanged = itemKeyRef.current !== itemKey;
  if (itemChanged) {
    itemKeyRef.current = itemKey;
    readyNotifiedRef.current = false;
    stateRef.current = enabled && !forceReady ? "skeleton" : "ready";
  }
  const renderState = itemChanged
    ? enabled && !forceReady ? "skeleton" : "ready"
    : state;

  const cancelQueuedTask = () => {
    cancelTaskRef.current?.();
    cancelTaskRef.current = null;
  };

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      cancelQueuedTask();
    };
  }, []);

  useEffect(() => {
    if (itemChanged) {
      cancelQueuedTask();
      setObservedPriority(null);
      setState(enabled && !forceReady ? "skeleton" : "ready");
    }
  }, [enabled, forceReady, itemKey, itemChanged]);

  useEffect(() => {
    if (!enabled || forceReady || explicitPriority !== undefined) return undefined;
    const element = boundaryRef.current;
    if (!element) return undefined;
    return visibility.register({
      element,
      key: itemKey,
      onPriorityChange: (nextPriority) => {
        setObservedPriority(nextPriority);
        if (nextPriority == null) {
          cancelQueuedTask();
          if (stateRef.current === "queued") {
            stateRef.current = "skeleton";
            setState("skeleton");
          }
        }
      },
    });
  }, [enabled, explicitPriority, forceReady, itemKey, visibility]);

  useEffect(() => {
    if (!enabled || forceReady || renderState === "ready") {
      cancelQueuedTask();
      return;
    }
    const priority = priorityRef.current;
    if (priority == null || phase === "fast" || cancelTaskRef.current) return;

    stateRef.current = "queued";
    setState("queued");
    const taskKey = `deferred-render:${itemKey}`;
    cancelTaskRef.current = scheduler.schedule({
      commit: () => {
        cancelTaskRef.current = null;
        if (!mountedRef.current || itemKeyRef.current !== itemKey) return;
        stateRef.current = "ready";
        setState("ready");
      },
      key: taskKey,
      priority,
    });
  }, [enabled, forceReady, itemKey, phase, renderState, scheduler, observedPriority, explicitPriority]);

  useEffect(() => {
    if (renderState !== "ready" || readyNotifiedRef.current) return;
    readyNotifiedRef.current = true;
    onReadyRef.current?.(itemKey);
  }, [itemKey, renderState]);

  const estimatedSize = SKELETON_BLOCK_SIZE_PX[size];
  const style = {
    "--render-estimated-block-size": `${estimatedSize}px`,
  } as CSSProperties;

  return (
    <div
      aria-hidden={renderState === "ready" ? undefined : true}
      className={cn(
        "deferred-render-boundary",
        renderState === "ready" && contentVisibilityContainment && "render-safe-content",
        className,
      )}
      data-render-item-key={itemKey}
      data-render-state={renderState}
      data-testid="deferred-render-boundary"
      ref={boundaryRef}
      style={style}
    >
      {renderState === "ready" ? children : fallback ?? <DefaultDeferredSkeleton size={size} />}
    </div>
  );
}
