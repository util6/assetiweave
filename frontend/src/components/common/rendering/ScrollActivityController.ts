import {
  FAST_SCROLL_ENTER_PX_PER_MS,
  FAST_SCROLL_EXIT_PX_PER_MS,
  SCROLL_IDLE_DELAY_MS,
  VELOCITY_INSTANT_WEIGHT,
  VELOCITY_PREVIOUS_WEIGHT,
} from "./renderingConstants";

export type ScrollPhase = "idle" | "moving" | "fast";

export interface ScrollActivitySnapshot {
  direction: "backward" | "forward" | null;
  phase: ScrollPhase;
  velocity: number;
}

export interface ScrollActivityController {
  attach(element: HTMLElement): () => void;
  getSnapshot(): ScrollActivitySnapshot;
  subscribe(listener: () => void): () => void;
}

const IDLE_SNAPSHOT: ScrollActivitySnapshot = {
  direction: null,
  phase: "idle",
  velocity: 0,
};

export function createScrollActivityController(): ScrollActivityController {
  let element: HTMLElement | null = null;
  let frameId: number | null = null;
  let idleTimerId: number | null = null;
  let previousOffset = 0;
  let previousTime = 0;
  let previousVelocity = 0;
  let pendingScroll = false;
  let snapshot = IDLE_SNAPSHOT;
  const listeners = new Set<() => void>();

  const notifyIfChanged = (next: ScrollActivitySnapshot) => {
    if (
      next.direction === snapshot.direction
      && next.phase === snapshot.phase
      && next.velocity === snapshot.velocity
    ) {
      return;
    }
    snapshot = next;
    listeners.forEach((listener) => listener());
  };

  const clearFrame = () => {
    if (frameId == null) return;
    cancelAnimationFrame(frameId);
    frameId = null;
  };

  const clearIdleTimer = () => {
    if (idleTimerId == null) return;
    window.clearTimeout(idleTimerId);
    idleTimerId = null;
  };

  const scheduleIdle = () => {
    clearIdleTimer();
    idleTimerId = window.setTimeout(() => {
      idleTimerId = null;
      pendingScroll = false;
      previousVelocity = 0;
      notifyIfChanged(IDLE_SNAPSHOT);
    }, SCROLL_IDLE_DELAY_MS);
  };

  const sample = () => {
    frameId = null;
    if (!element || !pendingScroll) return;

    const now = performance.now();
    const offset = element.scrollTop;
    const elapsed = Math.max(now - previousTime, 1);
    const instantVelocity = Math.abs(offset - previousOffset) / elapsed;
    const velocity = previousVelocity * VELOCITY_PREVIOUS_WEIGHT
      + instantVelocity * VELOCITY_INSTANT_WEIGHT;
    const direction = offset > previousOffset
      ? "forward"
      : offset < previousOffset
        ? "backward"
        : snapshot.direction;
    const phase: ScrollPhase = snapshot.phase === "fast"
      ? velocity > FAST_SCROLL_EXIT_PX_PER_MS ? "fast" : "moving"
      : velocity >= FAST_SCROLL_ENTER_PX_PER_MS ? "fast" : "moving";

    previousOffset = offset;
    previousTime = now;
    previousVelocity = velocity;
    pendingScroll = false;
    notifyIfChanged({ direction, phase, velocity });
  };

  const scheduleSample = () => {
    if (frameId != null) return;
    frameId = requestAnimationFrame(sample);
  };

  const detach = () => {
    if (element) element.removeEventListener("scroll", handleScroll);
    element = null;
    clearFrame();
    clearIdleTimer();
    pendingScroll = false;
    previousVelocity = 0;
    snapshot = IDLE_SNAPSHOT;
  };

  function handleScroll() {
    if (!element) return;
    clearIdleTimer();
    if (!pendingScroll) {
      pendingScroll = true;
      notifyIfChanged({
        direction: snapshot.direction,
        phase: snapshot.phase === "fast" ? "fast" : "moving",
        velocity: snapshot.velocity,
      });
    }
    scheduleIdle();
    scheduleSample();
  }

  return {
    attach(nextElement) {
      detach();
      element = nextElement;
      previousOffset = nextElement.scrollTop;
      previousTime = performance.now();
      nextElement.addEventListener("scroll", handleScroll, { passive: true });
      return detach;
    },
    getSnapshot() {
      return snapshot;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
