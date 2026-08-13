import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { cancelAppClosePrompt, completeAppClose } from "./appLifecycle";

describe("appLifecycle", () => {
  beforeEach(() => {
    invokeMock.mockReset().mockResolvedValue(undefined);
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
});
