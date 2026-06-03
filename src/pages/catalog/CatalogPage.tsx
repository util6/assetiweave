import { useEffect, useState } from "react";
import { Filter, Grid3X3, LayoutList, Plus, RefreshCw, Settings, SlidersHorizontal, Tag } from "lucide-react";
import { AssetDeleteDialog } from "../../components/assets/AssetDeleteDialog";
import { AssetEditDialog } from "../../components/assets/AssetEditDialog";
import { AssetList } from "../../components/assets/AssetList";
import { AssetToolbar, type AssetViewMode } from "../../components/assets/AssetToolbar";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import type { CatalogController } from "../../hooks/catalog/useCatalogController";
import { useI18n } from "../../i18n/I18nProvider";
import { deleteAsset, listSkillGroups, setSkillGroupManualMembers, updateAssetDescription } from "../../services/catalog";
import type { Asset, AssetGroupDetail } from "../../types";

export function CatalogPage({
  catalog,
  onOpenSettings,
}: {
  catalog: CatalogController;
  onOpenSettings: () => void;
}) {
  const { t } = useI18n();
  const [assetViewMode, setAssetViewMode] = useState<AssetViewMode>("list");
  const [editingAsset, setEditingAsset] = useState<Asset | null>(null);
  const [deletingAsset, setDeletingAsset] = useState<Asset | null>(null);
  const [assetGroups, setAssetGroups] = useState<AssetGroupDetail[]>([]);
  const [assetActionBusy, setAssetActionBusy] = useState(false);

  useEffect(() => {
    if (!editingAsset) {
      return;
    }

    void refreshAssetGroups();
  }, [editingAsset]);

  async function refreshAssetGroups() {
    try {
      setAssetGroups(await listSkillGroups());
    } catch (error) {
      catalog.showNotification({ tone: "error", message: errorMessage(error) });
    }
  }

  async function handleSaveAssetDescription(description: string | null) {
    if (!editingAsset) {
      return;
    }

    setAssetActionBusy(true);
    try {
      const savedAsset = await updateAssetDescription(editingAsset.id, description);
      catalog.applyAssetUpdate(savedAsset);
      catalog.clearDeploymentPlan();
      catalog.showNotification({
        tone: "success",
        messageKey: "asset.notification.updated",
        messageParams: { name: savedAsset.name },
      });
      setEditingAsset(null);
    } catch (error) {
      catalog.showNotification({ tone: "error", message: errorMessage(error) });
    } finally {
      setAssetActionBusy(false);
    }
  }

  async function handleDeleteAsset(unmount: boolean) {
    if (!deletingAsset) {
      return;
    }

    setAssetActionBusy(true);
    try {
      const deletedAsset = await deleteAsset(deletingAsset.id, unmount);
      catalog.removeAsset(deletedAsset.id);
      catalog.clearDeploymentPlan();
      catalog.showNotification({
        tone: "success",
        messageKey: "asset.notification.deleted",
        messageParams: { name: deletedAsset.name },
      });
      setDeletingAsset(null);
    } catch (error) {
      catalog.showNotification({ tone: "error", message: errorMessage(error) });
    } finally {
      setAssetActionBusy(false);
    }
  }

  async function handleSetAssetGroupMembership(group: AssetGroupDetail, enabled: boolean) {
    if (!editingAsset) {
      return;
    }

    const manualAssetIds = new Set(group.manual_asset_ids);
    if (enabled) {
      manualAssetIds.add(editingAsset.id);
    } else {
      manualAssetIds.delete(editingAsset.id);
    }

    setAssetActionBusy(true);
    try {
      const savedGroup = await setSkillGroupManualMembers(group.group.id, [...manualAssetIds]);
      setAssetGroups((current) =>
        current.map((candidate) => (candidate.group.id === savedGroup.group.id ? savedGroup : candidate)),
      );
      catalog.showNotification({
        tone: "success",
        messageKey: "asset.notification.groupUpdated",
        messageParams: { name: editingAsset.name },
      });
    } catch (error) {
      catalog.showNotification({ tone: "error", message: errorMessage(error) });
    } finally {
      setAssetActionBusy(false);
    }
  }

  async function handleToggleAssetMount(profileId: string) {
    if (!editingAsset) {
      return;
    }

    setAssetActionBusy(true);
    try {
      await catalog.toggleMountProfile(editingAsset.id, profileId);
    } finally {
      setAssetActionBusy(false);
    }
  }

  return (
    <>
      <AssetToolbar
        actionGroups={[
          [
            {
              disabled: catalog.busy,
              icon: <Plus size={22} />,
              label: t("toolbar.createDeploymentPlan"),
              onClick: catalog.createDeploymentPlan,
              primary: true,
            },
          ],
          [
            {
              disabled: catalog.busy || catalog.refreshingMountStatus,
              icon: <RefreshCw size={17} />,
              label: t("toolbar.refreshMountStatus"),
              onClick: () => void catalog.refreshMountStatus(),
            },
            { icon: <Settings size={17} />, label: t("toolbar.settings"), onClick: onOpenSettings },
          ],
        ]}
        ariaLabel={t("toolbar.aria.assetActions")}
        filters={[
          { icon: <Filter size={17} />, label: t("toolbar.filter.all", { count: catalog.assets.length }) },
          { icon: <Tag size={17} />, label: t("toolbar.filter.tag") },
          { icon: <SlidersHorizontal size={17} />, label: t("toolbar.sort.createdAt") },
        ]}
        metrics={[
          { label: t("metric.sources"), value: catalog.sources.length > 0 ? catalog.sources.length : (catalog.overview?.source_count ?? 0) },
          { label: t("metric.supportedApps"), value: catalog.profiles.length > 0 ? catalog.profiles.length : (catalog.overview?.profile_count ?? 0) },
        ]}
        onQueryChange={catalog.setQuery}
        onViewModeChange={setAssetViewMode}
        query={catalog.query}
        searchPlaceholder={t("toolbar.searchPlaceholder")}
        sticky
        viewAriaLabel={t("toolbar.view.aria")}
        viewMode={assetViewMode}
        viewOptions={[
          { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
          { icon: <Grid3X3 size={17} />, label: t("toolbar.view.grid"), value: "grid" },
        ]}
      />

      <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
        <DeploymentPlanPanel plan={catalog.plan} />
        <AssetList
          appShortcuts={catalog.appShortcuts}
          assetMountStatuses={catalog.assetMountStatuses}
          assets={catalog.filteredAssets}
          expandedIds={catalog.expandedIds}
          onDeleteAsset={setDeletingAsset}
          onEditAsset={setEditingAsset}
          onRevealPath={(path) => void catalog.revealPath(path)}
          onToggleAsset={catalog.toggleAsset}
          onToggleMount={catalog.toggleMountProfile}
          profiles={catalog.profiles}
          sources={catalog.sources}
          viewMode={assetViewMode}
        />
      </section>
      <AssetEditDialog
        asset={editingAsset}
        busy={assetActionBusy}
        groups={assetGroups}
        mountStatuses={catalog.assetMountStatuses}
        onClose={() => setEditingAsset(null)}
        onSetGroupMembership={handleSetAssetGroupMembership}
        onSubmit={handleSaveAssetDescription}
        onToggleMount={handleToggleAssetMount}
        profiles={catalog.profiles}
      />
      <AssetDeleteDialog
        asset={deletingAsset}
        busy={assetActionBusy}
        mountStatuses={catalog.assetMountStatuses}
        onClose={() => setDeletingAsset(null)}
        onConfirm={handleDeleteAsset}
      />
    </>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
