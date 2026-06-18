import { useEffect, useMemo, useState } from "react";
import { useSkillBackup } from "../../app/backgroundTasks/SkillBackupProvider";
import { Columns3, DatabaseZap, DownloadCloud, FolderPlus, LayoutList, RefreshCw, Settings } from "lucide-react";
import { AssetDeleteDialog } from "../../components/assets/AssetDeleteDialog";
import { AssetEditDialog } from "../../components/assets/AssetEditDialog";
import { AssetToolbar, type AssetToolbarViewMode } from "../../components/assets/AssetToolbar";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { PageHeader } from "../../components/foundation/PageHeader";
import { SourceEditDialog } from "../../components/sources/SourceEditDialog";
import { SkillAcquireDialog } from "../../components/sources/SkillAcquireDialog";
import { SourceList } from "../../components/sources/SourceList";
import { SourceImportDialog } from "../../components/sources/SourceImportDialog";
import { SourceSummary } from "../../components/sources/SourceSummary";
import { useSourcesController } from "../../hooks/sources/useSourcesController";
import { useI18n } from "../../i18n/I18nProvider";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  deleteAsset,
  listSkillGroups,
  selectSourceDirectory,
  setSkillGroupManualMembers,
  updateAssetDescription,
} from "../../services/catalog";
import type { AppShortcut, Asset, AssetGroupDetail, AssetMountStatus, Source, TargetProfile } from "../../types";
import { getBackupableSkillAssets } from "../../utils/skillBackup";

type SourceViewMode = Extract<AssetToolbarViewMode, "list" | "columns">;

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
  const currentEditingAsset = editingAsset
    ? (assets.find((asset) => asset.id === editingAsset.id) ?? editingAsset)
    : null;
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
        onQueryChange={sources.setQuery}
        onViewModeChange={setViewMode}
        query={sources.query}
        searchClassName="flex-1"
        searchPlaceholder={t("source.toolbar.searchPlaceholder")}
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
        sources={sources.filteredSources}
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
