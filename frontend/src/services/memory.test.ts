import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryRecallSession, listMemoryPublicTasks, listMemoryRecent } from "./memory";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

describe("memory service", () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.unstubAllGlobals());

  it("does not simulate recent Memory in browser preview", async () => {
    vi.stubGlobal("window", {});
    await expect(listMemoryRecent()).resolves.toEqual([]);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("keeps Recall session creation desktop-only", async () => {
    vi.stubGlobal("window", {});
    await expect(createMemoryRecallSession()).rejects.toThrow("desktop application");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("validates the public TaskRuntime list at the desktop boundary", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([]);
    await expect(listMemoryPublicTasks(true)).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith("list_memory_public_tasks", { params: { active_only: true } });
  });
});
