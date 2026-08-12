/* @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  RouteTransitionOverlay,
  useRouteTransition,
} from "./RouteTransition";

function TransitionProbe() {
  const { completeTransition, startTransition, transition } = useRouteTransition({ durationMs: 300 });

  return (
    <>
      <button
        onClick={() => startTransition("groups", "正在加载分组管理")}
        type="button"
      >
        切换分组
      </button>
      <button onClick={() => completeTransition()} type="button">
        完成加载
      </button>
      <RouteTransitionOverlay transition={transition} />
    </>
  );
}

describe("RouteTransition", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows a page skeleton immediately and fades it out after the visual buffer", () => {
    vi.useFakeTimers();
    render(<TransitionProbe />);

    fireEvent.click(screen.getByRole("button", { name: "切换分组" }));

    expect(document.querySelector("[data-route-transition]")?.getAttribute("data-route-transition")).toBe("enter");
    expect(screen.getByText("正在加载分组管理")).toBeTruthy();
    expect(document.querySelectorAll(".aurora-skeleton").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole("button", { name: "完成加载" }));
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(document.querySelector("[data-route-transition]")?.getAttribute("data-route-transition")).toBe("exit");

    act(() => {
      vi.advanceTimersByTime(140);
    });
    expect(document.querySelector("[data-route-transition]")).toBeNull();
  });

  it("replaces an in-flight transition when navigation changes again", () => {
    vi.useFakeTimers();
    function Probe() {
      const { completeTransition, startTransition, transition } = useRouteTransition({ durationMs: 300 });
      return (
        <>
          <button onClick={() => startTransition("groups", "分组")} type="button">分组</button>
          <button onClick={() => startTransition("sources", "来源")} type="button">来源</button>
          <button onClick={() => completeTransition()} type="button">完成</button>
          <RouteTransitionOverlay transition={transition} />
        </>
      );
    }

    render(<Probe />);
    fireEvent.click(screen.getByRole("button", { name: "分组" }));
    act(() => {
      vi.advanceTimersByTime(180);
    });
    fireEvent.click(screen.getByRole("button", { name: "来源" }));

    const overlay = document.querySelector("[data-route-transition]");
    expect(overlay?.textContent).toContain("来源");
    expect(overlay?.textContent).not.toContain("分组");
    expect(overlay?.getAttribute("data-route-transition")).toBe("enter");

    act(() => {
      vi.advanceTimersByTime(300);
    });
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    act(() => {
      vi.advanceTimersByTime(140);
    });
    expect(document.querySelector("[data-route-transition]")).toBeNull();
  });
});
