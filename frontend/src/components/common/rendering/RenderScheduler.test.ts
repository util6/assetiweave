/* @vitest-environment jsdom */

import { act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createRenderScheduler } from "./RenderScheduler";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function createRafHarness() {
  const callbacks = new Map<number, FrameRequestCallback>();
  let nextId = 1;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = nextId++;
    callbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => callbacks.delete(id));
  return {
    callbacks,
    flush: (time = 0) => {
      const pending = [...callbacks.values()];
      callbacks.clear();
      pending.forEach((callback) => callback(time));
    },
  };
}

const task = (key: string, priority: 0 | 1 | 2, commit: () => void) => ({ key, priority, commit });

describe("RenderScheduler", () => {
  it("does not submit while fast, then flushes moving one task per frame", () => {
    const raf = createRafHarness();
    const scheduler = createRenderScheduler();
    const commits: string[] = [];
    scheduler.setPhase("fast");
    scheduler.schedule(task("first", 0, () => commits.push("first")));
    expect(raf.callbacks.size).toBe(0);
    expect(commits).toEqual([]);

    scheduler.setPhase("moving");
    expect(raf.callbacks.size).toBe(1);
    act(() => raf.flush());
    expect(commits).toEqual(["first"]);
    expect(scheduler.size()).toBe(0);
  });

  it("deduplicates keys, orders idle work, and respects the four-item budget", () => {
    const raf = createRafHarness();
    vi.spyOn(performance, "now").mockReturnValue(0);
    const scheduler = createRenderScheduler();
    scheduler.setPhase("idle");
    const commits: string[] = [];
    scheduler.schedule(task("rear", 2, () => commits.push("rear")));
    scheduler.schedule(task("front", 0, () => commits.push("front")));
    scheduler.schedule(task("middle", 1, () => commits.push("middle")));
    scheduler.schedule(task("duplicate", 2, () => commits.push("old")));
    scheduler.schedule(task("duplicate", 0, () => commits.push("new")));
    scheduler.schedule(task("fourth", 2, () => commits.push("fourth")));
    scheduler.schedule(task("fifth", 2, () => commits.push("fifth")));

    expect(scheduler.size()).toBe(6);
    act(() => raf.flush());
    expect(commits).toEqual(["front", "new", "middle", "rear"]);
    expect(scheduler.size()).toBe(2);
    act(() => raf.flush());
    expect(commits).toEqual(["front", "new", "middle", "rear", "fourth", "fifth"]);
  });

  it("isolates commit failures and makes cancellation and disposal idempotent", () => {
    const raf = createRafHarness();
    const errors: unknown[] = [];
    const scheduler = createRenderScheduler({ onError: (error) => errors.push(error) });
    scheduler.setPhase("moving");
    const committed: string[] = [];
    scheduler.schedule(task("bad", 0, () => { throw new Error("boom"); }));
    scheduler.schedule(task("good", 1, () => committed.push("good")));
    scheduler.cancel("missing");
    act(() => raf.flush());
    expect(errors).toHaveLength(1);
    expect(committed).toEqual([]);
    act(() => raf.flush());
    expect(committed).toEqual(["good"]);
    scheduler.dispose();
    scheduler.dispose();
    scheduler.cancel("good");
    expect(scheduler.size()).toBe(0);
  });
});
