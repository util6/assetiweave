import { describe, expect, it } from "vitest";
import type { AssetGroupDetail, AssetMountStatus } from "../types";
import { enabledGroupIds, getGroupProfileMountCounts, groupMemberAssetIds, toggleEnabledGroupSelection } from "./skillGroups";

describe("skill group helpers", () => {
  it("returns resolved member asset ids in group order", () => {
    expect(groupMemberAssetIds(group(["asset-a", "asset-b"]))).toEqual(["asset-a", "asset-b"]);
    expect(groupMemberAssetIds(null)).toEqual([]);
  });

  it("includes manual asset ids when resolved members are stale", () => {
    expect(groupMemberAssetIds(group(["asset-a"], ["asset-a", "asset-b"]))).toEqual(["asset-a", "asset-b"]);
  });

  it("does not keep stale manual resolved members after they are removed", () => {
    expect(groupMemberAssetIds(group(["asset-a"], []))).toEqual([]);
  });

  it("keeps rule resolved members even when manual asset ids are empty", () => {
    expect(groupMemberAssetIds(group(["asset-a"], [], "rule"))).toEqual(["asset-a"]);
  });

  it("counts physically mounted members per profile", () => {
    const counts = getGroupProfileMountCounts(
      ["asset-a", "asset-b", "asset-c"],
      "codex",
      new Map([
        ["asset-a", [status("asset-a", "codex", "mounted")]],
        ["asset-b", [status("asset-b", "codex", "not_mounted")]],
      ]),
    );

    expect(counts).toEqual({ mounted: 1, total: 3 });
  });

  it("selects and clears all enabled groups without selecting disabled groups", () => {
    const enabledA = group(["asset-a"], ["asset-a"], "manual", "group-a");
    const enabledB = group(["asset-b"], ["asset-b"], "manual", "group-b");
    const disabled = group(["asset-c"], ["asset-c"], "manual", "group-c", false);

    expect(enabledGroupIds([enabledA, enabledB, disabled])).toEqual(["group-a", "group-b"]);
    expect([...toggleEnabledGroupSelection([], [enabledA, enabledB, disabled])].sort()).toEqual([
      "group-a",
      "group-b",
    ]);
    expect([...toggleEnabledGroupSelection(["group-a", "group-b"], [enabledA, enabledB, disabled])]).toEqual([]);
  });
});

function group(
  assetIds: string[],
  manualAssetIds = assetIds,
  origin: AssetGroupDetail["members"][number]["origin"] = "manual",
  groupId = "frontend",
  enabled = true,
): AssetGroupDetail {
  return {
    group: {
      id: groupId,
      name: "Frontend",
      description: null,
      color: "#10b981",
      asset_kind: "skill",
      enabled,
      sort_order: 0,
      rules: { source_ids: [], relative_path_globs: [], name_contains: null },
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    },
    members: assetIds.map((asset_id) => ({ asset_id, origin })),
    manual_asset_ids: manualAssetIds,
  };
}

function status(assetId: string, profileId: string, state: AssetMountStatus["state"]): AssetMountStatus {
  return {
    asset_id: assetId,
    profile_id: profileId,
    state,
    target_dir: `/target/${profileId}`,
    target_path: `/target/${profileId}/skill`,
    linked_source: null,
  };
}
