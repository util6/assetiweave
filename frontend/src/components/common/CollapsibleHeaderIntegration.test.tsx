// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { afterEach, describe, expect, it } from "vitest";
import {
  CollapsibleHeader,
  CollapsibleHeaderProvider,
  useCollapsibleHeader,
} from "./CollapsibleHeader";
import { RenderSafeScrollSurface } from "./rendering/RenderSafeScrollSurface";

afterEach(cleanup);

describe("CollapsibleHeader with RenderSafeScrollSurface integration", () => {
  it("automatically collapses header when nested RenderSafeScrollSurface scrolls down", () => {
    function Page() {
      return (
        <CollapsibleHeaderProvider topThreshold={10}>
          <div data-testid="bounded-route">
            <CollapsibleHeader>
              <header data-testid="header">My Page Title</header>
            </CollapsibleHeader>
            <main>
              {/* Nested deep inside components */}
              <div>
                <RenderSafeScrollSurface data-testid="scroll-surface">
                  <div style={{ height: 2000 }}>Long Content</div>
                </RenderSafeScrollSurface>
              </div>
            </main>
          </div>
        </CollapsibleHeaderProvider>
      );
    }

    render(<Page />);

    const headerShell =
      screen.getByTestId("header").parentElement?.parentElement;
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");

    const scrollSurface = screen.getByTestId("scroll-surface");

    // 1. Within topThreshold (<=10) -> stays expanded
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 8 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");

    // 2. Scroll down past topThreshold -> immediately collapses
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 25 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("true");

    // 3. Keep scrolling down -> stays collapsed
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 60 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("true");

    // 4. Scroll up (backward) -> immediately re-expands
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 45 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");

    // 5. Jump back to top (<=10) -> always expanded
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 0 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");
  });

  it("resets with a real scroll baseline so an upward nudge stays expanded", () => {
    function Page() {
      const collapsible = useCollapsibleHeader();
      return (
        <div data-testid="bounded-route">
          <CollapsibleHeader>
            <header data-testid="header">My Page Title</header>
          </CollapsibleHeader>
          <button onClick={() => collapsible?.reset(400)} type="button">
            reset
          </button>
          <RenderSafeScrollSurface data-testid="scroll-surface">
            <div style={{ height: 2000 }}>Long Content</div>
          </RenderSafeScrollSurface>
        </div>
      );
    }

    render(
      <CollapsibleHeaderProvider topThreshold={10}>
        <Page />
      </CollapsibleHeaderProvider>,
    );

    const headerShell =
      screen.getByTestId("header").parentElement?.parentElement;
    const scrollSurface = screen.getByTestId("scroll-surface");

    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 400 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: "reset" }));
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");

    // 395 是从 400 向上回滚 5px，不能被当成向下滚动再次收起。
    act(() => {
      fireEvent.scroll(scrollSurface, { target: { scrollTop: 395 } });
    });
    expect(headerShell?.getAttribute("data-collapsed")).toBe("false");
  });
});
