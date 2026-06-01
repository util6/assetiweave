import { useEffect, useMemo, useState } from "react";
import { type NotificationMessage } from "../../components/notifications/NotificationBanner";
import {
  applySkillGroupExclusiveMount,
  applySkillGroupMount,
  previewSkillGroupExclusiveMount,
  revealPath,
} from "../../services/catalog";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { summarizeMountStatusRefresh } from "../../utils/mountState";
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
  const { setMountProfiles, toggleMountProfile } = useMountSelection(
    catalogData.assetMountStatuses,
    catalogData.applyAssetMountStatus,
  );
  const [query, setQuery] = useState("");
  const [refreshingMountStatus, setRefreshingMountStatus] = useState(false);
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

  function showNotification(notification: Omit<NotificationMessage, "id"> & { id?: string }) {
    setNotification({
      id: notification.id ?? `notification-${Date.now()}`,
      ...notification,
    });
  }

  async function refreshMountStatus() {
    if (refreshingMountStatus) {
      return;
    }

    setRefreshingMountStatus(true);
    setNotification({
      id: `mount-status-refreshing-${Date.now()}`,
      tone: "info",
      messageKey: "mount.notification.refreshingStatus",
    });

    try {
      const statuses = await catalogData.refreshMountState();
      const summary = summarizeMountStatusRefresh(statuses);
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-status-refreshed-${Date.now()}`,
        tone: summary.issueCount > 0 ? "warning" : "success",
        messageKey: "mount.notification.statusRefreshed",
        messageParams: {
          count: summary.total,
          mounted: summary.mounted,
          issues: summary.issueCount,
        },
      });
    } catch (error) {
      setNotification({
        id: `mount-status-refresh-error-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.statusRefreshFailed",
        messageParams: { message: errorMessage(error) },
      });
    } finally {
      setRefreshingMountStatus(false);
    }
  }

  async function toggleMountAndClearPlan(assetId: string, profileId: string) {
    const asset = assetById.get(assetId);
    if (isDirectMountBlockedSource(asset ? sourceById.get(asset.source_id) : undefined)) {
      return;
    }

    try {
      await toggleMountProfile(assetId, profileId);
      await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-sync-${assetId}-${profileId}-${Date.now()}`,
        tone: "success",
        messageKey: "mount.notification.synced",
        messageParams: { name: asset?.name ?? assetId },
      });
    } catch (error) {
      await catalogData.refreshMountState().catch(() => undefined);
      setNotification({
        id: `mount-error-${assetId}-${profileId}-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.failed",
        messageParams: { message: errorMessage(error) },
      });
    }
  }

  async function setMountProfilesAndClearPlan(assetIds: string[], profileId: string, enabled: boolean) {
    const mountableAssetIds = assetIds.filter((assetId) => {
      const asset = assetById.get(assetId);
      return asset && !isDirectMountBlockedSource(sourceById.get(asset.source_id));
    });
    if (mountableAssetIds.length === 0) {
      return;
    }

    try {
      await setMountProfiles(mountableAssetIds, profileId, enabled);
      await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-batch-sync-${profileId}-${Date.now()}`,
        tone: "success",
        messageKey: "mount.notification.batchSynced",
        messageParams: { count: mountableAssetIds.length },
      });
    } catch (error) {
      await catalogData.refreshMountState().catch(() => undefined);
      setNotification({
        id: `mount-batch-error-${profileId}-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.failed",
        messageParams: { message: errorMessage(error) },
      });
    }
  }

  async function setGroupMountProfileAndClearPlan(
    groupId: string,
    assetIds: string[],
    profileId: string,
    enabled: boolean,
  ) {
    if (assetIds.length === 0) {
      return;
    }

    try {
      if (isTauriRuntime()) {
        const result = await applySkillGroupMount(groupId, profileId, enabled);
        await catalogData.refreshMountState();
        operations.clearDeploymentPlan();
        setNotification({
          id: `mount-group-sync-${groupId}-${profileId}-${Date.now()}`,
          tone: result.error_count > 0 ? "warning" : "success",
          messageKey: "group.mount.result",
          messageParams: {
            updated: result.updated_count,
            total: result.requested_count,
            errors: result.error_count,
          },
        });
        return;
      }

      const mountableAssetIds = assetIds.filter((assetId) => {
        const asset = assetById.get(assetId);
        return asset && !isDirectMountBlockedSource(sourceById.get(asset.source_id));
      });
      if (mountableAssetIds.length === 0) {
        return;
      }

      await setMountProfiles(mountableAssetIds, profileId, enabled);
      await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-group-preview-sync-${groupId}-${profileId}-${Date.now()}`,
        tone: "success",
        messageKey: "group.mount.result",
        messageParams: {
          updated: mountableAssetIds.length,
          total: mountableAssetIds.length,
          errors: 0,
        },
      });
    } catch (error) {
      await catalogData.refreshMountState().catch(() => undefined);
      setNotification({
        id: `mount-group-error-${groupId}-${profileId}-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.failed",
        messageParams: { message: errorMessage(error) },
      });
      throw error;
    }
  }

  async function previewGroupExclusiveMount(groupIds: string[], profileId: string) {
    return previewSkillGroupExclusiveMount({
      group_ids: groupIds,
      profile_id: profileId,
      mount_selected: true,
      dry_run: true,
    });
  }

  async function applyGroupExclusiveMountAndClearPlan(groupIds: string[], profileId: string) {
    try {
      const result = await applySkillGroupExclusiveMount({
        group_ids: groupIds,
        profile_id: profileId,
        mount_selected: true,
        dry_run: false,
      });
      await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-group-exclusive-sync-${profileId}-${Date.now()}`,
        tone: result.errors.length > 0 || result.skipped_count > 0 ? "warning" : "success",
        messageKey: "group.exclusive.result",
        messageParams: {
          keep: result.keep_count,
          mount: result.mount_count,
          unmount: result.unmount_count,
          skipped: result.skipped_count + result.errors.length,
        },
      });
      return result;
    } catch (error) {
      await catalogData.refreshMountState().catch(() => undefined);
      setNotification({
        id: `mount-group-exclusive-error-${profileId}-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.failed",
        messageParams: { message: errorMessage(error) },
      });
      throw error;
    }
  }

  return {
    ...catalogData,
    ...operations,
    dismissNotification,
    expandedIds,
    filteredAssets,
    notification,
    query,
    refreshingMountStatus,
    revealPath,
    applyGroupExclusiveMount: applyGroupExclusiveMountAndClearPlan,
    previewGroupExclusiveMount,
    refreshMountStatus,
    setGroupMountProfile: setGroupMountProfileAndClearPlan,
    setMountProfiles: setMountProfilesAndClearPlan,
    setQuery,
    showNotification,
    toggleAsset,
    toggleMountProfile: toggleMountAndClearPlan,
  };
}

export type CatalogController = ReturnType<typeof useCatalogController>;

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
