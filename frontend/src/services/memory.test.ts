import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { acceptMemoryCandidate, createMemoryItem, listMemoryItems } from "./memory";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("memory service", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns an explicit empty state in browser preview without simulating Memory rules", async () => {
    vi.stubGlobal("window", {});

    await expect(listMemoryItems()).resolves.toEqual({
      availability: "browser_preview",
      total_count: 0,
      items: [],
      limit: 50,
      offset: 0,
    });
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("rejects browser-preview writes instead of creating a second persistence engine", async () => {
    vi.stubGlobal("window", {});

    await expect(
      createMemoryItem({
        kind: "decision",
        title: "Decision",
        content_markdown: "Content",
      }),
    ).rejects.toThrow("desktop application");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("invokes and validates the Tauri Memory item list", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({ total_count: 0, items: [], limit: 25, offset: 0 });

    await expect(listMemoryItems({ statuses: ["active"], limit: 25 })).resolves.toEqual({
      availability: "tauri",
      total_count: 0,
      items: [],
      limit: 25,
      offset: 0,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_memory_items", {
      params: { statuses: ["active"], limit: 25 },
    });
  });

  it("sends candidate acceptance through the dedicated Tauri command", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({});

    await expect(
      acceptMemoryCandidate({ item_id: "memory-1", title: "Edited candidate" }),
    ).rejects.toThrow();
    expect(invokeMock).toHaveBeenCalledWith("accept_memory_candidate", {
      params: { item_id: "memory-1", title: "Edited candidate" },
    });
  });
});
