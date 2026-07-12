import { useEffect, useMemo, useState } from "react";
import { ArrowDownWideNarrow, Filter, FolderOpen, Grid3X3, LayoutList, Plus, RefreshCw, Settings, Sparkles } from "lucide-react";
import { AssetDeleteDialog } from "../../components/assets/AssetDeleteDialog";
import { AssetEditDialog } from "../../components/assets/AssetEditDialog";
import { AssetList } from "../../components/assets/AssetList";
import { AssetToolbar, type AssetViewMode } from "../../components/assets/AssetToolbar";
import { ToolbarMultiSelectDropdown, ToolbarSingleSelectDropdown, ToolbarSortDirectionButton } from "../../components/common/DataToolbar";
import { PageMetrics } from "../../components/common/PageMetrics";
import { PageHeader } from "../../components/foundation/PageHeader";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import { useSkillBackup } from "../../app/backgroundTasks/SkillBackupProvider";
import type { CatalogController } from "../../hooks/catalog/useCatalogController";
import { filterAssets, type AssetSortBy, type AssetSortDirection } from "../../hooks/catalog/useAssetFilter";
import { useI18n } from "../../i18n/I18nProvider";
import { assetKindLabel } from "../../i18n/domain";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  deleteAsset,
  listSkillGroups,
  setSkillGroupManualMembers,
  updateAssetDescription,
} from "../../services/catalog";
import type { Asset, AssetGroupDetail } from "../../types";
import type { AssetKind } from "../../types";

export function CatalogPage({
  catalog,
  onManualOpen,
  onOpenSettings,
}: {
  catalog: CatalogController;
  onManualOpen: () => void;
  onOpenSettings: () => void;
}) {
  const { t } = useI18n();
  const { startBackup, task: backupTask } = useSkillBackup();
  const [assetViewMode, setAssetViewMode] = useState<AssetViewMode>("list");
  const [kindFilters, setKindFilters] = useState<AssetKind[]>([]);
  const [sourceFilters, setSourceFilters] = useState<string[]>([]);
  const [sortBy, setSortBy] = useState<AssetSortBy>("created");
  const [sortDirection, setSortDirection] = useState<AssetSortDirection>("desc");
  const [editingAsset, setEditingAsset] = useState<Asset | null>(null);
  const [deletingAsset, setDeletingAsset] = useState<Asset | null>(null);
  const [assetGroups, setAssetGroups] = useState<AssetGroupDetail[]>([]);
  const [assetActionBusy, setAssetActionBusy] = useState(false);
  const visibleAssets = useMemo(
    () =>
      filterAssets(catalog.assets, {
        kindFilters,
        query: catalog.query,
        sortBy,
        sortDirection,
        sourceFilters,
      }),
    [catalog.assets, catalog.query, kindFilters, sortBy, sortDirection, sourceFilters],
  );
  const kindFilterOptions = useMemo(
    () =>
      buildKindFilterOptions(catalog.assets).map(({ count, kind }) => ({
        label: `${assetKindLabel(kind, t)} (${count})`,
        value: kind,
      })),
    [catalog.assets, t],
  );
  const sourceFilterOptions = useMemo(() => {
    const countBySourceId = new Map<string, number>();
    catalog.assets.forEach((asset) => {
      countBySourceId.set(asset.source_id, (countBySourceId.get(asset.source_id) ?? 0) + 1);
    });

    return catalog.sources
      .filter((source) => countBySourceId.has(source.id))
      .map((source) => ({
        label: `${source.name} (${countBySourceId.get(source.id) ?? 0})`,
        value: source.id,
      }));
  }, [catalog.assets, catalog.sources]);
  const currentEditingAsset = editingAsset
    ? (catalog.assets.find((asset) => asset.id === editingAsset.id) ?? editingAsset)
    : null;

  useEffect(() => {
    if (!editingAsset) {
      return;
    }

    void refreshAssetGroups();
  }, [editingAsset]);

  useEffect(() => {
    if (editingAsset && !catalog.assets.some((asset) => asset.id === editingAsset.id)) {
      setEditingAsset(null);
    }
  }, [catalog.assets, editingAsset]);

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
      catalog.applyAssetUpdate({ ...editingAsset, ...savedAsset });
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

  async function handleBackupAsset() {
    if (!editingAsset) {
      return;
    }

    try {
      await startBackup([editingAsset.id]);
    } catch (error) {
      catalog.showNotification({ tone: "error", message: errorMessage(error) });
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
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        actions={
          <PageMetrics
            metrics={[
              {
                label: t("metric.sources"),
                value: catalog.sources.length > 0 ? catalog.sources.length : (catalog.overview?.source_count ?? 0),
              },
              {
                label: t("metric.supportedApps"),
                value: catalog.profiles.length > 0 ? catalog.profiles.length : (catalog.overview?.profile_count ?? 0),
              },
            ]}
          />
        }
        eyebrow={t("catalog.page.subtitle")}
        icon={<Sparkles size={21} />}
        title={t("catalog.page.title")}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />

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
        filterControls={
          <>
            <ToolbarMultiSelectDropdown
              allLabel={t("toolbar.filter.all", { count: catalog.assets.length })}
              ariaLabel={t("toolbar.filter.kind")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<Filter size={15} />}
              label={t("toolbar.filter.kind")}
              onClear={() => setKindFilters([])}
              onToggleValue={(value) => setKindFilters((current) => toggleFilterValue(current, value))}
              options={kindFilterOptions}
              selectedValues={kindFilters}
            />
            <ToolbarMultiSelectDropdown
              allLabel={t("toolbar.filter.sourceAll", { count: catalog.sources.length })}
              ariaLabel={t("toolbar.filter.source")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<FolderOpen size={15} />}
              label={t("toolbar.filter.source")}
              onClear={() => setSourceFilters([])}
              onToggleValue={(value) => setSourceFilters((current) => toggleFilterValue(current, value))}
              options={sourceFilterOptions}
              selectedValues={sourceFilters}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("toolbar.sort.label")}
              icon={<ArrowDownWideNarrow size={15} />}
              onChange={setSortBy}
              options={[
                { label: t("toolbar.sort.createdAt"), value: "created" },
                { label: t("toolbar.sort.updatedAt"), value: "updated" },
                { label: t("toolbar.sort.name"), value: "name" },
                { label: t("toolbar.sort.kind"), value: "kind" },
              ]}
              value={sortBy}
            />
            <ToolbarSortDirectionButton
              direction={sortDirection}
              label={t("toolbar.sort.direction.label")}
              onClick={() => setSortDirection((current) => (current === "desc" ? "asc" : "desc"))}
              title={t(sortDirection === "desc" ? "toolbar.sort.direction.descTitle" : "toolbar.sort.direction.ascTitle")}
            />
          </>
        }
        onQueryChange={catalog.setQuery}
        onViewModeChange={setAssetViewMode}
        query={catalog.query}
        searchPlaceholder={t("toolbar.searchPlaceholder")}
        searchSubmitLabel={t("toolbar.searchSubmit")}
        sticky
        stickyBleed
        viewAriaLabel={t("toolbar.view.aria")}
        viewMode={assetViewMode}
        viewOptions={[
          { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
          { icon: <Grid3X3 size={17} />, label: t("toolbar.view.grid"), value: "grid" },
        ]}
      />

      <DeploymentPlanPanel plan={catalog.plan} />
      <AssetList
        appShortcuts={catalog.appShortcuts}
        assetMountStatuses={catalog.assetMountStatuses}
        assets={visibleAssets}
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
      <AssetEditDialog
        asset={currentEditingAsset}
        backupTask={backupTask}
        busy={assetActionBusy}
        groups={assetGroups}
        mountStatuses={catalog.assetMountStatuses}
        onBackup={handleBackupAsset}
        onClose={() => setEditingAsset(null)}
        onSetGroupMembership={handleSetAssetGroupMembership}
        onSubmit={handleSaveAssetDescription}
        onToggleMount={handleToggleAssetMount}
        profiles={catalog.profiles}
        source={catalog.sources.find((source) => source.id === editingAsset?.source_id)}
      />
      <AssetDeleteDialog
        asset={deletingAsset}
        busy={assetActionBusy}
        mountStatuses={catalog.assetMountStatuses}
        onClose={() => setDeletingAsset(null)}
        onConfirm={handleDeleteAsset}
      />
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function toggleFilterValue<Value extends string>(current: Value[], value: Value) {
  if (current.includes(value)) {
    return current.filter((item) => item !== value);
  }
  return [...current, value];
}

function buildKindFilterOptions(assets: Asset[]) {
  const countByKind = new Map<AssetKind, number>();
  assets.forEach((asset) => {
    countByKind.set(asset.kind, (countByKind.get(asset.kind) ?? 0) + 1);
  });

  return [...countByKind.entries()]
    .map(([kind, count]) => ({ count, kind }))
    .sort((first, second) => first.kind.localeCompare(second.kind));
}
