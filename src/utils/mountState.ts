import type { AssetMountStatus } from "../types";

export type MountDisplayState = "mounted" | "not_mounted" | "conflict" | "broken";

export interface MountStatusRefreshSummary {
  total: number;
  mounted: number;
  notMounted: number;
  conflict: number;
  broken: number;
  issueCount: number;
}

export function groupMountStatusesByAssetId(statuses: AssetMountStatus[]) {
  return statuses.reduce<Map<string, AssetMountStatus[]>>((grouped, status) => {
    grouped.set(status.asset_id, [...(grouped.get(status.asset_id) ?? []), status]);
    return grouped;
  }, new Map());
}

export function getMountDisplayState(mountStatus?: AssetMountStatus): MountDisplayState {
  return mountStatus?.state ?? "not_mounted";
}

export function getMountDisplayStatesByProfileId(mountStatuses: AssetMountStatus[]) {
  return mountStatuses.reduce<Record<string, MountDisplayState>>((states, status) => {
    states[status.profile_id] = getMountDisplayState(status);
    return states;
  }, {});
}

export function getAssetMountSummaryState(mountStatuses: AssetMountStatus[]): MountDisplayState {
  const states = Object.values(getMountDisplayStatesByProfileId(mountStatuses));
  if (states.includes("conflict")) return "conflict";
  if (states.includes("broken")) return "broken";
  if (states.includes("mounted")) return "mounted";
  return "not_mounted";
}

export function getMountedProfileIds(mountStatuses: AssetMountStatus[]) {
  return mountStatuses
    .filter((status) => status.state === "mounted")
    .map((status) => status.profile_id);
}

export function summarizeMountStatusRefresh(statuses: AssetMountStatus[]): MountStatusRefreshSummary {
  const summary = statuses.reduce(
    (current, status) => {
      current.total += 1;
      if (status.state === "mounted") current.mounted += 1;
      if (status.state === "not_mounted") current.notMounted += 1;
      if (status.state === "conflict") current.conflict += 1;
      if (status.state === "broken") current.broken += 1;
      return current;
    },
    {
      total: 0,
      mounted: 0,
      notMounted: 0,
      conflict: 0,
      broken: 0,
      issueCount: 0,
    },
  );
  summary.issueCount = summary.conflict + summary.broken;
  return summary;
}
