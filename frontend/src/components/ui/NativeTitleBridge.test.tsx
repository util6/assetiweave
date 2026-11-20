// @vitest-environment jsdom
import * as React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";

import {
  GlobalNativeTitleBridge,
  NATIVE_TITLE_DATA_ATTR,
  NATIVE_TITLE_EXEMPT_ATTR,
} from "./NativeTitleBridge";
import { TooltipProvider } from "./tooltip";
import { TruncatedText } from "../foundation/TruncatedText";

function renderWithBridge(ui: React.ReactNode) {
  return render(
    <TooltipProvider delayDuration={300} skipDelayDuration={150}>
      <GlobalNativeTitleBridge />
      {ui}
    </TooltipProvider>,
  );
}

describe("GlobalNativeTitleBridge", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("strips native title to data-aw-tooltip, preserves icon-only aria-label, and opens Scheme C tooltip on hover", () => {
    renderWithBridge(
      <div>
        <button type="button" title="关闭窗口" />
        <button aria-label="保留标签" type="button" title="复制路径" />
        <span title="详细状态">简略文本</span>
      </div>,
    );

    const buttons = screen.getAllByRole("button");
    const closeBtn = buttons[0];
    const copyBtn = buttons[1];
    const badgeText = screen.getByText("简略文本");

    expect(closeBtn.getAttribute("title")).toBeNull();
    expect(closeBtn.getAttribute(NATIVE_TITLE_DATA_ATTR)).toBe("关闭窗口");
    expect(closeBtn.getAttribute("aria-label")).toBe("关闭窗口");

    expect(copyBtn.getAttribute("title")).toBeNull();
    expect(copyBtn.getAttribute(NATIVE_TITLE_DATA_ATTR)).toBe("复制路径");
    expect(copyBtn.getAttribute("aria-label")).toBe("保留标签");

    expect(badgeText.getAttribute("title")).toBeNull();
    expect(badgeText.getAttribute(NATIVE_TITLE_DATA_ATTR)).toBe("详细状态");
    expect(badgeText.getAttribute("aria-label")).toBeNull();

    fireEvent.mouseOver(closeBtn);
    expect(screen.queryByRole("tooltip")).toBeNull();

    act(() => {
      vi.advanceTimersByTime(300);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.textContent).toContain("关闭窗口");
    const wrapper = tooltip.closest(".ui-tooltip");
    expect(wrapper).not.toBeNull();

    // Moving quickly to the second button reuses skipDelayDuration (0ms open)
    fireEvent.mouseOut(closeBtn, { relatedTarget: copyBtn });
    fireEvent.mouseOver(copyBtn);
    expect(screen.getByRole("tooltip").textContent).toContain("复制路径");

    // Pressing Escape dismisses immediately
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("captures dynamically updated title attributes and live-updates active tooltip", async () => {
    const { rerender } = renderWithBridge(
      <button type="button" title="复制">
        Copy
      </button>,
    );

    const btn = screen.getByRole("button", { name: "Copy" });
    fireEvent.mouseOver(btn);
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.getByRole("tooltip").textContent).toContain("复制");

    rerender(
      <TooltipProvider delayDuration={300} skipDelayDuration={150}>
        <GlobalNativeTitleBridge />
        <button type="button" title="已复制">
          Copy
        </button>
      </TooltipProvider>,
    );

    // Flush MutationObserver microtasks
    await act(async () => {
      await Promise.resolve();
    });

    expect(btn.getAttribute("title")).toBeNull();
    expect(btn.getAttribute(NATIVE_TITLE_DATA_ATTR)).toBe("已复制");
    expect(screen.getByRole("tooltip").textContent).toContain("已复制");
  });

  it("ignores exempt elements, SVG title children, and yields to overflowing TruncatedText", () => {
    const { container } = renderWithBridge(
      <div>
        <div {...{ [NATIVE_TITLE_EXEMPT_ATTR]: "" }} title="原生豁免">
          豁免区域
        </div>
        <svg viewBox="0 0 10 10">
          <title>图标说明</title>
        </svg>
        <div title="外层说明">
          <TruncatedText>这是一个会被截断的长文本内容</TruncatedText>
        </div>
      </div>,
    );

    const exempt = screen.getByText("豁免区域");
    expect(exempt.getAttribute("title")).toBe("原生豁免");
    expect(exempt.getAttribute(NATIVE_TITLE_DATA_ATTR)).toBeNull();

    expect(container.querySelector("svg > title")?.textContent).toBe(
      "图标说明",
    );

    const truncatedHost = screen.getByText("这是一个会被截断的长文本内容");
    Object.defineProperty(truncatedHost, "clientWidth", {
      configurable: true,
      get: () => 40,
    });
    Object.defineProperty(truncatedHost, "scrollWidth", {
      configurable: true,
      get: () => 200,
    });
    Object.defineProperty(truncatedHost, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(truncatedHost, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });

    fireEvent.mouseEnter(truncatedHost);
    fireEvent.mouseOver(truncatedHost);
    act(() => {
      vi.advanceTimersByTime(320);
    });

    const tooltips = screen.getAllByRole("tooltip");
    expect(tooltips).toHaveLength(1);
    expect(tooltips[0].textContent).toContain("这是一个会被截断的长文本内容");
  });
});
