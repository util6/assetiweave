import { describe, expect, it } from "vitest";
import type { Asset } from "../types";
import { getBackupableSkillAssets, getBackupableSkillAssetsByIds } from "./skillBackup";

describe("skill backup helpers", () => {
  it("keeps only Skill assets that are not already in the backup library", () => {
    const backupable = getBackupableSkillAssets([
      asset("skill-a"),
      asset("prompt-a", { kind: "prompt" }),
      asset("skill-b", { backup_status: { state: "backed_up", backup_path: "/backup/skill-b", hidden_asset_ids: [] } }),
      asset("skill-c", { backup_status: { state: "downloaded", backup_path: "/backup/skill-c", hidden_asset_ids: [] } }),
    ]);

    expect(backupable.map((candidate) => candidate.id)).toEqual(["skill-a"]);
  });

  it("resolves a unique backupable Skill list from member ids", () => {
    const assetsById = new Map([
      ["skill-a", asset("skill-a")],
      ["skill-b", asset("skill-b", { backup_status: { state: "backed_up", backup_path: "/backup/skill-b", hidden_asset_ids: [] } })],
      ["prompt-a", asset("prompt-a", { kind: "prompt" })],
    ]);

    const backupable = getBackupableSkillAssetsByIds(assetsById, ["skill-a", "skill-a", "missing", "skill-b", "prompt-a"]);

    expect(backupable.map((candidate) => candidate.id)).toEqual(["skill-a"]);
  });
});

function asset(id: string, overrides: Partial<Asset> = {}): Asset {
  return {
    id,
    source_id: "source-a",
    name: id,
    kind: "skill",
    format: "directory",
    relative_path: id,
    absolute_path: `/tmp/${id}`,
    entry_file: null,
    description: null,
    content_hash: null,
    discovered_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}
