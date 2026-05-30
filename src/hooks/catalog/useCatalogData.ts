import { useEffect, useState } from "react";
import { fallbackNavigationModel } from "../../mock/catalog";
import type { NavigationModel } from "../../router/types";
import { getNavigationModel, getOverview, listAppShortcutSettings, listAssetMounts, listAssetMountStatuses, listAssets, listProfiles, listSources, updateAppShortcuts, updateNavigationModel } from "../../services/catalog";
import type { AppOverview, AppShortcut, Asset, AssetKind, AssetMount, AssetMountStatus, Source, TargetProfile } from "../../types";

export function useCatalogData() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [assetMounts, setAssetMounts] = useState<AssetMount[]>([]);
  const [assetMountStatuses, setAssetMountStatuses] = useState<AssetMountStatus[]>([]);
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [sources, setSources] = useState<Source[]>([]);
  const [profiles, setProfiles] = useState<TargetProfile[]>([]);
  const [appShortcuts, setAppShortcuts] = useState<AppShortcut[]>([]);
  const [navigationModel, setNavigationModel] = useState<NavigationModel>(fallbackNavigationModel);
  const activeAssetKind = getActiveAssetKind(navigationModel);

  useEffect(() => {
    void loadCatalogData();
  }, []);

  async function loadCatalogData() {
    const appNavigationModel = await getNavigationModel();
    const activeKind = getActiveAssetKind(appNavigationModel);
    const [assetList, sourceList, appOverview, profileList, shortcutList, mountList, mountStatusList] =
      await Promise.all([
        listAssets(activeKind),
        listSources(),
        getOverview(),
        listProfiles(),
        listAppShortcutSettings(),
        listAssetMounts(),
        listAssetMountStatuses(),
      ]);
    setAssets(assetList);
    setSources(sourceList);
    setAssetMounts(mountList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
    setNavigationModel(appNavigationModel);
    setProfiles(profileList);
    setAppShortcuts(shortcutList);
  }

  async function refreshOverview(nextAssets?: Asset[]) {
    const [assetList, sourceList, appOverview, mountList, mountStatusList] = await Promise.all([
      nextAssets ? Promise.resolve(nextAssets) : listAssets(activeAssetKind),
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

  function applyAssetMountStatus(nextStatus: AssetMountStatus) {
    setAssetMountStatuses((current) => [
      ...current.filter(
        (status) => status.asset_id !== nextStatus.asset_id || status.profile_id !== nextStatus.profile_id,
      ),
      nextStatus,
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
    activeAssetKind,
    appShortcuts,
    applyAssetMount,
    applyAssetMountStatus,
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

function getActiveAssetKind(model: NavigationModel): AssetKind | undefined {
  return model.headerTabs.find((tab) => tab.id === model.activeHeaderTabId)?.assetKind;
}
