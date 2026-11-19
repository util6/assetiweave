/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { TooltipProvider } from "../ui/tooltip";
import { SessionShortIdChip } from "./SessionShortIdChip";

const SESSION_ID =
  "conversation-session-5b6a46fa1111222233334444555566667777888899990000aaaabbbbccccdddd";

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function renderChip() {
  return render(
    <I18nProvider>
      <TooltipProvider>
        <SessionShortIdChip sessionId={SESSION_ID} t={(key) => key} />
      </TooltipProvider>
    </I18nProvider>,
  );
}

describe("SessionShortIdChip", () => {
  it("renders the eight-character fragment shared with the CLI", () => {
    renderChip();

    expect(screen.getByText("5b6a46fa")).toBeTruthy();
  });

  it("copies the short id and reports the copied state", async () => {
    renderChip();

    fireEvent.click(screen.getByRole("button"));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith("5b6a46fa"));
    await waitFor(() =>
      expect(screen.getByRole("button").getAttribute("aria-label")).toBe(
        "conversation.content.copied",
      ),
    );
  });

  it("uses the unified glassmorphic Tooltip instead of a native title attribute", () => {
    vi.useFakeTimers();
    renderChip();

    const button = screen.getByRole("button");
    expect(button.hasAttribute("title")).toBe(false);

    fireEvent.pointerMove(button, { pointerType: "mouse" });
    act(() => {
      vi.advanceTimersByTime(320);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.className).toContain("ui-tooltip");
    expect(tooltip.textContent).toBe("conversation.session.copyShortId");
  });
});
