import { describe, expect, it, vi } from "vitest";
import { backupSkills } from "./catalog";

const invokeMock = vi.hoisted(() => vi.fn());
const openMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openMock,
}));

describe("catalog services", () => {
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
});
