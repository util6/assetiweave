/** @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ResizableColumns,
  calculateScrollThumb,
  resolveColumnMinWidths,
} from "./ResizableColumns";
import { sanitizeColumnWeights } from "./columnLayouts";

const mockSetColumnLayoutAsync = vi.fn(async () => {});
const mockSettings = {
  columnLayouts: {} as Record<string, number[]>,
};

vi.mock("../../store/settings/useAppSettings", () => ({
  useAppSettings: () => ({
    setColumnLayout: vi.fn(),
    setColumnLayoutAsync: mockSetColumnLayoutAsync,
    settings: mockSettings,
    settingsLoaded: true,
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();

  class MockResizeObserver {
    callback: ResizeObserverCallback;
    constructor(callback: ResizeObserverCallback) {
      this.callback = callback;
    }
    observe(target: Element) {
      this.callback(
        [
          {
            target,
            borderBoxSize: [{ inlineSize: 900, blockSize: 600 }],
            contentBoxSize: [{ inlineSize: 900, blockSize: 600 }],
            contentRect: {
              bottom: 600,
              height: 600,
              left: 0,
              right: 900,
              top: 0,
              width: 900,
              x: 0,
              y: 0,
            } as DOMRectReadOnly,
          } as unknown as ResizeObserverEntry,
        ],
        this as unknown as ResizeObserver,
      );
    }
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", MockResizeObserver);
  window.ResizeObserver =
    MockResizeObserver as unknown as typeof ResizeObserver;

  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    bottom: 600,
    height: 600,
    left: 0,
    right: 900,
    top: 0,
    width: 900,
    x: 0,
    y: 0,
    toJSON: () => {},
  });

  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    get() {
      return 900;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get() {
      return 600;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get() {
      return 900;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get() {
      return 600;
    },
  });
});

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}

describe("ResizableColumns", () => {
  it("renders accessible native separators and scroll controls between columns", () => {
    const { container } = render(
      <ResizableColumns
        ariaLabel="Resize columns"
        className="min-h-40"
        columns={[
          { defaultWeight: 0.72 },
          { defaultWeight: 1.14, minWidthScale: 1.1 },
          { defaultWeight: 1.4, minWidthScale: 1.45 },
        ]}
        minimumWidth={280}
        scrollBarLabel="Scroll columns"
        scrollLeftLabel="Scroll columns left"
        scrollRightLabel="Scroll columns right"
      >
        <section>Sources</section>
        <section>Skills</section>
        <section>Mount targets</section>
      </ResizableColumns>,
      { wrapper: createWrapper() },
    );

    const separators = screen.getAllByRole("separator");
    expect(separators.length).toBe(2);
    expect(separators[0].getAttribute("aria-label")).toBe("Resize columns 1");
    expect(separators[1].getAttribute("aria-label")).toBe("Resize columns 2");

    expect(screen.getByRole("scrollbar")).toBeDefined();
    expect(screen.getByLabelText("Scroll columns left")).toBeDefined();
    expect(screen.getByLabelText("Scroll columns right")).toBeDefined();

    const scrollControls = container.querySelector(
      "[data-resizable-columns-scroll-controls]",
    );
    expect(scrollControls).not.toBeNull();
    expect(scrollControls?.className).toContain("sticky bottom-0");

    const html = container.innerHTML;
    expect(html).toContain("--resizable-columns-min-width");
    expect(html).toContain("--resizable-columns-width");
    expect(html).toContain("w-[max(100%,var(--resizable-columns-width))]");
  });

  it("supports native keyboard resizing on separators", () => {
    render(
      <ResizableColumns
        ariaLabel="Resize columns"
        columns={[{ defaultWeight: 1 }, { defaultWeight: 1 }]}
        minimumWidth={200}
        scrollBarLabel="Scroll columns"
        scrollLeftLabel="Scroll columns left"
        scrollRightLabel="Scroll columns right"
      >
        <div>Left</div>
        <div>Right</div>
      </ResizableColumns>,
      { wrapper: createWrapper() },
    );

    const separators = screen.getAllByRole("separator");
    expect(separators.length).toBeGreaterThan(0);
    const separator = separators[0];

    // react-resizable-panels 原生 Separator 响应方向键
    fireEvent.keyDown(separator, { key: "ArrowLeft" });
    fireEvent.keyDown(separator, { key: "ArrowRight" });
  });

  it("migrates legacy localStorage weights to settings and removes the old key", async () => {
    const storageKey = "test_columns_pref";
    localStorage.setItem(storageKey, JSON.stringify([0.5, 1.5]));

    render(
      <ResizableColumns
        ariaLabel="Resize columns"
        columns={[{ defaultWeight: 1 }, { defaultWeight: 1 }]}
        minimumWidth={200}
        scrollBarLabel="Scroll columns"
        scrollLeftLabel="Scroll columns left"
        scrollRightLabel="Scroll columns right"
        storageKey={storageKey}
      >
        <div>Left</div>
        <div>Right</div>
      </ResizableColumns>,
      { wrapper: createWrapper() },
    );

    await waitFor(() => {
      expect(mockSetColumnLayoutAsync).toHaveBeenCalledWith(
        storageKey,
        [0.5, 1.5],
      );
    });

    expect(localStorage.getItem(storageKey)).toBeNull();
  });

  it("sanitizes persisted weights before using them", () => {
    expect(sanitizeColumnWeights([2, 1, 1], [1, 1, 1])).toEqual([
      1.5, 0.75, 0.75,
    ]);
    expect(sanitizeColumnWeights([2, 0, 1], [1, 1, 1])).toEqual([1, 1, 1]);
    expect(sanitizeColumnWeights([2, 1], [1, 1, 1])).toEqual([1, 1, 1]);
  });

  it("rescales persisted weights to the default weight total", () => {
    const sanitizedWeights = sanitizeColumnWeights(
      [0.3244, 0.2351, 0.4405],
      [0.72, 0.9, 1.45],
    );

    expect(
      Number(
        sanitizedWeights.reduce((sum, weight) => sum + weight, 0).toFixed(2),
      ),
    ).toBe(3.07);
  });

  it("scales each column minimum width from the global setting", () => {
    expect(
      resolveColumnMinWidths(280, [
        { defaultWeight: 0.72 },
        { defaultWeight: 1.14, minWidthScale: 1.1 },
        { defaultWeight: 1.4, minWidthScale: 1.45 },
      ]),
    ).toEqual([280, 308, 406]);
  });

  it("calculates a mac-style scrollbar thumb from viewport metrics", () => {
    expect(
      calculateScrollThumb({
        clientWidth: 900,
        scrollLeft: 300,
        scrollWidth: 1500,
      }),
    ).toEqual({
      leftRatio: 0.5,
      widthRatio: 0.6,
    });
  });
});
