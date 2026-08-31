import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fallbackNavigationModel } from "../mock/catalog";
import { backupSkills, getNavigationModel, scanSources, startSkillBackupTask } from "./catalog";

const invokeMock = vi.hoisted(() => vi.fn());
const openMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openMock,
}));

describe("catalog services", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    openMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("backs up each unique Skill asset id", async () => {
    invokeMock.mockImplementation(async (_command: string, args: { assetId: string }) => ({
      id: args.assetId,
      source_id: "source-a",
      name: args.assetId,
      kind: "skill",
      format: "directory",
      relative_path: args.assetId,
      absolute_path: `/tmp/${args.assetId}`,
      entry_file: null,
      description: null,
      content_hash: null,
      discovered_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    }));

    const results = await backupSkills(["skill-a", "skill-a", "skill-b"]);

    expect(results.map((asset) => asset.id)).toEqual(["skill-a", "skill-b"]);
    expect(invokeMock).toHaveBeenNthCalledWith(1, "backup_skill", { assetId: "skill-a" });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "backup_skill", { assetId: "skill-b" });
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it("starts one background task for unique Skill asset ids", async () => {
    const runningTask = {
      id: "skill-backup-1",
      status: "running",
      asset_ids: ["skill-a", "skill-b"],
      total_count: 2,
      completed_count: 0,
      failed_count: 0,
      current_asset_id: "skill-a",
      started_at: "2026-06-18T00:00:00Z",
      finished_at: null,
      assets: [],
      errors: [],
      error: null,
    } as const;
    invokeMock.mockResolvedValue(runningTask);

    const result = await startSkillBackupTask([" skill-a ", "skill-a", "skill-b", ""]);

    expect(result).toEqual(runningTask);
    expect(invokeMock).toHaveBeenCalledWith("backup_skills", {
      assetIds: ["skill-a", "skill-b"],
    });
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("adds new default Memory navigation to an older browser-preview model without overwriting custom labels", async () => {
    const storedModel = JSON.parse(JSON.stringify(fallbackNavigationModel));
    storedModel.headerTabs = storedModel.headerTabs
      .filter((tab: { id: string }) => tab.id !== "memory")
      .map((tab: { id: string; label: string }) => (tab.id === "skills" ? { ...tab, label: "My Skills" } : tab));
    delete storedModel.subNavItems.memory;
    const storage = createMockLocalStorage();
    storage.setItem("assetiweave.preview.navigation", JSON.stringify(storedModel));
    vi.stubGlobal("localStorage", storage);
    invokeMock.mockRejectedValueOnce(new Error("browser preview"));

    const model = await getNavigationModel();

    expect(model.headerTabs.find((tab) => tab.id === "skills")?.label).toBe("My Skills");
    expect(model.headerTabs.some((tab) => tab.id === "memory")).toBe(true);
    expect(model.subNavItems.memory.map((item) => item.routeKey)).toEqual([
      "memory.recent",
      "memory.recall",
    ]);
  });

  it("does not expose a synchronous desktop scan fallback", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });

    await expect(scanSources()).rejects.toThrow("Desktop source scans must use startSourceScan");
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: vi.fn(() => values.clear()),
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    key: vi.fn((index: number) => Array.from(values.keys())[index] ?? null),
    removeItem: vi.fn((key: string) => values.delete(key)),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
  };
}
