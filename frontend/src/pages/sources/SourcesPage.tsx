import { useEffect, useMemo, useState } from "react";
import { useSkillBackup } from "../../app/backgroundTasks/SkillBackupProvider";
import { ArrowDownWideNarrow, Columns3, DatabaseZap, DownloadCloud, Filter, FolderPlus, LayoutList, Power, RefreshCw, Settings } from "lucide-react";
import { AssetDeleteDialog } from "../../components/assets/AssetDeleteDialog";
import { AssetEditDialog } from "../../components/assets/AssetEditDialog";
import { AssetToolbar, type AssetToolbarViewMode } from "../../components/assets/AssetToolbar";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { ToolbarMultiSelectDropdown, ToolbarSingleSelectDropdown, ToolbarSortDirectionButton } from "../../components/common/DataToolbar";
import { PageHeader } from "../../components/foundation/PageHeader";
import { SourceEditDialog } from "../../components/sources/SourceEditDialog";
import { SkillAcquireDialog } from "../../components/sources/SkillAcquireDialog";
import { SourceList } from "../../components/sources/SourceList";
import { SourceImportDialog } from "../../components/sources/SourceImportDialog";
import { SourceSummary } from "../../components/sources/SourceSummary";
import { useSourcesController } from "../../hooks/sources/useSourcesController";
import { sourceKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  deleteAsset,
  listSkillGroups,
  selectSourceDirectory,
  setSkillGroupManualMembers,
  updateAssetDescription,
} from "../../services/catalog";
import type { AppShortcut, Asset, AssetGroupDetail, AssetMountStatus, Source, SourceKind, TargetProfile } from "../../types";
import { getBackupableSkillAssets } from "../../utils/skillBackup";

type SourceViewMode = Extract<AssetToolbarViewMode, "list" | "columns">;
type SourceStatusFilter = "enabled" | "disabled" | "issue";
type SourceSortBy = "priority" | "name" | "asset-count" | "last-scanned";

export function SourcesPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  expandedAssetIds,
  onAssetReveal,
  onApplyAssetUpdate,
  onCatalogRefresh,
  onClearDeploymentPlan,
  onManualOpen,
  onNotifyError,
  onOpenSettings,
  onRefreshMountStatus,
  onRemoveAsset,
  onSetSourceMountProfile,
  onToggleAsset,
  onToggleMount,
  profiles,
  refreshingMountStatus,
}: {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  expandedAssetIds: Set<string>;
  onAssetReveal: (path: string) => void;
  onApplyAssetUpdate: (asset: Asset) => void;
  onCatalogRefresh: (assets?: Asset[]) => Promise<void>;
  onClearDeploymentPlan: () => void;
  onManualOpen: () => void;
  onNotifyError: (message: string) => void;
  onOpenSettings: () => void;
  onRefreshMountStatus: () => Promise<void>;
  onRemoveAsset: (assetId: string) => void;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  refreshingMountStatus: boolean;
}) {
  const { t } = useI18n();
  const { startBackup, task: backupTask } = useSkillBackup();
  const sources = useSourcesController(assets, onCatalogRefresh);
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [acquireDialogOpen, setAcquireDialogOpen] = useState(false);
  const [editingSource, setEditingSource] = useState<Source | null>(null);
  const [deletingSource, setDeletingSource] = useState<Source | null>(null);
  const [editingAsset, setEditingAsset] = useState<Asset | null>(null);
  const [deletingAsset, setDeletingAsset] = useState<Asset | null>(null);
  const [assetGroups, setAssetGroups] = useState<AssetGroupDetail[]>([]);
  const [assetActionBusy, setAssetActionBusy] = useState(false);
  const [viewMode, setViewMode] = useState<SourceViewMode>("list");
  const [kindFilters, setKindFilters] = useState<SourceKind[]>([]);
  const [statusFilters, setStatusFilters] = useState<SourceStatusFilter[]>([]);
  const [sortBy, setSortBy] = useState<SourceSortBy>("priority");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("asc");
  const currentEditingAsset = editingAsset
    ? (assets.find((asset) => asset.id === editingAsset.id) ?? editingAsset)
    : null;
  const visibleSources = useMemo(
    () =>
      filterAndSortSources({
        assetCounts: sources.assetCounts,
        kindFilters,
        sortBy,
        sortDirection,
        sources: sources.filteredSources,
        statusFilters,
      }),
    [kindFilters, sortBy, sortDirection, sources.assetCounts, sources.filteredSources, statusFilters],
  );
  const sourceKindOptions = useMemo(() => {
    const countByKind = new Map<SourceKind, number>();
    sources.sources.forEach((source) => countByKind.set(source.kind, (countByKind.get(source.kind) ?? 0) + 1));
    return [...countByKind.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([kind, count]) => ({ label: `${sourceKindLabel(kind, t)} (${count})`, value: kind }));
  }, [sources.sources, t]);
  const sourceStatusOptions = useMemo(
    () => [
      {
        label: t("toolbar.filter.enabled", { count: sources.sources.filter((source) => source.enabled).length }),
        value: "enabled" as const,
      },
      {
        label: t("toolbar.filter.disabled", { count: sources.sources.filter((source) => !source.enabled).length }),
        value: "disabled" as const,
      },
      {
        label: t("toolbar.filter.issue", {
          count: sources.sources.filter((source) => hasSourceIssue(source)).length,
        }),
        value: "issue" as const,
      },
    ],
    [sources.sources, t],
  );
  const sourceBackupAssets = useMemo(() => {
    if (!editingSource) {
      return [];
    }
    return getBackupableSkillAssets(assets.filter((asset) => asset.source_id === editingSource.id));
  }, [assets, editingSource]);

  useEffect(() => {
    if (!editingAsset) {
      return;
    }

    void refreshAssetGroups();
  }, [editingAsset]);

  useEffect(() => {
    if (editingAsset && !assets.some((asset) => asset.id === editingAsset.id)) {
      setEditingAsset(null);
    }
  }, [assets, editingAsset]);

  async function refreshAssetGroups() {
    try {
      setAssetGroups(await listSkillGroups());
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleDeleteSource() {
    if (!deletingSource) {
      return;
    }
    if (isProtectedSource(deletingSource)) {
      onNotifyError(t("source.delete.protected"));
      setDeletingSource(null);
      return;
    }

    try {
      await sources.removeSource(deletingSource);
      setDeletingSource(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  async function handleSaveAssetDescription(description: string | null) {
    if (!editingAsset) {
      return;
    }

    setAssetActionBusy(true);
    try {
      const savedAsset = await updateAssetDescription(editingAsset.id, description);
      onApplyAssetUpdate({ ...editingAsset, ...savedAsset });
      onClearDeploymentPlan();
      setEditingAsset(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
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
      onRemoveAsset(deletedAsset.id);
      onClearDeploymentPlan();
      setDeletingAsset(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
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
      onNotifyError(errorMessage(error));
    }
  }

  async function handleBackupSourceAssets() {
    if (sourceBackupAssets.length === 0) {
      return;
    }

    try {
      await startBackup(sourceBackupAssets.map((asset) => asset.id));
    } catch (error) {
      onNotifyError(errorMessage(error));
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
    } catch (error) {
      onNotifyError(errorMessage(error));
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
      await onToggleMount(editingAsset.id, profileId);
    } finally {
      setAssetActionBusy(false);
    }
  }

  async function handleSaveSource(source: Source) {
    try {
      await sources.saveSource(source);
      setEditingSource(null);
    } catch (error) {
      onNotifyError(errorMessage(error));
    }
  }

  return (
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        actions={
          <SourceSummary
            assets={sources.summary.assets}
            enabled={sources.summary.enabled}
            issues={sources.summary.issues}
            total={sources.summary.total}
          />
        }
        eyebrow={t("source.page.subtitle")}
        icon={<DatabaseZap size={21} />}
        title={t("source.page.title")}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />

      <AssetToolbar
        actionGroups={[
          [
            {
              disabled: sources.busy,
              icon: <FolderPlus size={17} />,
              label: t("source.toolbar.add"),
              onClick: () => setImportDialogOpen(true),
              primary: true,
              text: t("source.toolbar.add"),
            },
            {
              disabled: sources.busy,
              icon: <DownloadCloud size={17} />,
              label: t("source.toolbar.discover"),
              onClick: () => setAcquireDialogOpen(true),
              text: t("source.toolbar.discover"),
            },
          ],
          [
            {
              disabled: sources.busy || refreshingMountStatus,
              icon: <RefreshCw size={17} />,
              label: t("toolbar.refreshMountStatus"),
              onClick: () => void onRefreshMountStatus(),
            },
            { icon: <Settings size={17} />, label: t("toolbar.settings"), onClick: onOpenSettings },
          ],
        ]}
        ariaLabel={t("source.page.title")}
        filterControls={
          <>
            <ToolbarMultiSelectDropdown
              allLabel={t("toolbar.filter.kindAll", { count: sources.sources.length })}
              ariaLabel={t("source.toolbar.kindFilter")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<Filter size={15} />}
              label={t("source.toolbar.kindFilter")}
              onClear={() => setKindFilters([])}
              onToggleValue={(value) => setKindFilters((current) => toggleFilterValue(current, value))}
              options={sourceKindOptions}
              selectedValues={kindFilters}
            />
            <ToolbarMultiSelectDropdown
              allLabel={t("toolbar.filter.statusAll", { count: sources.sources.length })}
              ariaLabel={t("source.toolbar.statusFilter")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<Power size={15} />}
              label={t("source.toolbar.statusFilter")}
              onClear={() => setStatusFilters([])}
              onToggleValue={(value) => setStatusFilters((current) => toggleFilterValue(current, value))}
              options={sourceStatusOptions}
              selectedValues={statusFilters}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("toolbar.sort.label")}
              icon={<ArrowDownWideNarrow size={15} />}
              onChange={setSortBy}
              options={[
                { label: t("source.toolbar.sort.priority"), value: "priority" },
                { label: t("toolbar.sort.name"), value: "name" },
                { label: t("source.toolbar.sort.assetCount"), value: "asset-count" },
                { label: t("source.toolbar.sort.lastScanned"), value: "last-scanned" },
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
        onQueryChange={sources.setQuery}
        onViewModeChange={setViewMode}
        query={sources.query}
        searchClassName="flex-1"
        searchPlaceholder={t("source.toolbar.searchPlaceholder")}
        searchSubmitLabel={t("source.toolbar.searchSubmit")}
        sticky
        stickyBleed
        viewAriaLabel={t("toolbar.view.aria")}
        viewMode={viewMode}
        viewOptions={[
          { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
          { icon: <Columns3 size={17} />, label: t("toolbar.view.columns"), value: "columns" },
        ]}
      />

      <SourceList
        appShortcuts={appShortcuts}
        assetMountStatuses={assetMountStatuses}
        assets={assets}
        busy={sources.busy}
        expandedAssetIds={expandedAssetIds}
        onDelete={setDeletingSource}
        onDeleteAsset={setDeletingAsset}
        onEdit={setEditingSource}
        onEditAsset={setEditingAsset}
        onAssetReveal={onAssetReveal}
        onReveal={(path) => void sources.revealPath(path)}
        onSetSourceMountProfile={(assetIds, profileId, enabled) =>
          void onSetSourceMountProfile(assetIds, profileId, enabled)
        }
        onToggleAsset={onToggleAsset}
        onToggleMount={onToggleMount}
        profiles={profiles}
        sources={visibleSources}
        viewMode={viewMode}
      />

      <SourceImportDialog
        busy={sources.busy}
        onClose={() => setImportDialogOpen(false)}
        onNotifyError={onNotifyError}
        onPickRootPath={() => selectSourceDirectory(t("source.import.dialogTitle"))}
        onSubmit={sources.importSource}
        open={importDialogOpen}
        suggestedPriority={sources.nextPriority}
      />
      <SkillAcquireDialog
        onAcquired={sources.scanAllSources}
        onClose={() => setAcquireDialogOpen(false)}
        onNotifyError={onNotifyError}
        open={acquireDialogOpen}
      />
      <SourceEditDialog
        backupAssetIds={sourceBackupAssets.map((asset) => asset.id)}
        backupAssetCount={sourceBackupAssets.length}
        backupTask={backupTask}
        busy={sources.busy || assetActionBusy}
        onClose={() => setEditingSource(null)}
        onBackup={handleBackupSourceAssets}
        onNotifyError={onNotifyError}
        onPickRootPath={() => selectSourceDirectory(t("source.import.dialogTitle"))}
        onSubmit={handleSaveSource}
        source={editingSource}
      />
      <ConfirmDialog
        busy={sources.busy}
        confirmLabel={t("common.delete")}
        message={deletingSource ? t("source.deleteDialog.message", { name: deletingSource.name }) : ""}
        onClose={() => setDeletingSource(null)}
        onConfirm={() => void handleDeleteSource()}
        open={Boolean(deletingSource)}
        title={t("source.deleteDialog.title")}
        tone="danger"
      >
        <div className="rounded-xl border border-theme-card-border bg-theme-card/65 p-3 text-body-sm text-on-surface-variant">
          {t("source.deleteDialog.detail")}
        </div>
      </ConfirmDialog>
      <AssetEditDialog
        asset={currentEditingAsset}
        backupTask={backupTask}
        busy={assetActionBusy}
        groups={assetGroups}
        mountStatuses={assetMountStatuses}
        onBackup={handleBackupAsset}
        onClose={() => setEditingAsset(null)}
        onSetGroupMembership={handleSetAssetGroupMembership}
        onSubmit={handleSaveAssetDescription}
        onToggleMount={handleToggleAssetMount}
        profiles={profiles}
        source={sources.sources.find((source) => source.id === editingAsset?.source_id)}
      />
      <AssetDeleteDialog
        asset={deletingAsset}
        busy={assetActionBusy}
        mountStatuses={assetMountStatuses}
        onClose={() => setDeletingAsset(null)}
        onConfirm={handleDeleteAsset}
      />
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isProtectedSource(source: Source) {
  return source.id === "assetiweave-library-skills" || source.source_origin === "assetiweave_library";
}

function filterAndSortSources({
  assetCounts,
  kindFilters,
  sortBy,
  sortDirection,
  sources,
  statusFilters,
}: {
  assetCounts: Record<string, number>;
  kindFilters: SourceKind[];
  sortBy: SourceSortBy;
  sortDirection: "asc" | "desc";
  sources: Source[];
  statusFilters: SourceStatusFilter[];
}) {
  const kindSet = new Set(kindFilters);
  const statusSet = new Set(statusFilters);

  return sources
    .filter((source) => {
      if (kindSet.size > 0 && !kindSet.has(source.kind)) {
        return false;
      }
      if (statusSet.size === 0) {
        return true;
      }
      return (
        (statusSet.has("enabled") && source.enabled) ||
        (statusSet.has("disabled") && !source.enabled) ||
        (statusSet.has("issue") && hasSourceIssue(source))
      );
    })
    .sort((left, right) => compareSources(left, right, sortBy, sortDirection, assetCounts));
}

function compareSources(
  left: Source,
  right: Source,
  sortBy: SourceSortBy,
  sortDirection: "asc" | "desc",
  assetCounts: Record<string, number>,
) {
  const direction = sortDirection === "asc" ? 1 : -1;
  let primary = 0;

  if (sortBy === "priority") {
    primary = left.priority - right.priority;
  } else if (sortBy === "asset-count") {
    primary = (assetCounts[left.id] ?? 0) - (assetCounts[right.id] ?? 0);
  } else if (sortBy === "last-scanned") {
    primary = compareOptionalDate(left.last_scanned_at, right.last_scanned_at);
  } else {
    primary = left.name.localeCompare(right.name);
  }

  if (primary !== 0) {
    return primary * direction;
  }

  return left.name.localeCompare(right.name) || left.id.localeCompare(right.id);
}

function hasSourceIssue(source: Source) {
  return source.last_scan_status?.startsWith("error:") ?? false;
}

function compareOptionalDate(left: string | null | undefined, right: string | null | undefined) {
  const leftTime = left ? Date.parse(left) : Number.NaN;
  const rightTime = right ? Date.parse(right) : Number.NaN;
  if (!Number.isFinite(leftTime) && !Number.isFinite(rightTime)) return 0;
  if (!Number.isFinite(leftTime)) return -1;
  if (!Number.isFinite(rightTime)) return 1;
  return leftTime - rightTime;
}

function toggleFilterValue<Value extends string>(current: Value[], value: Value) {
  if (current.includes(value)) {
    return current.filter((item) => item !== value);
  }
  return [...current, value];
}
