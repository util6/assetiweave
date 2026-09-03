/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { RouteTransitionOverlay } from "./RouteTransition";

describe("RouteTransitionOverlay", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders null when transition is null", () => {
    const { container } = render(<RouteTransitionOverlay transition={null} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders the skeleton overlay during enter phase", () => {
    render(
      <RouteTransitionOverlay
        transition={{
          id: 1,
          layout: "columns",
          label: "正在加载分组管理",
          phase: "enter",
        }}
      />,
    );

    const overlay = document.querySelector("[data-route-transition]");
    expect(overlay?.getAttribute("data-route-transition")).toBe("enter");
    expect(screen.getByText("正在加载分组管理")).toBeTruthy();
    expect(
      document.querySelectorAll(".aurora-skeleton").length,
    ).toBeGreaterThan(0);
  });

  it("adds exit class during exit phase", () => {
    render(
      <RouteTransitionOverlay
        transition={{
          id: 2,
          layout: "list",
          label: "正在加载来源",
          phase: "exit",
        }}
      />,
    );

    const overlay = document.querySelector("[data-route-transition]");
    expect(overlay?.getAttribute("data-route-transition")).toBe("exit");
    expect(overlay?.classList.contains("aurora-route-transition-exit")).toBe(
      true,
    );
  });
});
