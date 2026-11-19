/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { createRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DialogFrame } from "./DialogFrame";
import { resolveOpenDelay, TooltipProvider } from "../ui/tooltip";
import { TruncatedText } from "./TruncatedText";

function mockElementMetrics(
  element: HTMLElement,
  metrics: {
    clientHeight: number;
    clientWidth: number;
    scrollHeight: number;
    scrollWidth: number;
  },
) {
  Object.defineProperty(element, "clientWidth", {
    configurable: true,
    get: () => metrics.clientWidth,
  });
  Object.defineProperty(element, "scrollWidth", {
    configurable: true,
    get: () => metrics.scrollWidth,
  });
  Object.defineProperty(element, "clientHeight", {
    configurable: true,
    get: () => metrics.clientHeight,
  });
  Object.defineProperty(element, "scrollHeight", {
    configurable: true,
    get: () => metrics.scrollHeight,
  });
}

describe("TruncatedText", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("renders only the host element and no tooltip or portal when text is not overflowing (including 1px subpixel tolerance)", () => {
    render(
      <TooltipProvider>
        <TruncatedText data-testid="target">Short text</TruncatedText>
      </TooltipProvider>,
    );

    const target = screen.getByTestId("target");
    expect(target.tagName).toBe("SPAN");
    expect(target.className).toContain("block");
    expect(target.className).toContain("min-w-0");
    expect(target.className).toContain("truncate");
    expect(target.hasAttribute("tabindex")).toBe(false);

    mockElementMetrics(target, {
      clientWidth: 160,
      scrollWidth: 161,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(target);
    act(() => {
      vi.advanceTimersByTime(400);
    });

    expect(screen.queryByRole("tooltip")).toBeNull();
    expect(
      document.body.querySelector("[data-radix-popper-content-wrapper]"),
    ).toBeNull();
  });

  it("opens tooltip on first hover after 300ms delay for single-line and 2-3 line overflow, and stays open when pointer enters content", () => {
    render(
      <TooltipProvider>
        <TruncatedText data-testid="single">
          /Users/example/very/long/project/directory/path
        </TruncatedText>
        <TruncatedText as="p" data-testid="multi" lines={2}>
          Line one, line two, and a third overflowing summary line that gets
          clamped
        </TruncatedText>
      </TooltipProvider>,
    );

    const single = screen.getByTestId("single");
    mockElementMetrics(single, {
      clientWidth: 120,
      scrollWidth: 320,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(single);
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.textContent).toBe(
      "/Users/example/very/long/project/directory/path",
    );

    // Moving pointer from trigger into TooltipContent keeps it open
    fireEvent.mouseLeave(single);
    fireEvent.mouseEnter(tooltip);
    act(() => {
      vi.advanceTimersByTime(150);
    });
    expect(screen.getByRole("tooltip")).toBeTruthy();

    fireEvent.mouseLeave(tooltip);
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    // Multi-line overflow (vertical) within skip-delay window (150ms)
    const multi = screen.getByTestId("multi");
    expect(multi.tagName).toBe("P");
    expect(multi.className).toContain("line-clamp-2");
    mockElementMetrics(multi, {
      clientWidth: 200,
      scrollWidth: 200,
      clientHeight: 40,
      scrollHeight: 68,
    });

    fireEvent.mouseEnter(multi);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.getByRole("tooltip").textContent).toContain(
      "third overflowing summary line",
    );
  });

  it("re-evaluates overflow dynamically on subsequent hovers when element widens", () => {
    render(
      <TooltipProvider>
        <TruncatedText data-testid="dynamic">
          /Users/example/workspace/project-alpha
        </TruncatedText>
      </TooltipProvider>,
    );

    const target = screen.getByTestId("dynamic");
    let width = 100;
    Object.defineProperty(target, "clientWidth", {
      configurable: true,
      get: () => width,
    });
    Object.defineProperty(target, "scrollWidth", {
      configurable: true,
      get: () => 240,
    });
    Object.defineProperty(target, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(target, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });

    fireEvent.mouseEnter(target);
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(screen.getByRole("tooltip")).toBeTruthy();

    fireEvent.mouseLeave(target);
    act(() => {
      vi.advanceTimersByTime(120);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    // Column widens so text fits completely
    width = 260;
    fireEvent.mouseEnter(target);
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("does not nest buttons or open on outer button focus by default, and supports focusable keyboard interaction + Escape", () => {
    const outerRef = createRef<HTMLButtonElement>();
    render(
      <TooltipProvider>
        <button ref={outerRef} type="button">
          <TruncatedText data-testid="inside-btn">
            Very long project path inside button
          </TruncatedText>
        </button>
        <TruncatedText data-testid="focusable-text" focusable>
          Standalone focusable truncated text
        </TruncatedText>
      </TooltipProvider>,
    );

    const insideBtn = screen.getByTestId("inside-btn");
    expect(insideBtn.querySelector("button")).toBeNull();
    expect(insideBtn.closest("button")).toBe(outerRef.current);

    mockElementMetrics(insideBtn, {
      clientWidth: 80,
      scrollWidth: 240,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.focus(outerRef.current as HTMLButtonElement);
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    // Focusable variant opens on focus and closes on Escape
    const focusableEl = screen.getByTestId("focusable-text");
    expect(focusableEl.getAttribute("tabindex")).toBe("0");
    mockElementMetrics(focusableEl, {
      clientWidth: 80,
      scrollWidth: 240,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.focus(focusableEl);
    expect(screen.getByRole("tooltip").textContent).toBe(
      "Standalone focusable truncated text",
    );

    fireEvent.keyDown(focusableEl, { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("supports explicit tooltip text for rich children, tooltip={false}, inline variant, and dynamic lines toggle", () => {
    function DynamicFixture() {
      const [expanded, setExpanded] = useState(false);
      return (
        <TooltipProvider>
          <TruncatedText data-testid="rich" tooltip="Full highlighted query">
            <strong>Full</strong> highlighted query
          </TruncatedText>
          <TruncatedText data-testid="disabled" tooltip={false}>
            Disabled tooltip text
          </TruncatedText>
          <TruncatedText data-testid="inline" inline>
            Inline single-line text
          </TruncatedText>
          <TruncatedText data-testid="three-lines" lines={3}>
            Three-line clamped text
          </TruncatedText>
          <TruncatedText
            as="p"
            data-testid="collapsible"
            lines={expanded ? false : 2}
          >
            Collapsible notification message
          </TruncatedText>
          <button onClick={() => setExpanded(true)} type="button">
            Expand
          </button>
        </TooltipProvider>
      );
    }

    render(<DynamicFixture />);

    const rich = screen.getByTestId("rich");
    mockElementMetrics(rich, {
      clientWidth: 60,
      scrollWidth: 200,
      clientHeight: 20,
      scrollHeight: 20,
    });
    fireEvent.mouseEnter(rich);
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(screen.getByRole("tooltip").textContent).toBe(
      "Full highlighted query",
    );
    fireEvent.mouseLeave(rich);
    act(() => {
      vi.advanceTimersByTime(120);
    });

    const disabled = screen.getByTestId("disabled");
    mockElementMetrics(disabled, {
      clientWidth: 60,
      scrollWidth: 200,
      clientHeight: 20,
      scrollHeight: 20,
    });
    fireEvent.mouseEnter(disabled);
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    const inlineEl = screen.getByTestId("inline");
    expect(inlineEl.className).toContain("inline-block");
    expect(inlineEl.className).toContain("truncate");
    const threeLinesEl = screen.getByTestId("three-lines");
    expect(threeLinesEl.className).toContain("line-clamp-3");

    const collapsible = screen.getByTestId("collapsible");
    expect(collapsible.className).toContain("line-clamp-2");
    mockElementMetrics(collapsible, {
      clientWidth: 100,
      scrollWidth: 100,
      clientHeight: 36,
      scrollHeight: 72,
    });
    fireEvent.mouseEnter(collapsible);
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(screen.getByRole("tooltip")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Expand" }));
    expect(collapsible.className).not.toContain("line-clamp-2");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("renders above modal DialogFrame layers with pointer-events-auto", () => {
    render(
      <TooltipProvider>
        <DialogFrame
          layer="top"
          onClose={() => undefined}
          portal={false}
          title="Settings"
        >
          <TruncatedText data-testid="modal-text">
            /Users/example/very/long/modal/path/that/overflows
          </TruncatedText>
        </DialogFrame>
      </TooltipProvider>,
    );

    const modalText = screen.getByTestId("modal-text");
    mockElementMetrics(modalText, {
      clientWidth: 100,
      scrollWidth: 320,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(modalText);
    act(() => {
      vi.advanceTimersByTime(320);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.className).toContain("z-[110]");
    expect(tooltip.className).toContain("pointer-events-auto");
    expect(tooltip.className).toContain("[overflow-wrap:anywhere]");
  });

  it("resolves open delay with skipDelayDuration window and warms subsequent tooltips", () => {
    vi.setSystemTime(new Date("2026-11-18T00:00:00.000Z"));
    expect(resolveOpenDelay(null, 300, 150)).toBe(300);
    expect(resolveOpenDelay(Date.now() - 100, 300, 150)).toBe(0);
    expect(resolveOpenDelay(Date.now() - 200, 300, 150)).toBe(300);
    expect(resolveOpenDelay(Date.now() - 50, 300, 0)).toBe(300);

    render(
      <TooltipProvider delayDuration={300} skipDelayDuration={150}>
        <TruncatedText data-testid="first">
          First overflowing path text
        </TruncatedText>
        <TruncatedText data-testid="second">
          Second overflowing path text
        </TruncatedText>
      </TooltipProvider>,
    );

    const first = screen.getByTestId("first");
    const second = screen.getByTestId("second");
    mockElementMetrics(first, {
      clientWidth: 80,
      scrollWidth: 240,
      clientHeight: 20,
      scrollHeight: 20,
    });
    mockElementMetrics(second, {
      clientWidth: 80,
      scrollWidth: 240,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(first);
    act(() => {
      vi.advanceTimersByTime(310);
    });
    expect(screen.getByRole("tooltip").textContent).toBe(
      "First overflowing path text",
    );

    fireEvent.mouseLeave(first);
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();

    // Within 150ms skipDelayDuration window, second tooltip opens immediately
    fireEvent.mouseEnter(second);
    expect(screen.getByRole("tooltip").textContent).toBe(
      "Second overflowing path text",
    );
  });

  it("portals its anchor into document.body outside contain/transform containers and renders Scheme C glass tooltip structure", () => {
    render(
      <TooltipProvider>
        <div
          className="deferred-render-boundary"
          data-testid="contained-wrapper"
          style={{
            contain: "layout paint style",
            transform: "translateY(-1px)",
          }}
        >
          <TruncatedText data-testid="contained-target">
            ~/Documents/very/long/path/inside/contained/transform/parent
          </TruncatedText>
        </div>
      </TooltipProvider>,
    );

    const wrapper = screen.getByTestId("contained-wrapper");
    const target = screen.getByTestId("contained-target");
    mockElementMetrics(target, {
      clientWidth: 140,
      scrollWidth: 420,
      clientHeight: 20,
      scrollHeight: 20,
    });
    target.getBoundingClientRect = () =>
      ({
        top: 240,
        left: 88,
        right: 228,
        bottom: 260,
        width: 140,
        height: 20,
        x: 88,
        y: 240,
        toJSON: () => ({}),
      }) as DOMRect;

    fireEvent.mouseEnter(target);
    act(() => {
      vi.advanceTimersByTime(310);
    });

    // Anchor must live directly in document.body rather than inside contain/transform wrapper
    const anchor = document.body.querySelector(
      "[data-truncated-tooltip-anchor]",
    ) as HTMLElement | null;
    expect(anchor).not.toBeNull();
    expect(wrapper.contains(anchor)).toBe(false);
    expect(anchor?.style.top).toBe("240px");
    expect(anchor?.style.left).toBe("88px");

    // Scheme C tooltip structure: outer .ui-tooltip (overflow visible) + inner .ui-tooltip-scroll + .ui-tooltip-arrow
    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.className).toContain("ui-tooltip");
    expect(tooltip.getAttribute("data-align")).toBe("start");
    expect(tooltip.querySelector(".ui-tooltip-scroll")?.textContent).toBe(
      "~/Documents/very/long/path/inside/contained/transform/parent",
    );
    expect(tooltip.querySelector(".ui-tooltip-arrow")).not.toBeNull();
  });
});
