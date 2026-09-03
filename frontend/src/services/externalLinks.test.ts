import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openExternalUrl } from "./externalLinks";

const openUrlMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: openUrlMock,
}));

describe("externalLinks service", () => {
  beforeEach(() => {
    openUrlMock.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("opens URL via window.open in browser preview runtime", async () => {
    const windowOpenMock = vi.fn();
    vi.stubGlobal("window", {
      open: windowOpenMock,
    });

    await openExternalUrl("https://example.com");

    expect(windowOpenMock).toHaveBeenCalledWith(
      "https://example.com",
      "_blank",
      "noopener,noreferrer",
    );
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("opens URL via plugin-opener in Tauri runtime", async () => {
    const windowOpenMock = vi.fn();
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {},
      open: windowOpenMock,
    });

    await openExternalUrl("https://example.com");

    expect(openUrlMock).toHaveBeenCalledWith("https://example.com");
    expect(windowOpenMock).not.toHaveBeenCalled();
  });

  it("falls back to window.open if plugin-opener fails in Tauri runtime", async () => {
    const windowOpenMock = vi.fn();
    vi.stubGlobal("window", {
      __TAURI_INTERNALS__: {},
      open: windowOpenMock,
    });
    openUrlMock.mockRejectedValueOnce(new Error("opener failed"));

    await openExternalUrl("https://example.com");

    expect(windowOpenMock).toHaveBeenCalledWith(
      "https://example.com",
      "_blank",
      "noopener,noreferrer",
    );
  });
});
