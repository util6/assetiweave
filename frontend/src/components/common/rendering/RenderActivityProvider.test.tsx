/* @vitest-environment jsdom */

import { act, render, screen } from "@testing-library/react";
import { StrictMode, useEffect, useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RenderActivityProvider, useScrollActivitySnapshot } from "./RenderActivityProvider";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function SnapshotReader() {
  const snapshot = useScrollActivitySnapshot();
  return <output data-phase={snapshot.phase}>{snapshot.direction ?? "none"}</output>;
}

describe("RenderActivityProvider", () => {
  it("attaches once under StrictMode and publishes data-scroll-phase", () => {
    const rafCallbacks = new Map<number, FrameRequestCallback>();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = rafCallbacks.size + 1;
      rafCallbacks.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => rafCallbacks.delete(id));
    const surface = document.createElement("div");
    document.body.append(surface);
    const ref = { current: surface } as React.MutableRefObject<HTMLDivElement | null>;
    const addEventListener = vi.spyOn(surface, "addEventListener");

    render(
      <StrictMode>
        <RenderActivityProvider scrollElementRef={ref}>
          <SnapshotReader />
        </RenderActivityProvider>
      </StrictMode>,
    );
    expect(surface.getAttribute("data-scroll-phase")).toBe("idle");
    const scrollListeners = addEventListener.mock.calls.filter(([type]) => type === "scroll");
    expect(scrollListeners).toHaveLength(2);

    Object.defineProperty(surface, "scrollTop", { configurable: true, value: 100 });
    act(() => {
      surface.dispatchEvent(new Event("scroll"));
      [...rafCallbacks.values()].forEach((callback) => callback(16));
      rafCallbacks.clear();
    });
    expect(surface.getAttribute("data-scroll-phase")).toBe("fast");
  });

  it("disposes the controller when the provider unmounts", () => {
    const ref = { current: null } as React.MutableRefObject<HTMLDivElement | null>;
    const cancelAnimationFrame = vi.fn();
    vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    }));
    vi.stubGlobal("cancelAnimationFrame", cancelAnimationFrame);
    const { unmount } = render(
      <RenderActivityProvider scrollElementRef={ref}>
        <div ref={ref}>content</div>
      </RenderActivityProvider>,
    );
    const surface = screen.getByText("content");
    surface.dispatchEvent(new Event("scroll"));
    unmount();
    expect(cancelAnimationFrame).toHaveBeenCalled();
  });
});
