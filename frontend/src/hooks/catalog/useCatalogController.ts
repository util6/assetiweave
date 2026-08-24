import { useEffect, useMemo, useRef, useState } from "react";
import { useCatalogTasks } from "../../app/backgroundTasks/CatalogTaskProvider";
import { type NotificationMessage } from "../../components/notifications/NotificationBanner";
import {
  applySkillGroupExclusiveMount,
  applySkillGroupMount,
  previewSkillGroupExclusiveMount,
  revealPath,
  type BatchMountTaskSnapshot,
} from "../../services/catalog";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { useTenantController } from "../tenants/useTenantController";
import {
  countAssetsForProfileState,
  countMountedAssetsForProfile,
  summarizeMountStatusRefresh,
} from "../../utils/mountState";
import { buildAssetMountNotification } from "../../utils/mountNotifications";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { useAssetFilter } from "./useAssetFilter";
import { useCatalogData } from "./useCatalogData";
import { useCatalogOperations } from "./useCatalogOperations";
import { useExpandedAssets } from "./useExpandedAssets";
import { useMountSelection } from "./useMountSelection";

type PendingBatchMount =
  | {
      mode: "explicit" | "group";
      assetIds: string[];
      profileId: string;
      enabled: boolean;
      groupId?: string;
    }
  | {
      mode: "exclusive";
      groupIds: string[];
      profileId: string;
    };

export function useCatalogController() {
  const { settings } = useAppSettings();
  const { batchMount, sourceScan, startBatchMount, startSourceScan } = useCatalogTasks();
  const catalogData = useCatalogData();
  const operations = useCatalogOperations(
    catalogData.refreshOverview,
    catalogData.activeAssetKind,
    startSourceScan,
    sourceScan,
  );
  const tenantController = useTenantController({
    onTenantChanged: async () => {
      await catalogData.reloadCatalogData();
      operations.clearDeploymentPlan();
      setQuery("");
    },
  });
  const { expandedIds, toggleAsset } = useExpandedAssets();
  const { setMountProfiles, toggleMountProfile } = useMountSelection(
    catalogData.assetMountStatuses,
    catalogData.applyAssetMountStatus,
    startBatchMount,
  );
  const refreshedBatchTaskRef = useRef<string | null>(null);
  const pendingBatchTasksRef = useRef(new Map<string, PendingBatchMount>());
  const latestBatchMountRef = useRef<BatchMountTaskSnapshot | null>(batchMount);
  latestBatchMountRef.current = batchMount;
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
  const assetFilterOptions = useMemo(
    () => ({
      kindFilters: [],
      query,
      sortBy: "created" as const,
      sortDirection: "desc" as const,
      sourceFilters: [],
    }),
    [query],
  );
  const filteredAssets = useAssetFilter(catalogData.assets, assetFilterOptions);
  const assetById = useMemo(() => new Map(catalogData.assets.map((asset) => [asset.id, asset])), [catalogData.assets]);
  const sourceById = useMemo(() => new Map(catalogData.sources.map((source) => [source.id, source])), [catalogData.sources]);

  useEffect(() => {
    if (!settings.showStartupNotification) {
      setNotification((current) => (current?.id === "mvp-notification-outlet" ? null : current));
    }
  }, [settings.showStartupNotification]);

  useEffect(() => {
    if (!batchMount || (batchMount.status !== "completed" && batchMount.status !== "failed" && batchMount.status !== "cancelled")) {
      return;
    }
    if (refreshedBatchTaskRef.current === batchMount.id) {
      return;
    }
    const pending = pendingBatchTasksRef.current.get(batchMount.id);
    if (!pending) {
      return;
    }
    refreshedBatchTaskRef.current = batchMount.id;
    pendingBatchTasksRef.current.delete(batchMount.id);
    void settleBatchMount(batchMount, pending);
  }, [batchMount, catalogData.refreshMountState]);

  async function settleBatchMount(task: BatchMountTaskSnapshot, pending: PendingBatchMount) {
    try {
      const refreshedStatuses = await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      if (task.status !== "completed") {
        setNotification({
          id: `mount-batch-error-${pending.profileId}-${Date.now()}`,
          tone: "error",
          messageKey: "mount.notification.failed",
          messageParams: { message: task.error?.message ?? task.status },
        });
        return;
      }

      if (pending.mode === "exclusive") {
        const result = batchResult(task);
        setNotification({
          id: `mount-group-exclusive-sync-${pending.profileId}-${Date.now()}`,
          tone: numberField(result, "skipped_count") > 0 || arrayLength(result, "errors") > 0 ? "warning" : "success",
          messageKey: "group.exclusive.result",
          messageParams: {
            profile: getProfileName(pending.profileId, catalogData.profiles),
            keep: numberField(result, "keep_count"),
            mount: numberField(result, "mount_count"),
            unmount: numberField(result, "unmount_count"),
            mounted: countMountedAssetsForProfile(refreshedStatuses, pending.profileId),
            skipped: numberField(result, "skipped_count") + arrayLength(result, "errors"),
          },
        });
        return;
      }

      const result = batchResult(task);
      const isGroup = pending.mode === "group";
      const errorCount = isGroup ? numberField(result, "error_count") : 0;
      setNotification({
        id: `mount-${pending.mode}-sync-${pending.groupId ?? pending.profileId}-${Date.now()}`,
        tone: errorCount > 0 ? "warning" : "success",
        messageKey: isGroup
          ? pending.enabled
            ? "group.mount.resultMounted"
            : "group.mount.resultUnmounted"
          : pending.enabled
            ? "mount.notification.batchMountedProfile"
            : "mount.notification.batchUnmountedProfile",
        messageParams: {
          ...(isGroup
            ? {
                updated: countAssetsForProfileState(
                  pending.assetIds,
                  refreshedStatuses,
                  pending.profileId,
                  pending.enabled ? "mounted" : "not_mounted",
                ),
                errors: errorCount,
              }
            : {
                count: countAssetsForProfileState(
                  pending.assetIds,
                  refreshedStatuses,
                  pending.profileId,
                  pending.enabled ? "mounted" : "not_mounted",
                ),
              }),
          profile: getProfileName(pending.profileId, catalogData.profiles),
          mounted: countMountedAssetsForProfile(refreshedStatuses, pending.profileId),
        },
      });
    } catch (error) {
      setNotification({
        id: `mount-batch-error-${pending.profileId}-${Date.now()}`,
        tone: "error",
        messageKey: "mount.notification.failed",
        messageParams: { message: errorMessage(error) },
      });
    }
  }

  function registerPendingBatchMount(task: BatchMountTaskSnapshot, pending: PendingBatchMount) {
    pendingBatchTasksRef.current.set(task.id, pending);
    const latest = latestBatchMountRef.current;
    const terminal = isTerminalBatchMount(task)
      ? task
      : latest?.id === task.id && isTerminalBatchMount(latest)
        ? latest
        : null;
    if (!terminal || refreshedBatchTaskRef.current === task.id) {
      return;
    }
    refreshedBatchTaskRef.current = task.id;
    pendingBatchTasksRef.current.delete(task.id);
    void settleBatchMount(terminal, pending);
  }

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
      const statuses = await catalogData.refreshCatalogAndMountState();
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
      const refreshedStatuses = await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      const mountNotification = buildAssetMountNotification({
        assetId,
        assetName: asset?.name ?? assetId,
        profileId,
        profileName: getProfileName(profileId, catalogData.profiles),
        statuses: refreshedStatuses,
      });
      setNotification({
        id: `mount-sync-${assetId}-${profileId}-${Date.now()}`,
        ...mountNotification,
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
      const task = await setMountProfiles(mountableAssetIds, profileId, enabled);
      if (task) {
        registerPendingBatchMount(task, {
          mode: "explicit",
          assetIds: mountableAssetIds,
          profileId,
          enabled,
        });
        return;
      }
      const refreshedStatuses = await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-batch-sync-${profileId}-${Date.now()}`,
        tone: "success",
        messageKey: enabled ? "mount.notification.batchMountedProfile" : "mount.notification.batchUnmountedProfile",
        messageParams: {
          count: countAssetsForProfileState(
            mountableAssetIds,
            refreshedStatuses,
            profileId,
            enabled ? "mounted" : "not_mounted",
          ),
          profile: getProfileName(profileId, catalogData.profiles),
          mounted: countMountedAssetsForProfile(refreshedStatuses, profileId),
        },
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
        const task = await startBatchMount({
          mode: "group",
          groupId,
          profileId,
          enabled,
        });
        registerPendingBatchMount(task, {
          mode: "group",
          groupId,
          assetIds,
          profileId,
          enabled,
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
      const refreshedStatuses = await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-group-preview-sync-${groupId}-${profileId}-${Date.now()}`,
        tone: "success",
        messageKey: enabled ? "group.mount.resultMounted" : "group.mount.resultUnmounted",
        messageParams: {
          updated: countAssetsForProfileState(
            mountableAssetIds,
            refreshedStatuses,
            profileId,
            enabled ? "mounted" : "not_mounted",
          ),
          profile: getProfileName(profileId, catalogData.profiles),
          mounted: countMountedAssetsForProfile(refreshedStatuses, profileId),
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
      if (isTauriRuntime()) {
        const task = await startBatchMount({
          mode: "exclusive",
          groupIds,
          profileId,
        });
        registerPendingBatchMount(task, {
          mode: "exclusive",
          groupIds,
          profileId,
        });
        return;
      }
      const result = await applySkillGroupExclusiveMount({
        group_ids: groupIds,
        profile_id: profileId,
        mount_selected: true,
        dry_run: false,
      });
      const refreshedStatuses = await catalogData.refreshMountState();
      operations.clearDeploymentPlan();
      setNotification({
        id: `mount-group-exclusive-sync-${profileId}-${Date.now()}`,
        tone: result.errors.length > 0 || result.skipped_count > 0 ? "warning" : "success",
        messageKey: "group.exclusive.result",
        messageParams: {
          profile: getProfileName(profileId, catalogData.profiles),
          keep: result.keep_count,
          mount: result.mount_count,
          unmount: result.unmount_count,
          mounted: countMountedAssetsForProfile(refreshedStatuses, profileId),
          skipped: result.skipped_count + result.errors.length,
        },
      });
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
    ...tenantController,
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

function getProfileName(profileId: string, profiles: { id: string; name: string }[]) {
  return profiles.find((profile) => profile.id === profileId)?.name ?? profileId;
}

function batchResult(task: BatchMountTaskSnapshot): Record<string, unknown> {
  return task.result && typeof task.result === "object"
    ? task.result as Record<string, unknown>
    : {};
}

function numberField(result: Record<string, unknown>, key: string): number {
  return typeof result[key] === "number" ? result[key] : 0;
}

function arrayLength(result: Record<string, unknown>, key: string): number {
  return Array.isArray(result[key]) ? result[key].length : 0;
}

function isTerminalBatchMount(task: BatchMountTaskSnapshot): boolean {
  return task.status === "completed" || task.status === "failed" || task.status === "cancelled";
}
