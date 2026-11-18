/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "../ui/tooltip";
import { SimpleSelect } from "./select";

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

describe("SimpleSelect component (built with @radix-ui/react-select)", () => {
  beforeAll(() => {
    window.HTMLElement.prototype.hasPointerCapture = vi.fn();
    window.HTMLElement.prototype.setPointerCapture = vi.fn();
    window.HTMLElement.prototype.releasePointerCapture = vi.fn();
    window.HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  const options = [
    { label: "系统字体", value: "system" },
    { label: "JetBrains Mono", value: "jetbrains" },
    { label: "衬线字体", value: "serif" },
  ];

  it("renders trigger with selected option label", () => {
    render(
      <SimpleSelect
        ariaLabel="选择字体"
        onChange={vi.fn()}
        options={options}
        value="jetbrains"
      />,
    );

    const combobox = screen.getByRole("combobox", { name: "选择字体" });
    expect(combobox.textContent).toContain("JetBrains Mono");
  });

  it("renders placeholder when value is empty or not matched", () => {
    render(
      <SimpleSelect
        ariaLabel="选择字体"
        onChange={vi.fn()}
        options={options}
        placeholder="请选择字体"
        value=""
      />,
    );

    const combobox = screen.getByRole("combobox", { name: "选择字体" });
    expect(combobox.textContent).toContain("请选择字体");
  });

  it("disables combobox trigger when disabled prop is true", () => {
    render(
      <SimpleSelect
        ariaLabel="选择字体"
        disabled
        onChange={vi.fn()}
        options={options}
        value="system"
      />,
    );

    const combobox = screen.getByRole("combobox", { name: "选择字体" });
    expect(combobox.hasAttribute("disabled")).toBe(true);
  });

  it("calls onChange when selecting another option", () => {
    const handleChange = vi.fn();
    render(
      <SimpleSelect
        ariaLabel="选择字体"
        onChange={handleChange}
        options={options}
        value="system"
      />,
    );

    const combobox = screen.getByRole("combobox", { name: "选择字体" });
    fireEvent.keyDown(combobox, { key: "ArrowDown" });
    const option = screen.getByRole("option", { name: "JetBrains Mono" });
    fireEvent.click(option);

    expect(handleChange).toHaveBeenCalledWith("jetbrains");
  });

  it("updates rendered label when controlled value changes", () => {
    const { rerender } = render(
      <SimpleSelect
        ariaLabel="选择字体"
        onChange={vi.fn()}
        options={options}
        value="system"
      />,
    );

    expect(
      screen.getByRole("combobox", { name: "选择字体" }).textContent,
    ).toContain("系统字体");

    rerender(
      <SimpleSelect
        ariaLabel="选择字体"
        onChange={vi.fn()}
        options={options}
        value="serif"
      />,
    );

    expect(
      screen.getByRole("combobox", { name: "选择字体" }).textContent,
    ).toContain("衬线字体");
  });

  it("automatically closes other open selects when a new select is opened", () => {
    render(
      <div>
        <SimpleSelect
          ariaLabel="字体 A"
          id="select-a"
          onChange={vi.fn()}
          options={options}
          value="system"
        />
        <SimpleSelect
          ariaLabel="字体 B"
          id="select-b"
          onChange={vi.fn()}
          options={options}
          value="jetbrains"
        />
      </div>,
    );

    const comboboxA = screen.getByRole("combobox", { name: "字体 A" });
    const comboboxB = screen.getByRole("combobox", { name: "字体 B" });

    // 打开 A
    fireEvent.keyDown(comboboxA, { key: "ArrowDown" });
    expect(screen.getAllByRole("option").length).toBeGreaterThan(0);

    // 直接打开 B
    fireEvent.keyDown(comboboxB, { key: "ArrowDown" });

    // 选项列表应该只来自当前活跃的下拉框，不应出现两个下拉框同时重叠展开
    const visibleOptions = screen.getAllByRole("option");
    expect(visibleOptions.length).toBe(options.length);
  });

  it("shows tooltip on hover when SimpleSelect selected value overflows trigger", () => {
    vi.useFakeTimers();
    const longLabel =
      "assetiweave-very-long-session-log-filename-2026-11-18.log";

    render(
      <TooltipProvider>
        <SimpleSelect
          ariaLabel="选择日志文件"
          onChange={vi.fn()}
          options={[{ label: longLabel, value: "long-log" }]}
          value="long-log"
        />
      </TooltipProvider>,
    );

    const combobox = screen.getByRole("combobox", { name: "选择日志文件" });
    expect(combobox.className).not.toContain("[&>span]:line-clamp-1");

    const triggerLabel = within(combobox).getByText(longLabel);
    expect(triggerLabel.className).toContain("truncate");
    expect(triggerLabel.className).toContain("pointer-events-auto");

    mockElementMetrics(triggerLabel, {
      clientWidth: 120,
      scrollWidth: 320,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(triggerLabel);
    act(() => {
      vi.advanceTimersByTime(320);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.textContent).toBe(longLabel);
  });

  it("shows tooltip on hover when SimpleSelect placeholder overflows trigger", () => {
    vi.useFakeTimers();
    const longPlaceholder =
      "请选择一个非常非常长的本地日志文件路径以便进行过滤与查看";

    render(
      <TooltipProvider>
        <SimpleSelect
          ariaLabel="选择日志文件"
          onChange={vi.fn()}
          options={options}
          placeholder={longPlaceholder}
          value=""
        />
      </TooltipProvider>,
    );

    const combobox = screen.getByRole("combobox", { name: "选择日志文件" });
    const placeholderEl = within(combobox).getByText(longPlaceholder);
    expect(placeholderEl.className).toContain("truncate");
    expect(placeholderEl.className).toContain("pointer-events-auto");

    mockElementMetrics(placeholderEl, {
      clientWidth: 100,
      scrollWidth: 280,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(placeholderEl);
    act(() => {
      vi.advanceTimersByTime(320);
    });

    expect(screen.getByRole("tooltip").textContent).toBe(longPlaceholder);
  });

  it("shows tooltip above SelectContent (z-[110]) when an option overflows in open dropdown", () => {
    vi.useFakeTimers();
    const longOptionLabel =
      "/Users/example/very/long/log/directory/assetiweave-runtime-detail.log";

    render(
      <TooltipProvider>
        <SimpleSelect
          ariaLabel="选择日志文件"
          onChange={vi.fn()}
          options={[
            { label: "short.log", value: "short" },
            { label: longOptionLabel, value: "long" },
          ]}
          value="short"
        />
      </TooltipProvider>,
    );

    const combobox = screen.getByRole("combobox", { name: "选择日志文件" });
    fireEvent.keyDown(combobox, { key: "ArrowDown" });

    const listbox = screen.getByRole("listbox");
    const selectContent = listbox.closest(".ui-menu");
    expect(selectContent?.className).toContain("z-[100]");

    const longOption = screen.getByRole("option", { name: longOptionLabel });
    expect(longOption.className).toContain("min-w-0");

    const optionText = within(longOption).getByText(longOptionLabel);
    expect(optionText.className).toContain("truncate");

    mockElementMetrics(optionText, {
      clientWidth: 140,
      scrollWidth: 360,
      clientHeight: 20,
      scrollHeight: 20,
    });

    fireEvent.mouseEnter(optionText);
    act(() => {
      vi.advanceTimersByTime(320);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.textContent).toBe(longOptionLabel);
    expect(tooltip.className).toContain("z-[110]");
  });
});
