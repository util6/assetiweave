import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import {
  invalidateCatalog,
  useSaveNavigation,
} from "../../app/query/catalogMutations";
import {
  assetsQueryOptions,
  catalogKeys,
  mountStatusesQueryOptions,
  navigationQueryOptions,
  overviewQueryOptions,
  profilesQueryOptions,
  shortcutsQueryOptions,
  sourcesQueryOptions,
} from "../../app/query/catalogQueries";
import { useQueryScope } from "../../app/query/QueryScopeProvider";
import { fallbackNavigationModel } from "../../mock/catalog";
import type { NavigationModel } from "../../router/types";
import {
  refreshAssetMountStatuses,
  updateAppShortcuts,
} from "../../services/catalog";
import type {
  AppOverview,
  AppShortcut,
  Asset,
  AssetKind,
  AssetMountStatus,
  Source,
  TargetProfile,
} from "../../types";

export function useCatalogData() {
  const queryClient = useQueryClient();
  const scope = useQueryScope();
  const activeScope = scope ?? { tenantId: "default", epoch: 1 };
  const enabled = Boolean(scope);

  const [optimisticNavigation, setOptimisticNavigation] =
    useState<NavigationModel | null>(null);

  // 租户/作用域切换时立即清除乐观导航，防止上一租户状态遮蔽新租户 Query 缓存
  useEffect(() => {
    setOptimisticNavigation(null);
  }, [activeScope.tenantId, activeScope.epoch]);

  const { save: saveNav, schedule: scheduleNav } = useSaveNavigation(
    activeScope,
    {
      onSettled: () => {
        setOptimisticNavigation(null);
      },
    },
  );

  const saveNavigationModel = async (nextModel: NavigationModel) => {
    setOptimisticNavigation(nextModel);
    try {
      const saved = await saveNav(nextModel);
      return saved;
    } finally {
      setOptimisticNavigation(null);
    }
  };

  const deferNavigationModelSave = (nextModel: NavigationModel) => {
    setOptimisticNavigation(nextModel);
    scheduleNav(nextModel);
  };

  const navigationQuery = useQuery({
    ...navigationQueryOptions(activeScope),
    enabled,
  });

  const navigationModel =
    optimisticNavigation ?? navigationQuery.data ?? fallbackNavigationModel;
  const activeAssetKind = getActiveAssetKind(navigationModel);

  const assetsQuery = useQuery({
    ...assetsQueryOptions(activeScope, activeAssetKind),
    enabled,
  });

  const sourcesQuery = useQuery({
    ...sourcesQueryOptions(activeScope),
    enabled,
  });

  const profilesQuery = useQuery({
    ...profilesQueryOptions(activeScope),
    enabled,
  });

  const overviewQuery = useQuery({
    ...overviewQueryOptions(activeScope),
    enabled,
  });

  const shortcutsQuery = useQuery({
    ...shortcutsQueryOptions(activeScope),
    enabled,
  });

  const mountStatusesQuery = useQuery({
    ...mountStatusesQueryOptions(activeScope),
    enabled,
  });

  const assets = assetsQuery.data ?? [];
  const sources = sourcesQuery.data ?? [];
  const profiles = profilesQuery.data ?? [];
  const overview = overviewQuery.data ?? null;
  const appShortcuts = shortcutsQuery.data ?? [];
  const assetMountStatuses = mountStatusesQuery.data ?? [];

  const loading =
    !scope ||
    assetsQuery.isLoading ||
    sourcesQuery.isLoading ||
    profilesQuery.isLoading ||
    overviewQuery.isLoading ||
    shortcutsQuery.isLoading ||
    mountStatusesQuery.isLoading ||
    navigationQuery.isLoading;

  async function loadCatalogData() {
    await invalidateCatalog(queryClient, activeScope, [
      "assets",
      "sources",
      "profiles",
      "overview",
      "shortcuts",
      "mountStatuses",
    ]);
  }

  async function refreshOverview(nextAssets?: Asset[]) {
    if (nextAssets) {
      queryClient.setQueryData(
        catalogKeys.assets(activeScope, activeAssetKind),
        nextAssets,
      );
      await invalidateCatalog(queryClient, activeScope, [
        "sources",
        "overview",
        "mountStatuses",
      ]);
    } else {
      await invalidateCatalog(queryClient, activeScope, [
        "assets",
        "sources",
        "overview",
        "mountStatuses",
      ]);
    }
  }

  async function refreshMountState() {
    const mountStatusList = await refreshAssetMountStatuses();
    queryClient.setQueryData(
      catalogKeys.mountStatuses(activeScope),
      mountStatusList,
    );
    return mountStatusList;
  }

  async function refreshCatalogAndMountState() {
    const mountStatusList = await refreshAssetMountStatuses();
    queryClient.setQueryData(
      catalogKeys.mountStatuses(activeScope),
      mountStatusList,
    );
    await invalidateCatalog(queryClient, activeScope, [
      "assets",
      "sources",
      "overview",
    ]);
    return mountStatusList;
  }

  async function refreshProfiles() {
    await invalidateCatalog(queryClient, activeScope, [
      "profiles",
      "shortcuts",
      "overview",
      "mountStatuses",
    ]);
  }

  function applyAssetMountStatus(nextStatus: AssetMountStatus) {
    queryClient.setQueryData<AssetMountStatus[]>(
      catalogKeys.mountStatuses(activeScope),
      (current = []) => [
        ...current.filter(
          (status) =>
            status.asset_id !== nextStatus.asset_id ||
            status.profile_id !== nextStatus.profile_id,
        ),
        nextStatus,
      ],
    );
  }

  function applyAssetUpdate(nextAsset: Asset) {
    queryClient.setQueryData<Asset[]>(
      catalogKeys.assets(activeScope, activeAssetKind),
      (current = []) =>
        current.map((asset) => (asset.id === nextAsset.id ? nextAsset : asset)),
    );
  }

  function removeAsset(assetId: string) {
    queryClient.setQueryData<Asset[]>(
      catalogKeys.assets(activeScope, activeAssetKind),
      (current = []) => current.filter((asset) => asset.id !== assetId),
    );
    queryClient.setQueryData<AssetMountStatus[]>(
      catalogKeys.mountStatuses(activeScope),
      (current = []) => current.filter((status) => status.asset_id !== assetId),
    );
    queryClient.setQueryData<AppOverview | null>(
      catalogKeys.overview(activeScope),
      (current) =>
        current
          ? { ...current, asset_count: Math.max(0, current.asset_count - 1) }
          : current,
    );
  }

  async function saveAppShortcuts(nextAppShortcuts: AppShortcut[]) {
    queryClient.setQueryData(
      catalogKeys.shortcuts(activeScope),
      nextAppShortcuts,
    );
    const savedAppShortcuts = await updateAppShortcuts(nextAppShortcuts);
    queryClient.setQueryData(
      catalogKeys.shortcuts(activeScope),
      savedAppShortcuts,
    );
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
  return model.headerTabs.find((tab) => tab.id === model.activeHeaderTabId)
    ?.assetKind;
}
