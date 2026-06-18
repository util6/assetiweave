import { beforeEach, describe, expect, it, vi } from "vitest";
import { backupSkills, startSkillBackupTask } from "./catalog";

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
});
