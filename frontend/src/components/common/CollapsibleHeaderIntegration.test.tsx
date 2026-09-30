// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  CollapsibleHeader,
  CollapsibleHeaderProvider,
} from "./CollapsibleHeader";
import { RenderSafeScrollSurface } from "./rendering/RenderSafeScrollSurface";

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

    const headerShell = screen.getByTestId("header").parentElement?.parentElement;
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
});
