import { describe, expect, it } from "vitest";
import type { AssetMountStatus } from "../types";
import {
  getAssetMountSummaryState,
  getMountDisplayState,
  getMountDisplayStatesByProfileId,
  getMountedProfileIds,
  countAssetsForProfileState,
  countMountedAssetsForProfile,
  groupMountStatusesByAssetId,
  summarizeMountStatusRefresh,
} from "./mountState";

describe("mount state helpers", () => {
  it("groups physical mount statuses by asset id", () => {
    const grouped = groupMountStatusesByAssetId([
      status("asset-a", "codex", "mounted"),
      status("asset-b", "codex", "not_mounted"),
    ]);

    expect(grouped.get("asset-a")).toEqual([status("asset-a", "codex", "mounted")]);
    expect(grouped.get("asset-b")).toEqual([status("asset-b", "codex", "not_mounted")]);
  });

  it("derives display state only from physical state", () => {
    expect(getMountDisplayState(status("asset-a", "codex", "mounted"))).toBe("mounted");
    expect(getMountDisplayState(status("asset-a", "codex", "not_mounted"))).toBe("not_mounted");
    expect(getMountDisplayState(status("asset-a", "codex", "conflict"))).toBe("conflict");
    expect(getMountDisplayState(status("asset-a", "codex", "broken"))).toBe("broken");
    expect(getMountDisplayState()).toBe("not_mounted");
  });

  it("builds display states for profiles from physical status", () => {
    expect(
      getMountDisplayStatesByProfileId([
        status("asset-a", "codex", "mounted"),
        status("asset-a", "cursor", "not_mounted"),
      ]),
    ).toEqual({
      codex: "mounted",
      cursor: "not_mounted",
    });
  });

  it("summarizes an asset by the most actionable mount state", () => {
    expect(getAssetMountSummaryState([status("asset-a", "codex", "mounted")])).toBe("mounted");
    expect(getAssetMountSummaryState([status("asset-a", "codex", "not_mounted")])).toBe("not_mounted");
    expect(getAssetMountSummaryState([status("asset-a", "codex", "conflict"), status("asset-a", "cursor", "mounted")])).toBe("conflict");
    expect(getAssetMountSummaryState([])).toBe("not_mounted");
  });

  it("extracts physically mounted profile ids", () => {
    expect(
      getMountedProfileIds([
        status("asset-a", "codex", "mounted"),
        status("asset-a", "cursor", "not_mounted"),
      ]),
    ).toEqual(["codex"]);
  });

  it("counts currently mounted assets for a profile", () => {
    expect(
      countMountedAssetsForProfile([
        status("asset-a", "codex", "mounted"),
        status("asset-b", "codex", "not_mounted"),
        status("asset-c", "codex", "mounted"),
        status("asset-d", "cursor", "mounted"),
      ], "codex"),
    ).toBe(2);
  });

  it("counts requested assets by refreshed profile state", () => {
    const statuses = [
      status("asset-a", "codex", "mounted"),
      status("asset-b", "codex", "not_mounted"),
      status("asset-c", "codex", "conflict"),
      status("asset-d", "cursor", "mounted"),
    ];

    expect(countAssetsForProfileState(["asset-a", "asset-a", "asset-b", "asset-missing"], statuses, "codex", "mounted")).toBe(1);
    expect(countAssetsForProfileState(["asset-a", "asset-b", "asset-missing"], statuses, "codex", "not_mounted")).toBe(2);
    expect(countAssetsForProfileState(["asset-c", "asset-d"], statuses, "codex", "conflict")).toBe(1);
  });

  it("summarizes physical refresh results for user feedback", () => {
    expect(
      summarizeMountStatusRefresh([
        status("asset-a", "codex", "mounted"),
        status("asset-a", "cursor", "not_mounted"),
        status("asset-b", "codex", "conflict"),
        status("asset-b", "cursor", "broken"),
      ]),
    ).toEqual({
      total: 4,
      mounted: 1,
      notMounted: 1,
      conflict: 1,
      broken: 1,
      issueCount: 2,
    });
  });
});

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
