// @vitest-environment jsdom

import { fireEvent, render, renderHook, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it } from "vitest";
import {
  CollapsibleHeader,
  CollapsibleHeaderProvider,
  useCollapsibleHeader,
  useCollapsibleHeaderState,
} from "./CollapsibleHeader";

describe("useCollapsibleHeaderState", () => {
  it("initializes with visible true by default", () => {
    const { result } = renderHook(() => useCollapsibleHeaderState());
    expect(result.current.visible).toBe(true);
  });

  it("collapses when scrolling down past top threshold", () => {
    const { result } = renderHook(() =>
      useCollapsibleHeaderState({ topThreshold: 10 }),
    );

    act(() => {
      result.current.onScroll({
        currentTarget: { scrollTop: 50 },
      } as React.UIEvent<HTMLElement>);
    });

    expect(result.current.visible).toBe(false);
  });

  it("re-expands when scrolling up", () => {
    const { result } = renderHook(() =>
      useCollapsibleHeaderState({ topThreshold: 10 }),
    );

    // Scroll down to collapse
    act(() => {
      result.current.onScroll({
        currentTarget: { scrollTop: 60 },
      } as React.UIEvent<HTMLElement>);
    });
    expect(result.current.visible).toBe(false);

    // Scroll up -> immediately re-expands
    act(() => {
      result.current.onScroll({
        currentTarget: { scrollTop: 40 },
      } as React.UIEvent<HTMLElement>);
    });
    expect(result.current.visible).toBe(true);
  });

  it("forces visible when scrolled back to top threshold", () => {
    const { result } = renderHook(() =>
      useCollapsibleHeaderState({ topThreshold: 15 }),
    );

    // Scroll down to collapse
    act(() => {
      result.current.onScroll({
        currentTarget: { scrollTop: 100 },
      } as React.UIEvent<HTMLElement>);
    });
    expect(result.current.visible).toBe(false);

    // Jump/scroll back to top (e.g. 5px)
    act(() => {
      result.current.onScroll({
        currentTarget: { scrollTop: 5 },
      } as React.UIEvent<HTMLElement>);
    });
    expect(result.current.visible).toBe(true);
  });
});

describe("CollapsibleHeader component", () => {
  it("renders children with proper data-collapsed attribute", () => {
    const { rerender } = render(
      <CollapsibleHeader visible={true}>
        <div>Header Content</div>
      </CollapsibleHeader>,
    );

    const header =
      screen.getByText("Header Content").parentElement?.parentElement;
    expect(header?.getAttribute("data-collapsed")).toBe("false");
    expect(header?.getAttribute("aria-hidden")).toBe("false");

    rerender(
      <CollapsibleHeader visible={false}>
        <div>Header Content</div>
      </CollapsibleHeader>,
    );

    expect(header?.getAttribute("data-collapsed")).toBe("true");
    expect(header?.getAttribute("aria-hidden")).toBe("true");
  });

  it("integrates with CollapsibleHeaderProvider context", () => {
    function TestConsumer() {
      const { onScroll } = useCollapsibleHeader()!;
      return (
        <div>
          <CollapsibleHeader>
            <h1>Managed Title</h1>
          </CollapsibleHeader>
          <div data-testid="scroll-area" onScroll={onScroll} />
        </div>
      );
    }

    render(
      <CollapsibleHeaderProvider topThreshold={10}>
        <TestConsumer />
      </CollapsibleHeaderProvider>,
    );

    const titleElement =
      screen.getByText("Managed Title").parentElement?.parentElement;
    expect(titleElement?.getAttribute("data-collapsed")).toBe("false");

    const scrollArea = screen.getByTestId("scroll-area");
    act(() => {
      fireEvent.scroll(scrollArea, { target: { scrollTop: 80 } });
    });

    expect(titleElement?.getAttribute("data-collapsed")).toBe("true");
  });
});
