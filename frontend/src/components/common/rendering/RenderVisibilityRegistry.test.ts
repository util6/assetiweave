/* @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from "vitest";
import { createRenderVisibilityRegistry } from "./RenderVisibilityRegistry";

type ObserverEntry = { boundingClientRect: DOMRect; isIntersecting: boolean; target: Element };

let intersectionCallbacks: Array<(entries: ObserverEntry[]) => void> = [];
let intersectionInstances: Array<{ disconnect: () => void; observe: (element: Element) => void }> = [];

function installObservers() {
  intersectionCallbacks = [];
  intersectionInstances = [];
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: (entries: ObserverEntry[]) => void) {
      intersectionCallbacks.push(callback);
      const instance = {
        disconnect: vi.fn(),
        observe: vi.fn(),
        unobserve: vi.fn(),
      };
      intersectionInstances.push(instance);
      return instance;
    }
  });
  vi.stubGlobal("ResizeObserver", class {
    observe = vi.fn();
    disconnect = vi.fn();
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("RenderVisibilityRegistry", () => {
  it("uses one shared observer and prioritizes visible and directional overscan items", () => {
    installObservers();
    const root = document.createElement("div");
    Object.defineProperty(root, "clientHeight", { configurable: true, value: 400 });
    vi.spyOn(root, "getBoundingClientRect").mockReturnValue({ top: 0, bottom: 400 } as DOMRect);
    const registry = createRenderVisibilityRegistry();
    registry.attach(root, () => "forward");
    const priorities: Array<number | null> = [];
    const visible = document.createElement("div");
    const ahead = document.createElement("div");
    const behind = document.createElement("div");
    registry.register({ element: visible, key: "visible", onPriorityChange: (value) => priorities.push(value) });
    registry.register({ element: ahead, key: "ahead", onPriorityChange: (value) => priorities.push(value) });
    registry.register({ element: behind, key: "behind", onPriorityChange: (value) => priorities.push(value) });

    expect(intersectionCallbacks).toHaveLength(1);
    expect(intersectionInstances[0]?.observe).toHaveBeenCalledTimes(3);
    intersectionCallbacks[0]?.([
      { boundingClientRect: { top: 100, bottom: 200 } as DOMRect, isIntersecting: true, target: visible },
      { boundingClientRect: { top: 450, bottom: 500 } as DOMRect, isIntersecting: true, target: ahead },
      { boundingClientRect: { top: -100, bottom: -20 } as DOMRect, isIntersecting: true, target: behind },
    ]);

    expect(priorities).toEqual([0, 1, 2]);
  });

  it("rebuilds the observer when the root size changes and unregisters cleanly", () => {
    installObservers();
    const root = document.createElement("div");
    Object.defineProperty(root, "clientHeight", { configurable: true, value: 300 });
    const registry = createRenderVisibilityRegistry();
    const detach = registry.attach(root, () => "backward");
    const onPriorityChange = vi.fn();
    const unregister = registry.register({ element: document.createElement("div"), key: "item", onPriorityChange });
    const resizeObserver = (globalThis.ResizeObserver as unknown as { prototype: { observe: unknown } });
    expect(resizeObserver).toBeTruthy();
    unregister();
    detach();
    expect(onPriorityChange).not.toHaveBeenCalled();
  });
});
