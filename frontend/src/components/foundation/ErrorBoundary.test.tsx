// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

afterEach(cleanup);

function ThrowingComponent({ message = "Boom" }: { message?: string }): ReactElement {
  throw new Error(message);
}

describe("ErrorBoundary", () => {
  it("renders children when no error occurs", () => {
    render(
      <ErrorBoundary>
        <div data-testid="child">Safe Child</div>
      </ErrorBoundary>,
    );

    expect(screen.getByTestId("child").textContent).toBe("Safe Child");
  });

  it("catches errors and renders fallback UI", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});

    render(
      <ErrorBoundary>
        <ThrowingComponent message="Simulated Crash" />
      </ErrorBoundary>,
    );

    expect(screen.getByText("组件渲染遇到问题")).toBeTruthy();
    expect(screen.getByText("Simulated Crash")).toBeTruthy();

    spy.mockRestore();
  });
});
