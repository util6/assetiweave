// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppClosePrompt } from "./AppClosePrompt";

const subscribeAppCloseRequestedMock = vi.hoisted(() => vi.fn());
const completeAppCloseMock = vi.hoisted(() => vi.fn());
const cancelAppClosePromptMock = vi.hoisted(() => vi.fn());
const runWindowActionMock = vi.hoisted(() => vi.fn());

vi.mock("../services/appLifecycle", () => ({
  cancelAppClosePrompt: cancelAppClosePromptMock,
  completeAppClose: completeAppCloseMock,
  subscribeAppCloseRequested: subscribeAppCloseRequestedMock,
}));

vi.mock("../services/windowChrome", () => ({
  runWindowAction: runWindowActionMock,
}));

vi.mock("../i18n/I18nProvider", () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

describe("AppClosePrompt", () => {
  let closeListener: (() => void) | undefined;

  beforeEach(() => {
    closeListener = undefined;
    subscribeAppCloseRequestedMock
      .mockReset()
      .mockImplementation(async (listener: () => void) => {
        closeListener = listener;
        return vi.fn();
      });
    completeAppCloseMock.mockReset().mockResolvedValue(undefined);
    cancelAppClosePromptMock.mockReset().mockResolvedValue(undefined);
    runWindowActionMock.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => cleanup());

  it("shows the close prompt with backup enabled and confirms the selected choice", async () => {
    render(<AppClosePrompt />);
    await waitFor(() =>
      expect(subscribeAppCloseRequestedMock).toHaveBeenCalledWith(
        expect.any(Function),
      ),
    );

    act(() => closeListener?.());

    expect(screen.getByRole("dialog")).toBeTruthy();
    const backupCheckbox = screen.getByRole("checkbox", {
      name: "app.close.backupDatabase",
    }) as HTMLInputElement;
    expect(backupCheckbox.checked).toBe(true);
    fireEvent.click(backupCheckbox);
    expect(backupCheckbox.checked).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "app.close.confirm" }));

    await waitFor(() => {
      expect(completeAppCloseMock).toHaveBeenCalledWith(false);
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  it("allows dismissing the prompt without closing the app", async () => {
    render(<AppClosePrompt />);
    await waitFor(() =>
      expect(subscribeAppCloseRequestedMock).toHaveBeenCalled(),
    );
    act(() => closeListener?.());

    fireEvent.click(screen.getByRole("button", { name: "common.close" }));

    await waitFor(() => {
      expect(cancelAppClosePromptMock).toHaveBeenCalledTimes(1);
      expect(screen.queryByRole("dialog")).toBeNull();
    });
  });

  it("cancels the close request and minimizes the window", async () => {
    render(<AppClosePrompt />);
    await waitFor(() =>
      expect(subscribeAppCloseRequestedMock).toHaveBeenCalled(),
    );
    act(() => closeListener?.());

    fireEvent.click(screen.getByRole("button", { name: "app.close.minimize" }));

    await waitFor(() => {
      expect(cancelAppClosePromptMock).toHaveBeenCalledTimes(1);
      expect(runWindowActionMock).toHaveBeenCalledWith("minimize");
      expect(screen.queryByRole("dialog")).toBeNull();
    });

    act(() => closeListener?.());

    expect(
      (
        screen.getByRole("button", {
          name: "app.close.confirm",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    expect(
      (
        screen.getByRole("checkbox", {
          name: "app.close.backupDatabase",
        }) as HTMLInputElement
      ).disabled,
    ).toBe(false);
  });
});
