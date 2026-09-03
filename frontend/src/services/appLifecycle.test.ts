import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("./appUpdater", () => ({
  isTauriRuntime: () => true,
}));

import {
  APP_CLOSE_REQUESTED_EVENT,
  cancelAppClosePrompt,
  completeAppClose,
  subscribeAppCloseRequested,
} from "./appLifecycle";

describe("appLifecycle", () => {
  beforeEach(() => {
    invokeMock.mockReset().mockResolvedValue(undefined);
    listenMock.mockReset().mockResolvedValue(vi.fn());
  });

  it("sends the camelCase backup argument expected by Tauri", async () => {
    await completeAppClose(false);

    expect(invokeMock).toHaveBeenCalledWith("complete_app_close", {
      backupDatabase: false,
    });
  });

  it("cancels the close prompt", async () => {
    await cancelAppClosePrompt();

    expect(invokeMock).toHaveBeenCalledWith("cancel_app_close_prompt");
  });

  it("subscribes to app-close-requested event", async () => {
    const listener = vi.fn();
    await subscribeAppCloseRequested(listener);

    expect(listenMock).toHaveBeenCalledWith(APP_CLOSE_REQUESTED_EVENT, expect.any(Function));
    const callback = listenMock.mock.calls[0][1];
    callback();
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

