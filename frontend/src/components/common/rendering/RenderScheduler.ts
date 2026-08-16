import type { ScrollPhase } from "./ScrollActivityController";

export type RenderPriority = 0 | 1 | 2;

export interface ScheduledRenderTask {
  commit: () => void;
  key: string;
  priority: RenderPriority;
}

export interface RenderScheduler {
  cancel(key: string): void;
  dispose(): void;
  schedule(task: ScheduledRenderTask): () => void;
  setPhase(phase: ScrollPhase): void;
  size(): number;
}

interface SchedulerOptions {
  onError?: (error: unknown, key: string) => void;
}

const MOVING_FRAME_LIMIT = 1;
const IDLE_TASK_LIMIT = 4;
const IDLE_BUDGET_MS = 4;

export function createRenderScheduler(options: SchedulerOptions = {}): RenderScheduler {
  let phase: ScrollPhase = "idle";
  let frameId: number | null = null;
  let disposed = false;
  const tasks = new Map<string, ScheduledRenderTask>();

  const clearFrame = () => {
    if (frameId == null) return;
    cancelAnimationFrame(frameId);
    frameId = null;
  };

  const scheduleFlush = () => {
    if (disposed || phase === "fast" || tasks.size === 0 || frameId != null) return;
    frameId = requestAnimationFrame(flush);
  };

  const flush = () => {
    frameId = null;
    if (disposed || phase === "fast" || tasks.size === 0) return;

    const startedAt = performance.now();
    const budget = phase === "moving" ? MOVING_FRAME_LIMIT : IDLE_TASK_LIMIT;
    let committed = 0;
    const ordered = [...tasks.values()].sort((left, right) => left.priority - right.priority);

    for (const task of ordered) {
      if (committed >= budget || (phase === "idle" && performance.now() - startedAt >= IDLE_BUDGET_MS)) break;
      if (!tasks.delete(task.key)) continue;
      committed += 1;
      try {
        task.commit();
      } catch (error) {
        options.onError?.(error, task.key);
      }
    }
    scheduleFlush();
  };

  return {
    cancel(key) {
      tasks.delete(key);
      if (tasks.size === 0) clearFrame();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      tasks.clear();
      clearFrame();
    },
    schedule(task) {
      if (disposed) return () => undefined;
      tasks.set(task.key, task);
      scheduleFlush();
      return () => {
        if (!disposed) tasks.delete(task.key);
        if (tasks.size === 0) clearFrame();
      };
    },
    setPhase(nextPhase) {
      if (disposed || phase === nextPhase) return;
      phase = nextPhase;
      if (phase === "fast") clearFrame();
      else scheduleFlush();
    },
    size() {
      return tasks.size;
    },
  };
}
