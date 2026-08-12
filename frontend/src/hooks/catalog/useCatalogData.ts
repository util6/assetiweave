import { useEffect, useRef, useState } from "react";
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
  const [loading, setLoading] = useState(true);
  const navigationSaveSequence = useRef(0);
  const deferredNavigationSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeAssetKind = getActiveAssetKind(navigationModel);

  useEffect(() => {
    void loadCatalogData();
  }, []);

  useEffect(
    () => () => {
      if (deferredNavigationSaveTimer.current !== null) {
        clearTimeout(deferredNavigationSaveTimer.current);
      }
    },
    [],
  );

  async function loadCatalogData() {
    setLoading(true);
    try {
      const loadNavigationSequence = navigationSaveSequence.current;
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
      if (navigationSaveSequence.current === loadNavigationSequence) {
        setNavigationModel(appNavigationModel);
      }
      setProfiles(profileList);
      setAppShortcuts(shortcutList);
    } finally {
      setLoading(false);
    }
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

  async function refreshCatalogAndMountState() {
    const [assetList, sourceList, appOverview, mountStatusList] = await Promise.all([
      listAssets(activeAssetKind),
      listSources(),
      getOverview(),
      refreshAssetMountStatuses(),
    ]);
    setAssets(assetList);
    setSources(sourceList);
    setAssetMountStatuses(mountStatusList);
    setOverview(appOverview);
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

  function applyAssetUpdate(nextAsset: Asset) {
    setAssets((current) => current.map((asset) => (asset.id === nextAsset.id ? nextAsset : asset)));
  }

  function removeAsset(assetId: string) {
    setAssets((current) => current.filter((asset) => asset.id !== assetId));
    setAssetMountStatuses((current) => current.filter((status) => status.asset_id !== assetId));
    setOverview((current) =>
      current ? { ...current, asset_count: Math.max(0, current.asset_count - 1) } : current,
    );
  }

  function cancelDeferredNavigationSave() {
    if (deferredNavigationSaveTimer.current !== null) {
      clearTimeout(deferredNavigationSaveTimer.current);
      deferredNavigationSaveTimer.current = null;
    }
  }

  async function saveNavigationModel(nextNavigationModel: NavigationModel) {
    cancelDeferredNavigationSave();
    const sequence = navigationSaveSequence.current + 1;
    navigationSaveSequence.current = sequence;
    setNavigationModel(nextNavigationModel);
    const savedNavigationModel = await updateNavigationModel(nextNavigationModel);
    if (navigationSaveSequence.current === sequence) {
      setNavigationModel(savedNavigationModel);
    }
    return savedNavigationModel;
  }

  function deferNavigationModelSave(nextNavigationModel: NavigationModel) {
    const sequence = navigationSaveSequence.current + 1;
    navigationSaveSequence.current = sequence;
    setNavigationModel(nextNavigationModel);
    cancelDeferredNavigationSave();
    deferredNavigationSaveTimer.current = setTimeout(() => {
      deferredNavigationSaveTimer.current = null;
      void updateNavigationModel(nextNavigationModel)
        .then((savedNavigationModel) => {
          if (navigationSaveSequence.current === sequence) {
            setNavigationModel(savedNavigationModel);
          }
        })
        .catch(() => undefined);
    }, 120);
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
    applyAssetUpdate,
    assetMountStatuses,
    assets,
    navigationModel,
    loading,
    overview,
    profiles,
    reloadCatalogData: loadCatalogData,
    refreshCatalogAndMountState,
    refreshMountState,
    refreshOverview,
    refreshProfiles,
    removeAsset,
    deferNavigationModelSave,
    saveAppShortcuts,
    saveNavigationModel,
    sources,
  };
}

function getActiveAssetKind(model: NavigationModel): AssetKind | undefined {
  return model.headerTabs.find((tab) => tab.id === model.activeHeaderTabId)?.assetKind;
}
