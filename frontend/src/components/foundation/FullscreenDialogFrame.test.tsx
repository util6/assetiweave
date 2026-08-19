// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FullscreenDialogFrame } from "./FullscreenDialogFrame";

afterEach(cleanup);

describe("FullscreenDialogFrame", () => {
  it("uses the shared modal primitive while exposing a full viewport content surface", () => {
    render(
      <FullscreenDialogFrame onClose={() => undefined} title="Settings">
        <div data-testid="content">Content</div>
      </FullscreenDialogFrame>,
    );

    const dialog = screen.getByRole("dialog");
    expect(dialog.className).toContain("h-full");
    expect(dialog.className).toContain("max-w-none");
    expect(dialog.parentElement?.className).toContain("p-0");
    expect(screen.getByTestId("content")).toBeTruthy();
  });
});
