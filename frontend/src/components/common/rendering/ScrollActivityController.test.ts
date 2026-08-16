/* @vitest-environment jsdom */

import { act } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  FAST_SCROLL_ENTER_PX_PER_MS,
  FAST_SCROLL_EXIT_PX_PER_MS,
  SCROLL_IDLE_DELAY_MS,
} from "./renderingConstants";
import { createScrollActivityController } from "./ScrollActivityController";

interface RafHarness {
  callbacks: Map<number, FrameRequestCallback>;
  flush: (time?: number) => void;
}

function createRafHarness(): RafHarness {
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

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ScrollActivityController", () => {
  it("attaches one passive listener and reports direction and moving phase", () => {
    const raf = createRafHarness();
    const now = vi.spyOn(performance, "now").mockReturnValueOnce(0).mockReturnValueOnce(20);
    const element = document.createElement("div");
    Object.defineProperty(element, "scrollTop", { configurable: true, value: 0, writable: true });
    const controller = createScrollActivityController();
    const listener = vi.spyOn(element, "addEventListener");
    const snapshots: ReturnType<typeof controller.getSnapshot>[] = [];
    controller.subscribe(() => snapshots.push(controller.getSnapshot()));

    const detach = controller.attach(element);
    Object.defineProperty(element, "scrollTop", { configurable: true, value: 40, writable: true });
    element.dispatchEvent(new Event("scroll"));
    act(() => raf.flush());

    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]?.[2]).toMatchObject({ passive: true });
    expect(controller.getSnapshot()).toMatchObject({
      direction: "forward",
      phase: "moving",
      velocity: 0.6,
    });
    expect(snapshots.length).toBeGreaterThan(0);
    expect(now).toHaveBeenCalled();
    detach();
  });

  it("enters fast at the enter threshold and keeps hysteresis until below exit", () => {
    const raf = createRafHarness();
    const times = [0, 10, 20, 30, 40, 50, 60];
    vi.spyOn(performance, "now").mockImplementation(() => times.shift() ?? 60);
    const element = document.createElement("div");
    Object.defineProperty(element, "scrollTop", { configurable: true, value: 0, writable: true });
    const controller = createScrollActivityController();
    controller.attach(element);

    const scrollAndFlush = (offset: number) => {
      Object.defineProperty(element, "scrollTop", { configurable: true, value: offset, writable: true });
      element.dispatchEvent(new Event("scroll"));
      act(() => raf.flush());
    };

    scrollAndFlush(50);
    expect(controller.getSnapshot().velocity).toBeGreaterThanOrEqual(FAST_SCROLL_ENTER_PX_PER_MS);
    expect(controller.getSnapshot().phase).toBe("fast");
    scrollAndFlush(55);
    expect(controller.getSnapshot().velocity).toBeGreaterThan(FAST_SCROLL_EXIT_PX_PER_MS);
    expect(controller.getSnapshot().phase).toBe("fast");
    scrollAndFlush(55);
    scrollAndFlush(55);
    scrollAndFlush(55);
    expect(controller.getSnapshot().velocity).toBeLessThan(FAST_SCROLL_EXIT_PX_PER_MS);
    expect(controller.getSnapshot().phase).toBe("moving");
  });

  it("returns to idle after the last scroll and cleans all work on detach", () => {
    vi.useFakeTimers();
    const raf = createRafHarness();
    const element = document.createElement("div");
    Object.defineProperty(element, "scrollTop", { configurable: true, value: 0, writable: true });
    const controller = createScrollActivityController();
    const subscriber = vi.fn();
    controller.subscribe(subscriber);
    const detach = controller.attach(element);

    element.dispatchEvent(new Event("scroll"));
    expect(raf.callbacks.size).toBe(1);
    detach();
    expect(raf.callbacks.size).toBe(0);
    const callsAfterDetach = subscriber.mock.calls.length;
    vi.advanceTimersByTime(SCROLL_IDLE_DELAY_MS);
    expect(controller.getSnapshot()).toMatchObject({ direction: null, phase: "idle", velocity: 0 });
    expect(subscriber).toHaveBeenCalledTimes(callsAfterDetach);
  });
});
