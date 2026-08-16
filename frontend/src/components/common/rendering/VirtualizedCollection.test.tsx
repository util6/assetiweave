/* @vitest-environment jsdom */

import { act, cleanup, render, screen } from "@testing-library/react";
import { createRef, type MutableRefObject } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RenderActivityProvider } from "./RenderActivityProvider";
import { VirtualizedCollection, overscanForPhase, type VirtualizedCollectionHandle } from "./VirtualizedCollection";

const items = Array.from({ length: 30 }, (_, index) => ({ id: `item-${index + 1}`, label: `Item ${index + 1}` }));

function setupDom() {
  const scrollElement = document.createElement("div");
  Object.defineProperty(scrollElement, "clientHeight", { configurable: true, value: 240 });
  Object.defineProperty(scrollElement, "offsetHeight", { configurable: true, value: 240 });
  Object.defineProperty(scrollElement, "offsetWidth", { configurable: true, value: 800 });
  Object.defineProperty(scrollElement, "scrollTop", { configurable: true, value: 0, writable: true });
  vi.spyOn(scrollElement, "getBoundingClientRect").mockReturnValue({ top: 0, bottom: 240, height: 240 } as DOMRect);
  const scrollElementRef = { current: scrollElement } as MutableRefObject<HTMLDivElement | null>;
  document.body.append(scrollElement);
  const rafCallbacks = new Map<number, FrameRequestCallback>();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = rafCallbacks.size + 1;
    rafCallbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => rafCallbacks.delete(id));
  vi.stubGlobal("ResizeObserver", class {
    constructor(private readonly callback: ResizeObserverCallback) {}
    observe(target: Element) {
      this.callback([{ target, contentRect: { width: 800, height: 240 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  });
  const flush = () => act(() => {
    const callbacks = [...rafCallbacks.values()];
    rafCallbacks.clear();
    callbacks.forEach((callback) => callback(16));
  });
  return { flush, rafCallbacks, scrollElement, scrollElementRef };
}

function renderCollection({
  collectionItems = items,
  eagerKeys,
  enabled = true,
  minItems = 12,
  pinnedKeys,
  ref,
}: {
  collectionItems?: typeof items;
  eagerKeys?: ReadonlySet<string>;
  enabled?: boolean;
  minItems?: number;
  pinnedKeys?: ReadonlySet<string>;
  ref?: React.Ref<VirtualizedCollectionHandle>;
} = {}) {
  const { flush, rafCallbacks, scrollElement, scrollElementRef } = setupDom();
  render(
    <RenderActivityProvider scrollElementRef={scrollElementRef}>
      <VirtualizedCollection
        eagerKeys={eagerKeys}
        enabled={enabled}
        getItemKey={(item) => item.id}
        items={collectionItems}
        minItems={minItems}
        pinnedKeys={pinnedKeys}
        ref={ref}
        renderItem={(item) => <span>{item.label}</span>}
        scrollElementRef={scrollElementRef}
      />
    </RenderActivityProvider>,
  );
  flush();
  return { flush, rafCallbacks, scrollElement, scrollElementRef };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("VirtualizedCollection", () => {
  it("uses coarse overscan values for each scroll phase", () => {
    expect(overscanForPhase("idle")).toBe(3);
    expect(overscanForPhase("moving")).toBe(5);
    expect(overscanForPhase("fast")).toBe(8);
  });

  it("renders a short collection in normal flow with stable keys", () => {
    renderCollection({ collectionItems: items.slice(0, 3) });
    expect(document.querySelectorAll("[data-virtual-item-key]")).toHaveLength(3);
    expect(document.querySelectorAll("[data-render-state=\"skeleton\"]")).toHaveLength(3);
  });

  it("bounds long collection mounts and supports pinned/eager items and imperative navigation", () => {
    const handle = createRef<VirtualizedCollectionHandle>();
    renderCollection({
      eagerKeys: new Set(["item-30"]),
      pinnedKeys: new Set(["item-29"]),
      ref: handle,
    });
    const mounted = document.querySelectorAll("[data-virtual-item-key]");
    expect(mounted.length).toBeLessThan(items.length);
    expect(document.querySelector("[data-virtual-item-key=\"item-29\"]")).toBeTruthy();
    expect(document.querySelector("[data-virtual-item-key=\"item-30\"]")).toBeTruthy();
    expect(document.querySelector("[data-virtual-item-key=\"item-30\"] [data-render-state]")?.getAttribute("data-render-state")).toBe("ready");
    expect(document.querySelector("[data-virtual-item-key=\"item-1\"]")?.getAttribute("aria-posinset")).toBe("1");
    expect(handle.current?.scrollToKey("item-30", { behavior: "auto" })).toBe(true);
    expect(handle.current?.scrollToKey("missing")).toBe(false);
    handle.current?.measure();
  });

  it("rejects duplicate and empty business keys and can disable virtualization", () => {
    expect(() => renderCollection({
      collectionItems: [{ id: "same", label: "a" }, { id: "same", label: "b" }],
      enabled: false,
    })).toThrow(/duplicate/i);
    cleanup();
    expect(() => renderCollection({
      collectionItems: [{ id: "", label: "empty" }],
      enabled: false,
    })).toThrow(/non-empty/i);
  });
});
