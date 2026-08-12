import { useEffect, useRef, useState } from "react";
import { PageSkeleton, type PageSkeletonKind } from "../components/foundation/Skeleton";
import { cn } from "../lib/utils";

export const ROUTE_TRANSITION_DURATION_MS = 300;

export type RouteTransitionKind = Exclude<
  PageSkeletonKind,
  | "memory-detail"
  | "memory-dreams"
  | "memory-library"
  | "memory-overview"
  | "memory-recall"
>;

export interface RouteTransitionState {
  id: number;
  kind: RouteTransitionKind;
  label: string;
  phase: "enter" | "exit";
}

export function useRouteTransition({ durationMs = ROUTE_TRANSITION_DURATION_MS } = {}) {
  const [transition, setTransition] = useState<RouteTransitionState | null>(null);
  const nextId = useRef(0);
  const startedAt = useRef(0);
  const exitTimer = useRef<number | null>(null);
  const clearTimer = useRef<number | null>(null);
  const fallbackTimer = useRef<number | null>(null);

  function clearTimers() {
    if (exitTimer.current !== null) {
      window.clearTimeout(exitTimer.current);
      exitTimer.current = null;
    }
    if (clearTimer.current !== null) {
      window.clearTimeout(clearTimer.current);
      clearTimer.current = null;
    }
    if (fallbackTimer.current !== null) {
      window.clearTimeout(fallbackTimer.current);
      fallbackTimer.current = null;
    }
  }

  function startTransition(kind: RouteTransitionKind, label: string) {
    clearTimers();
    const id = nextId.current + 1;
    nextId.current = id;
    startedAt.current = performance.now();
    setTransition({ id, kind, label, phase: "enter" });

    fallbackTimer.current = window.setTimeout(() => completeTransition(id), Math.max(durationMs, 3000));
  }

  function completeTransition(id = nextId.current) {
    if (id !== nextId.current) {
      return;
    }

    if (fallbackTimer.current !== null) {
      window.clearTimeout(fallbackTimer.current);
      fallbackTimer.current = null;
    }

    const exitDelay = Math.max(0, durationMs - (performance.now() - startedAt.current));
    exitTimer.current = window.setTimeout(() => {
      setTransition((current) => (current?.id === id ? { ...current, phase: "exit" } : current));
    }, exitDelay);
    clearTimer.current = window.setTimeout(() => {
      setTransition((current) => (current?.id === id ? null : current));
      exitTimer.current = null;
      clearTimer.current = null;
    }, exitDelay + 140);
  }

  useEffect(() => clearTimers, []);

  return { completeTransition, startTransition, transition };
}

export function RouteTransitionOverlay({ transition }: { transition: RouteTransitionState | null }) {
  if (!transition) {
    return null;
  }

  return (
    <div
      aria-busy="true"
      aria-live="polite"
      className={cn(
        "aurora-route-transition pointer-events-none absolute inset-0 z-20 overflow-auto",
        transition.phase === "exit" && "aurora-route-transition-exit",
      )}
      data-route-transition={transition.phase}
      data-route-transition-id={transition.id}
    >
      <div className="aurora-route-progress" />
      <PageSkeleton kind={transition.kind} label={transition.label} />
    </div>
  );
}
