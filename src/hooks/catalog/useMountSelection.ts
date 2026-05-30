import { useMemo } from "react";
import { setAssetMount, unmountAssetMount } from "../../services/catalog";
import type { AssetMount, AssetMountStatus } from "../../types";

export function useMountSelection(
  assetMounts: AssetMount[],
  assetMountStatuses: AssetMountStatus[],
  applyAssetMount: (mount: AssetMount) => void,
  applyAssetMountStatus: (status: AssetMountStatus) => void,
) {
  const selectedMounts = useMemo(() => {
    return assetMounts.reduce<Record<string, string[]>>((selected, mount) => {
      if (!mount.enabled) {
        return selected;
      }
      selected[mount.asset_id] = [...(selected[mount.asset_id] ?? []), mount.profile_id];
      return selected;
    }, {});
  }, [assetMounts]);

  async function toggleMountProfile(assetId: string, profileId: string) {
    const physicalStatus = assetMountStatuses.find(
      (status) => status.asset_id === assetId && status.profile_id === profileId,
    );
    if (physicalStatus?.state === "mounted") {
      await setMountProfile(assetId, profileId, false);
      return;
    }

    const selected = (selectedMounts[assetId] ?? []).includes(profileId);
    await setMountProfile(assetId, profileId, !selected);
  }

  async function setMountProfile(assetId: string, profileId: string, enabled: boolean) {
    try {
      const physicalStatus = assetMountStatuses.find(
        (status) => status.asset_id === assetId && status.profile_id === profileId,
      );
      if (!enabled && physicalStatus?.state === "mounted") {
        const result = await unmountAssetMount(assetId, profileId);
        applyAssetMount(result.mount);
        applyAssetMountStatus(result.status);
        return;
      }

      applyAssetMount(await setAssetMount(assetId, profileId, enabled));
    } catch (error) {
      if (isTauriRuntime()) {
        throw error;
      }

      const now = new Date().toISOString();
      applyAssetMount({
        asset_id: assetId,
        profile_id: profileId,
        enabled,
        strategy: "symlink_to_source",
        created_at: now,
        updated_at: now,
      });
    }
  }

  async function setMountProfiles(assetIds: string[], profileId: string, enabled: boolean) {
    for (const assetId of assetIds) {
      await setMountProfile(assetId, profileId, enabled);
    }
  }

  return { selectedMounts, setMountProfiles, toggleMountProfile };
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
