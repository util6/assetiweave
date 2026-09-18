// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PillTabs } from "./PillTabs";

describe("PillTabs", () => {
  it("renders all items and handles tab switching", () => {
    const handleSelect = vi.fn();
    const items = [
      { id: "all", label: "全部" },
      { id: "running", label: "进行中" },
      { id: "completed", label: "已完成" },
    ];

    render(
      <PillTabs
        activeId="all"
        ariaLabel="测试筛选"
        items={items}
        onSelect={handleSelect}
      />,
    );

    const allTab = screen.getByRole("button", { name: "全部" });
    const runningTab = screen.getByRole("button", { name: "进行中" });

    expect(allTab.getAttribute("aria-pressed")).toBe("true");
    expect(runningTab.getAttribute("aria-pressed")).toBe("false");

    fireEvent.click(runningTab);
    expect(handleSelect).toHaveBeenCalledWith("running", items[1]);
  });

  it("applies sm compact styling to tabs and indicator", () => {
    const items = [
      { id: "time", label: "按时间" },
      { id: "project", label: "按项目" },
    ];

    const { container } = render(
      <PillTabs activeId="time" items={items} onSelect={vi.fn()} size="sm" />,
    );

    const timeTab = screen.getByRole("button", { name: "按时间" });
    expect(timeTab.className).toContain("h-7");

    const indicator = container.querySelector(".ui-pill-indicator");
    expect(indicator).not.toBeNull();
    expect(indicator?.className).toContain("h-7");
  });
});
