import { describe, expect, it } from "vitest";
import type { AssetMountStatus } from "../types";
import { buildAssetMountNotification } from "./mountNotifications";

describe("mount notification helpers", () => {
  it("reports a successful single mount from refreshed physical state", () => {
    expect(
      buildAssetMountNotification({
        assetId: "asset-a",
        assetName: "Frontend UI",
        profileId: "codex",
        profileName: "Codex",
        statuses: [
          status("asset-a", "codex", "mounted"),
          status("asset-b", "codex", "mounted"),
          status("asset-c", "cursor", "mounted"),
        ],
      }),
    ).toEqual({
      tone: "success",
      messageKey: "mount.notification.assetMountedProfile",
      messageParams: {
        name: "Frontend UI",
        profile: "Codex",
        mounted: 2,
      },
    });
  });

  it("reports an unmounted single skill from refreshed physical state", () => {
    expect(
      buildAssetMountNotification({
        assetId: "asset-a",
        assetName: "Frontend UI",
        profileId: "codex",
        profileName: "Codex",
        statuses: [
          status("asset-a", "codex", "not_mounted"),
          status("asset-b", "codex", "mounted"),
        ],
      }),
    ).toEqual({
      tone: "success",
      messageKey: "mount.notification.assetUnmountedProfile",
      messageParams: {
        name: "Frontend UI",
        profile: "Codex",
        mounted: 1,
      },
    });
  });

  it("reports warning notifications for conflict and broken physical states", () => {
    expect(
      buildAssetMountNotification({
        assetId: "asset-a",
        assetName: "Frontend UI",
        profileId: "codex",
        profileName: "Codex",
        statuses: [status("asset-a", "codex", "conflict")],
      }),
    ).toEqual({
      tone: "warning",
      messageKey: "mount.notification.assetConflictProfile",
      messageParams: {
        name: "Frontend UI",
        profile: "Codex",
        mounted: 0,
      },
    });

    expect(
      buildAssetMountNotification({
        assetId: "asset-a",
        assetName: "Frontend UI",
        profileId: "codex",
        profileName: "Codex",
        statuses: [status("asset-a", "codex", "broken")],
      }),
    ).toEqual({
      tone: "warning",
      messageKey: "mount.notification.assetBrokenProfile",
      messageParams: {
        name: "Frontend UI",
        profile: "Codex",
        mounted: 0,
      },
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
