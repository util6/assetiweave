import { useMemo } from "react";
import { toggleAssetMount } from "../../services/catalog";
import type { AssetMount } from "../../types";

export function useMountSelection(assetMounts: AssetMount[], applyAssetMount: (mount: AssetMount) => void) {
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
    try {
      applyAssetMount(await toggleAssetMount(assetId, profileId));
    } catch (error) {
      if (isTauriRuntime()) {
        throw error;
      }

      const now = new Date().toISOString();
      applyAssetMount({
        asset_id: assetId,
        profile_id: profileId,
        enabled: !(selectedMounts[assetId] ?? []).includes(profileId),
        strategy: "symlink_to_source",
        created_at: now,
        updated_at: now,
      });
    }
  }

  return { selectedMounts, toggleMountProfile };
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
