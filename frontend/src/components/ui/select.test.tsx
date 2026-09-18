/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { SimpleSelect } from "./select";

describe("SimpleSelect component (built with @radix-ui/react-select)", () => {
  beforeAll(() => {
    window.HTMLElement.prototype.hasPointerCapture = vi.fn();
    window.HTMLElement.prototype.setPointerCapture = vi.fn();
    window.HTMLElement.prototype.releasePointerCapture = vi.fn();
    window.HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  afterEach(() => {
    cleanup();
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
});
