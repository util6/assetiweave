import { mountAssetMount, startBatchMount, unmountAssetMount } from "../../services/catalog";
import type { BatchMountTaskSnapshot } from "../../services/catalog";
import type { AssetMountStatus } from "../../types";
import { getMountDisplayState } from "../../utils/mountState";

export function useMountSelection(
  assetMountStatuses: AssetMountStatus[],
  applyAssetMountStatus: (status: AssetMountStatus) => void,
  startBackgroundBatchMount?: (params: Parameters<typeof startBatchMount>[0]) => Promise<BatchMountTaskSnapshot>,
) {
  async function toggleMountProfile(assetId: string, profileId: string) {
    const physicalStatus = assetMountStatuses.find(
      (status) => status.asset_id === assetId && status.profile_id === profileId,
    );
    const displayState = getMountDisplayState(physicalStatus);
    if (displayState === "mounted") {
      await setMountProfile(assetId, profileId, false);
      return;
    }

    await setMountProfile(assetId, profileId, true);
  }

  async function setMountProfile(assetId: string, profileId: string, enabled: boolean) {
    const physicalStatus = assetMountStatuses.find(
      (status) => status.asset_id === assetId && status.profile_id === profileId,
    );

    try {
      if (enabled) {
        const result = await mountAssetMount(assetId, profileId);
        applyAssetMountStatus(result.status);
        return;
      }

      if (!enabled && physicalStatus?.state === "mounted") {
        const result = await unmountAssetMount(assetId, profileId);
        applyAssetMountStatus(result.status);
        return;
      }
    } catch (error) {
      if (isTauriRuntime()) {
        throw error;
      }
      applyAssetMountStatus(fallbackMountStatus(assetId, profileId, enabled, physicalStatus));
    }
  }

  async function setMountProfiles(assetIds: string[], profileId: string, enabled: boolean) {
    if (isTauriRuntime() && startBackgroundBatchMount) {
      await startBackgroundBatchMount({
        mode: "explicit",
        assetIds,
        profileId,
        enabled,
      });
      return;
    }
    for (const assetId of assetIds) {
      await setMountProfile(assetId, profileId, enabled);
    }
  }

  return { setMountProfiles, toggleMountProfile };
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function fallbackMountStatus(
  assetId: string,
  profileId: string,
  enabled: boolean,
  physicalStatus?: AssetMountStatus,
): AssetMountStatus {
  return {
    asset_id: assetId,
    profile_id: profileId,
    target_dir: physicalStatus?.target_dir ?? "",
    target_path: physicalStatus?.target_path ?? "",
    state: enabled ? "mounted" : "not_mounted",
    linked_source: physicalStatus?.linked_source ?? null,
  };
}
