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
});
