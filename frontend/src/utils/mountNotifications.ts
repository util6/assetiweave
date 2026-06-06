import type { NotificationMessage } from "../components/notifications/NotificationBanner";
import type { AssetMountStatus } from "../types";
import { countMountedAssetsForProfile, getMountDisplayState } from "./mountState";

interface AssetMountNotificationInput {
  assetId: string;
  assetName: string;
  profileId: string;
  profileName: string;
  statuses: AssetMountStatus[];
}

export function buildAssetMountNotification({
  assetId,
  assetName,
  profileId,
  profileName,
  statuses,
}: AssetMountNotificationInput): Pick<NotificationMessage, "tone" | "messageKey" | "messageParams"> {
  const status = statuses.find((candidate) => candidate.asset_id === assetId && candidate.profile_id === profileId);
  const mounted = countMountedAssetsForProfile(statuses, profileId);
  const params = {
    name: assetName,
    profile: profileName,
    mounted,
  };

  switch (getMountDisplayState(status)) {
    case "mounted":
      return {
        tone: "success",
        messageKey: "mount.notification.assetMountedProfile",
        messageParams: params,
      };
    case "conflict":
      return {
        tone: "warning",
        messageKey: "mount.notification.assetConflictProfile",
        messageParams: params,
      };
    case "broken":
      return {
        tone: "warning",
        messageKey: "mount.notification.assetBrokenProfile",
        messageParams: params,
      };
    case "not_mounted":
    default:
      return {
        tone: "success",
        messageKey: "mount.notification.assetUnmountedProfile",
        messageParams: params,
      };
  }
}
