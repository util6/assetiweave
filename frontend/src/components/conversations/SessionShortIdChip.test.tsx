/* @vitest-environment jsdom */

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
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
});

function renderChip() {
  return render(
    <I18nProvider>
      <SessionShortIdChip sessionId={SESSION_ID} t={(key) => key} />
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
});
