import type { AssetGroupDetail, AssetMountStatus } from "../types";
import { getMountDisplayState } from "./mountState";

export function groupMemberAssetIds(group: AssetGroupDetail | null | undefined) {
  const assetIds = new Set<string>();
  const manualAssetIds = new Set(group?.manual_asset_ids ?? []);
  for (const member of group?.members ?? []) {
    if (member.origin === "manual" && !manualAssetIds.has(member.asset_id)) {
      continue;
    }
    assetIds.add(member.asset_id);
  }
  for (const assetId of manualAssetIds) {
    assetIds.add(assetId);
  }
  return [...assetIds];
}

export function enabledGroupIds(groups: AssetGroupDetail[]) {
  return groups.filter((detail) => detail.group.enabled).map((detail) => detail.group.id);
}

export function toggleEnabledGroupSelection(currentIds: Iterable<string>, groups: AssetGroupDetail[]) {
  const current = new Set(currentIds);
  const enabledIds = enabledGroupIds(groups);
  const allSelected = enabledIds.length > 0 && enabledIds.every((groupId) => current.has(groupId));
  for (const groupId of enabledIds) {
    if (allSelected) {
      current.delete(groupId);
    } else {
      current.add(groupId);
    }
  }
  return current;
}

export function shouldShowGroupExclusiveMountControls(selectedGroupCount: number) {
  return selectedGroupCount > 0;
}

export function getGroupProfileMountCounts(
  assetIds: string[],
  profileId: string,
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>,
) {
  return assetIds.reduce(
    (counts, assetId) => {
      const status = (mountStatusesByAssetId.get(assetId) ?? []).find(
        (candidate) => candidate.profile_id === profileId,
      );
      const displayState = getMountDisplayState(status);
      return {
        mounted: counts.mounted + (displayState === "mounted" ? 1 : 0),
        total: counts.total + 1,
      };
    },
    { mounted: 0, total: 0 },
  );
}
