import { useEffect, useState } from "react";
import { fallbackNavigationModel } from "../../mock/catalog";
import type { NavigationModel } from "../../router/types";
import {
  getNavigationModel,
  getOverview,
  listAppShortcutSettings,
  listAssetMountStatuses,
  listAssets,
  listProfiles,
  listSources,
  refreshAssetMountStatuses,
  updateAppShortcuts,
  updateNavigationModel,
} from "../../services/catalog";
import type { AppOverview, AppShortcut, Asset, AssetKind, AssetMountStatus, Source, TargetProfile } from "../../types";

export function useCatalogData() {
  const [assets, setAssets] = useState<Asset[]>([]);
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
    const [assetList, sourceList, appOverview, profileList, shortcutList, mountStatusList] =
      await Promise.all([
        listAssets(activeKind),
        listSources(),
        getOverview(),
        listProfiles(),
        listAppShortcutSettings(),
        listAssetMountStatuses(),
      ]);
    setAssets(assetList);
    setSources(sourceList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
    setNavigationModel(appNavigationModel);
    setProfiles(profileList);
    setAppShortcuts(shortcutList);
  }

  async function refreshOverview(nextAssets?: Asset[]) {
    const [assetList, sourceList, appOverview, mountStatusList] = await Promise.all([
      nextAssets ? Promise.resolve(nextAssets) : listAssets(activeAssetKind),
      listSources(),
      getOverview(),
      listAssetMountStatuses(),
    ]);
    setAssets(assetList);
    setSources(sourceList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
  }

  async function refreshMountState() {
    const mountStatusList = await refreshAssetMountStatuses();
    setAssetMountStatuses(mountStatusList);
    return mountStatusList;
  }

  async function refreshProfiles() {
    const [profileList, shortcutList, appOverview, mountStatusList] = await Promise.all([
      listProfiles(),
      listAppShortcutSettings(),
      getOverview(),
      listAssetMountStatuses(),
    ]);
    setProfiles(profileList);
    setAppShortcuts(shortcutList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
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
    applyAssetMountStatus,
    assetMountStatuses,
    assets,
    navigationModel,
    overview,
    profiles,
    refreshMountState,
    refreshOverview,
    refreshProfiles,
    saveAppShortcuts,
    saveNavigationModel,
    sources,
  };
}

function getActiveAssetKind(model: NavigationModel): AssetKind | undefined {
  return model.headerTabs.find((tab) => tab.id === model.activeHeaderTabId)?.assetKind;
}
