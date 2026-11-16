/* @vitest-environment jsdom */

import { act, cleanup, render } from "@testing-library/react";
import { createRef, type MutableRefObject } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RenderActivityProvider } from "./RenderActivityProvider";
import {
  VirtualizedCollection,
  overscanForPhase,
  type VirtualizedCollectionHandle,
} from "./VirtualizedCollection";

const items = Array.from({ length: 30 }, (_, index) => ({
  id: `item-${index + 1}`,
  label: `Item ${index + 1}`,
}));

interface ResizeObserverStub {
  callback: ResizeObserverCallback;
  targets: Element[];
}

function stubRenderGlobals() {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(224);
  const rafCallbacks = new Map<number, FrameRequestCallback>();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = rafCallbacks.size + 1;
    rafCallbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) =>
    rafCallbacks.delete(id),
  );
  const resizeObservers: ResizeObserverStub[] = [];
  vi.stubGlobal(
    "ResizeObserver",
    class {
      readonly stub: ResizeObserverStub;
      constructor(callback: ResizeObserverCallback) {
        this.stub = { callback, targets: [] };
        resizeObservers.push(this.stub);
      }
      observe(target: Element) {
        this.stub.targets.push(target);
        this.stub.callback(
          [
            {
              target,
              contentRect: {
                width: (target as HTMLElement).clientWidth,
                height: (target as HTMLElement).clientHeight,
              },
            } as ResizeObserverEntry,
          ],
          this as unknown as ResizeObserver,
        );
      }
      unobserve() {}
      disconnect() {}
    },
  );
  const flush = () =>
    act(() => {
      const callbacks = [...rafCallbacks.values()];
      rafCallbacks.clear();
      callbacks.forEach((callback) => callback(16));
    });
  const notifyResize = () =>
    act(() => {
      for (const stub of resizeObservers) {
        stub.callback(
          stub.targets.map(
            (target) =>
              ({
                target,
                contentRect: {
                  width: (target as HTMLElement).clientWidth,
                  height: (target as HTMLElement).clientHeight,
                },
              }) as ResizeObserverEntry,
          ),
          {} as ResizeObserver,
        );
      }
    });
  return { flush, notifyResize, rafCallbacks, resizeObservers };
}

function setupDom({ clientHeight = 240, clientWidth = 800 } = {}) {
  const { flush, notifyResize, rafCallbacks, resizeObservers } =
    stubRenderGlobals();
  const scrollElement = document.createElement("div");
  let measuredHeight = clientHeight;
  Object.defineProperty(scrollElement, "clientHeight", {
    configurable: true,
    get: () => measuredHeight,
  });
  Object.defineProperty(scrollElement, "offsetHeight", {
    configurable: true,
    get: () => measuredHeight,
  });
  Object.defineProperty(scrollElement, "clientWidth", {
    configurable: true,
    value: clientWidth,
  });
  Object.defineProperty(scrollElement, "offsetWidth", {
    configurable: true,
    value: clientWidth,
  });
  Object.defineProperty(scrollElement, "scrollTop", {
    configurable: true,
    value: 0,
    writable: true,
  });
  vi.spyOn(scrollElement, "getBoundingClientRect").mockReturnValue({
    top: 0,
    bottom: 240,
    height: 240,
  } as DOMRect);
  const scrollElementRef = {
    current: scrollElement,
  } as MutableRefObject<HTMLDivElement | null>;
  document.body.append(scrollElement);
  const resize = (height: number) => {
    measuredHeight = height;
    notifyResize();
  };
  return {
    flush,
    notifyResize,
    rafCallbacks,
    resize,
    resizeObservers,
    scrollElement,
    scrollElementRef,
  };
}

function collectionElement({
  collectionItems = items,
  eagerKeys,
  enabled = true,
  minItems = 12,
  pinnedKeys,
  ref,
  scrollElementRef,
}: {
  collectionItems?: typeof items;
  eagerKeys?: ReadonlySet<string>;
  enabled?: boolean;
  minItems?: number;
  pinnedKeys?: ReadonlySet<string>;
  ref?: React.Ref<VirtualizedCollectionHandle>;
  scrollElementRef: MutableRefObject<HTMLDivElement | null>;
}) {
  return (
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
    </RenderActivityProvider>
  );
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
    collectionElement({
      collectionItems,
      eagerKeys,
      enabled,
      minItems,
      pinnedKeys,
      ref,
      scrollElementRef,
    }),
  );
  flush();
  return { flush, rafCallbacks, scrollElement, scrollElementRef };
}

function virtualItemIndices(): number[] {
  return [...document.querySelectorAll("[data-virtual-item-key]")].map(
    (element) => Number(element.getAttribute("data-index")),
  );
}

function collectionTotalHeight(): number {
  const container = document.querySelector<HTMLElement>(
    "[data-virtualized-collection]",
  );
  return Number.parseFloat(container?.style.height ?? "0");
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("VirtualizedCollection", () => {
  it("commits destination row shells before the scroll callback returns", () => {
    const { scrollElement } = renderCollection();
    act(() => {
      scrollElement.scrollTop = 4000;
      scrollElement.dispatchEvent(new Event("scroll"));
      const indices = [
        ...document.querySelectorAll("[data-virtual-item-key]"),
      ].map((element) => Number(element.getAttribute("data-index")));
      // Assert inside the batch: act's eventual flush must not hide a blank frame.
      expect(Math.max(...indices)).toBeGreaterThan(12);
      expect(Math.min(...indices)).toBeGreaterThan(0);
    });
  });

  it("renders row shells when the scroll element ref binds after the first render", async () => {
    const { flush, scrollElement } = setupDom();
    const scrollElementRef = {
      current: null,
    } as MutableRefObject<HTMLDivElement | null>;
    const handle = createRef<VirtualizedCollectionHandle>();
    const { rerender } = render(
      collectionElement({ ref: handle, scrollElementRef }),
    );
    flush();
    expect(virtualItemIndices()).toHaveLength(0);

    scrollElementRef.current = scrollElement;
    rerender(collectionElement({ ref: handle, scrollElementRef }));
    await act(async () => {});

    expect(collectionTotalHeight()).toBeGreaterThan(0);
    expect(virtualItemIndices().length).toBeGreaterThan(0);
    expect(handle.current?.scrollToKey("item-30")).toBe(true);
  });

  it("renders row shells in the first frame when the parent ref binds after child layout effects", async () => {
    stubRenderGlobals();
    const scrollElementRef = {
      current: null,
    } as MutableRefObject<HTMLDivElement | null>;
    const handle = createRef<VirtualizedCollectionHandle>();

    function Harness() {
      return (
        <div
          ref={(node) => {
            scrollElementRef.current = node;
            if (!node) return;
            Object.defineProperty(node, "clientHeight", {
              configurable: true,
              value: 480,
            });
            Object.defineProperty(node, "offsetHeight", {
              configurable: true,
              value: 480,
            });
            Object.defineProperty(node, "clientWidth", {
              configurable: true,
              value: 800,
            });
            node.scrollTop = 4000;
          }}
        >
          <RenderActivityProvider scrollElementRef={scrollElementRef}>
            <VirtualizedCollection
              getItemKey={(item) => item.id}
              items={items}
              ref={handle}
              renderItem={(item) => <span>{item.label}</span>}
              scrollElementRef={scrollElementRef}
            />
          </RenderActivityProvider>
        </div>
      );
    }

    render(<Harness />);
    await act(async () => {});

    expect(collectionTotalHeight()).toBeGreaterThan(0);
    const indices = virtualItemIndices();
    expect(indices.length).toBeGreaterThan(0);
    // The retained scroll offset is restored, so the viewport is not at the top.
    expect(Math.max(...indices)).toBeGreaterThan(12);
    expect(handle.current?.scrollToKey("item-1", { align: "start" })).toBe(
      true,
    );
  });

  it("restores row shells when the viewport recovers from zero height", () => {
    const { flush, resize, scrollElementRef } = setupDom({
      clientHeight: 0,
    });
    render(collectionElement({ scrollElementRef }));
    flush();
    expect(collectionTotalHeight()).toBeGreaterThan(0);
    expect(virtualItemIndices()).toHaveLength(0);

    resize(480);

    expect(collectionTotalHeight()).toBeGreaterThan(0);
    expect(virtualItemIndices().length).toBeGreaterThan(0);
  });

  it("uses coarse overscan values for each scroll phase", () => {
    expect(overscanForPhase("idle")).toBe(3);
    expect(overscanForPhase("moving")).toBe(5);
    expect(overscanForPhase("fast")).toBe(8);
  });

  it("renders a short collection in normal flow with stable keys", () => {
    renderCollection({ collectionItems: items.slice(0, 3) });
    expect(document.querySelectorAll("[data-virtual-item-key]")).toHaveLength(
      3,
    );
    expect(
      document.querySelectorAll('[data-render-state="skeleton"]'),
    ).toHaveLength(3);
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
    expect(
      document.querySelector('[data-virtual-item-key="item-29"]'),
    ).toBeTruthy();
    expect(
      document.querySelector('[data-virtual-item-key="item-30"]'),
    ).toBeTruthy();
    expect(
      document
        .querySelector('[data-virtual-item-key="item-30"] [data-render-state]')
        ?.getAttribute("data-render-state"),
    ).toBe("ready");
    expect(
      document
        .querySelector('[data-virtual-item-key="item-1"]')
        ?.getAttribute("aria-posinset"),
    ).toBe("1");
    expect(handle.current?.scrollToKey("item-30", { behavior: "auto" })).toBe(
      true,
    );
    expect(handle.current?.scrollToKey("missing")).toBe(false);
    handle.current?.measure();
  });

  it("rejects duplicate and empty business keys and can disable virtualization", () => {
    expect(() =>
      renderCollection({
        collectionItems: [
          { id: "same", label: "a" },
          { id: "same", label: "b" },
        ],
        enabled: false,
      }),
    ).toThrow(/duplicate/i);
    cleanup();
    renderCollection({
      collectionItems: items.slice(0, 2),
      enabled: false,
    });
    expect(
      document.querySelectorAll('[data-render-state="ready"]'),
    ).toHaveLength(2);
    cleanup();
    expect(() =>
      renderCollection({
        collectionItems: [{ id: "", label: "empty" }],
        enabled: false,
      }),
    ).toThrow(/non-empty/i);
  });
});
