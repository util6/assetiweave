import { useEffect, useState } from "react";
import { fallbackNavigationModel } from "../../fixtures/catalog";
import type { NavigationModel } from "../../navigation/types";
import { getNavigationModel, getOverview, listAppShortcutSettings, listAssetMounts, listAssetMountStatuses, listAssets, listProfiles, listSources, updateAppShortcuts, updateNavigationModel } from "../../services/catalog";
import type { AppOverview, AppShortcut, Asset, AssetMount, AssetMountStatus, Source, TargetProfile } from "../../types";

export function useCatalogData() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [assetMounts, setAssetMounts] = useState<AssetMount[]>([]);
  const [assetMountStatuses, setAssetMountStatuses] = useState<AssetMountStatus[]>([]);
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [sources, setSources] = useState<Source[]>([]);
  const [profiles, setProfiles] = useState<TargetProfile[]>([]);
  const [appShortcuts, setAppShortcuts] = useState<AppShortcut[]>([]);
  const [navigationModel, setNavigationModel] = useState<NavigationModel>(fallbackNavigationModel);

  useEffect(() => {
    void Promise.all([listAssets(), listSources(), getOverview(), getNavigationModel(), listProfiles(), listAppShortcutSettings(), listAssetMounts(), listAssetMountStatuses()]).then(
      ([assetList, sourceList, appOverview, appNavigationModel, profileList, shortcutList, mountList, mountStatusList]) => {
        setAssets(assetList);
        setSources(sourceList);
        setAssetMounts(mountList);
        setAssetMountStatuses(mountStatusList);
        setOverview(appOverview);
        setNavigationModel(appNavigationModel);
        setProfiles(profileList);
        setAppShortcuts(shortcutList);
      },
    );
  }, []);

  async function refreshOverview(nextAssets?: Asset[]) {
    const [assetList, sourceList, appOverview, mountList, mountStatusList] = await Promise.all([
      nextAssets ? Promise.resolve(nextAssets) : listAssets(),
      listSources(),
      getOverview(),
      listAssetMounts(),
      listAssetMountStatuses(),
    ]);
    setAssets(assetList);
    setSources(sourceList);
    setAssetMounts(mountList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
  }

  function applyAssetMount(nextMount: AssetMount) {
    setAssetMounts((current) => [
      ...current.filter(
        (mount) => mount.asset_id !== nextMount.asset_id || mount.profile_id !== nextMount.profile_id,
      ),
      nextMount,
    ]);
  }

  async function saveNavigationModel(nextNavigationModel: NavigationModel) {
    setNavigationModel(nextNavigationModel);
    const savedNavigationModel = await updateNavigationModel(nextNavigationModel);
    setNavigationModel(savedNavigationModel);
    return savedNavigationModel;
  }

  async function saveAppShortcuts(nextAppShortcuts: AppShortcut[]) {
    setAppShortcuts(nextAppShortcuts);
    const savedAppShortcuts = await updateAppShortcuts(nextAppShortcuts);
    setAppShortcuts(savedAppShortcuts);
    return savedAppShortcuts;
  }

  return {
    appShortcuts,
    applyAssetMount,
    assetMounts,
    assetMountStatuses,
    assets,
    navigationModel,
    overview,
    profiles,
    refreshOverview,
    saveAppShortcuts,
    saveNavigationModel,
    sources,
  };
}
