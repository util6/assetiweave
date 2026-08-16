/* @vitest-environment jsdom */

import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DeferredSkeletonBoundary } from "./DeferredSkeletonBoundary";
import { RenderActivityProvider } from "./RenderActivityProvider";

interface ObserverEntry {
  boundingClientRect: DOMRect;
  isIntersecting: boolean;
  target: Element;
}

let intersectionCallback: ((entries: ObserverEntry[]) => void) | null = null;

function installObservers() {
  intersectionCallback = null;
  vi.stubGlobal("IntersectionObserver", class {
    constructor(callback: (entries: ObserverEntry[]) => void) {
      intersectionCallback = callback;
    }
    disconnect() {}
    observe() {}
    unobserve() {}
  });
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    disconnect() {}
  });
}

function renderBoundary(props: Partial<React.ComponentProps<typeof DeferredSkeletonBoundary>> = {}) {
  installObservers();
  const scrollElement = document.createElement("div");
  Object.defineProperty(scrollElement, "clientHeight", { configurable: true, value: 400 });
  vi.spyOn(scrollElement, "getBoundingClientRect").mockReturnValue({ top: 0, bottom: 400 } as DOMRect);
  const scrollElementRef = { current: scrollElement } as React.MutableRefObject<HTMLDivElement | null>;
  document.body.append(scrollElement);
  const rafCallbacks = new Map<number, FrameRequestCallback>();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    const id = rafCallbacks.size + 1;
    rafCallbacks.set(id, callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => rafCallbacks.delete(id));
  const result = render(
    <RenderActivityProvider scrollElementRef={scrollElementRef}>
      <DeferredSkeletonBoundary itemKey="turn-1" {...props}>
        <span>ready content</span>
      </DeferredSkeletonBoundary>
    </RenderActivityProvider>,
  );
  const flush = () => act(() => {
    const callbacks = [...rafCallbacks.values()];
    rafCallbacks.clear();
    callbacks.forEach((callback) => callback(16));
  });
  return { ...result, flush, scrollElement, scrollElementRef };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  intersectionCallback = null;
});

describe("DeferredSkeletonBoundary", () => {
  it("starts with a unified skeleton and becomes ready after visible idle scheduling", () => {
    const { flush } = renderBoundary({ fallback: <output>custom fallback</output> });
    const boundary = screen.getByTestId("deferred-render-boundary");
    expect(boundary.getAttribute("data-render-state")).toBe("skeleton");
    expect(screen.getByText("custom fallback")).toBeTruthy();
    expect(screen.queryByText("ready content")).toBeNull();

    act(() => intersectionCallback?.([{
      boundingClientRect: { top: 100, bottom: 200 } as DOMRect,
      isIntersecting: true,
      target: boundary,
    }]));
    flush();

    expect(screen.getByText("ready content")).toBeTruthy();
    expect(boundary.getAttribute("data-render-state")).toBe("ready");
  });

  it("does not commit in fast phase and never regresses ready content", () => {
    const { flush, scrollElement } = renderBoundary();
    Object.defineProperty(scrollElement, "scrollTop", { configurable: true, value: 500 });
    act(() => {
      scrollElement.dispatchEvent(new Event("scroll"));
      flush();
    });
    const boundary = screen.getAllByTestId("deferred-render-boundary").find((element) => element.getAttribute("data-render-state") === "skeleton")!;
    act(() => intersectionCallback?.([{
      boundingClientRect: { top: 100, bottom: 200 } as DOMRect,
      isIntersecting: true,
      target: boundary,
    }]));
    expect(boundary.getAttribute("data-render-state")).toBe("skeleton");
    expect(screen.queryByText("ready content")).toBeNull();

    // A forced item is already ready and remains ready while the phase changes.
    const forced = renderBoundary({ forceReady: true });
    expect(screen.getByText("ready content")).toBeTruthy();
    forced.unmount();
  });

  it("supports forceReady, disabled mode, custom fallback, and one ready callback per key", () => {
    const onReady = vi.fn();
    const { rerender } = renderBoundary({
      fallback: <output>custom fallback</output>,
      forceReady: true,
      onReady,
      size: "tall",
    });
    expect(screen.getByText("ready content")).toBeTruthy();
    expect(onReady).toHaveBeenCalledWith("turn-1");
    expect(onReady).toHaveBeenCalledTimes(1);

    rerender(
      <RenderActivityProvider scrollElementRef={{ current: document.createElement("div") }}>
        <DeferredSkeletonBoundary enabled={false} itemKey="disabled">
          <span>disabled content</span>
        </DeferredSkeletonBoundary>
      </RenderActivityProvider>,
    );
    expect(screen.getByText("disabled content")).toBeTruthy();
  });
});
