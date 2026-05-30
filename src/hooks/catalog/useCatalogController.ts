import { useEffect, useMemo, useState } from "react";
import { type NotificationMessage } from "../../components/notifications/NotificationBanner";
import { revealPath } from "../../services/catalog";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { useAssetFilter } from "./useAssetFilter";
import { useCatalogData } from "./useCatalogData";
import { useCatalogOperations } from "./useCatalogOperations";
import { useExpandedAssets } from "./useExpandedAssets";
import { useMountSelection } from "./useMountSelection";

export function useCatalogController() {
  const { settings } = useAppSettings();
  const catalogData = useCatalogData();
  const operations = useCatalogOperations(catalogData.refreshOverview, catalogData.activeAssetKind);
  const { expandedIds, toggleAsset } = useExpandedAssets();
  const { selectedMounts, setMountProfiles, toggleMountProfile } = useMountSelection(
    catalogData.assetMounts,
    catalogData.assetMountStatuses,
    catalogData.applyAssetMount,
    catalogData.applyAssetMountStatus,
  );
  const [query, setQuery] = useState("");
  const [notification, setNotification] = useState<NotificationMessage | null>(() =>
    settings.showStartupNotification
      ? {
          id: "mvp-notification-outlet",
          tone: "success",
          messageKey: "notification.ready",
        }
      : null,
  );
  const filteredAssets = useAssetFilter(catalogData.assets, query);
  const assetById = useMemo(() => new Map(catalogData.assets.map((asset) => [asset.id, asset])), [catalogData.assets]);
  const sourceById = useMemo(() => new Map(catalogData.sources.map((source) => [source.id, source])), [catalogData.sources]);

  useEffect(() => {
    if (!settings.showStartupNotification) {
      setNotification((current) => (current?.id === "mvp-notification-outlet" ? null : current));
    }
  }, [settings.showStartupNotification]);

  function dismissNotification(id: string) {
    setNotification((current) => (current?.id === id ? null : current));
  }

  async function toggleMountAndClearPlan(assetId: string, profileId: string) {
    const asset = assetById.get(assetId);
    if (isDirectMountBlockedSource(asset ? sourceById.get(asset.source_id) : undefined)) {
      return;
    }

    await toggleMountProfile(assetId, profileId);
    operations.clearDeploymentPlan();
  }

  async function setMountProfilesAndClearPlan(assetIds: string[], profileId: string, enabled: boolean) {
    const mountableAssetIds = assetIds.filter((assetId) => {
      const asset = assetById.get(assetId);
      return asset && !isDirectMountBlockedSource(sourceById.get(asset.source_id));
    });
    if (mountableAssetIds.length === 0) {
      return;
    }

    await setMountProfiles(mountableAssetIds, profileId, enabled);
    operations.clearDeploymentPlan();
  }

  return {
    ...catalogData,
    ...operations,
    dismissNotification,
    expandedIds,
    filteredAssets,
    notification,
    query,
    revealPath,
    selectedMounts,
    setMountProfiles: setMountProfilesAndClearPlan,
    setQuery,
    toggleAsset,
    toggleMountProfile: toggleMountAndClearPlan,
  };
}

export type CatalogController = ReturnType<typeof useCatalogController>;
