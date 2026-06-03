import { useState } from "react";
import { Columns3, DatabaseZap, FolderPlus, LayoutList, RefreshCw, Settings } from "lucide-react";
import { AssetToolbar, type AssetToolbarViewMode } from "../../components/assets/AssetToolbar";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { SourceEditDialog } from "../../components/sources/SourceEditDialog";
import { SourceList } from "../../components/sources/SourceList";
import { SourceImportDialog } from "../../components/sources/SourceImportDialog";
import { SourceSummary } from "../../components/sources/SourceSummary";
import { useSourcesController } from "../../hooks/sources/useSourcesController";
import { useI18n } from "../../i18n/I18nProvider";
import { selectSourceDirectory } from "../../services/catalog";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";

type SourceViewMode = Extract<AssetToolbarViewMode, "list" | "columns">;

export function SourcesPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  expandedAssetIds,
  onAssetReveal,
  onCatalogRefresh,
  onNotifyError,
  onOpenSettings,
  onRefreshMountStatus,
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
  onCatalogRefresh: (assets?: Asset[]) => Promise<void>;
  onNotifyError: (message: string) => void;
  onOpenSettings: () => void;
  onRefreshMountStatus: () => Promise<void>;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => Promise<void>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  refreshingMountStatus: boolean;
}) {
  const { t } = useI18n();
  const sources = useSourcesController(assets, onCatalogRefresh);
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [editingSource, setEditingSource] = useState<Source | null>(null);
  const [deletingSource, setDeletingSource] = useState<Source | null>(null);
  const [viewMode, setViewMode] = useState<SourceViewMode>("list");

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
      <div className="flex items-start justify-between gap-4 max-[920px]:flex-col">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-status-update">
            <DatabaseZap size={21} />
            <span className="text-label-caps uppercase">{t("source.page.subtitle")}</span>
          </div>
          <h1 className="mt-1 text-h2 text-on-surface">{t("source.page.title")}</h1>
        </div>
        <div className="w-full max-w-3xl">
          <SourceSummary
            assets={sources.summary.assets}
            enabled={sources.summary.enabled}
            issues={sources.summary.issues}
            total={sources.summary.total}
          />
        </div>
      </div>

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
        onEdit={setEditingSource}
        onAssetReveal={onAssetReveal}
        onReveal={(path) => void sources.revealPath(path)}
        onSetSourceMountProfile={(assetIds, profileId, enabled) =>
          void onSetSourceMountProfile(assetIds, profileId, enabled)
        }
        onToggleAsset={onToggleAsset}
        onToggleMount={onToggleMount}
        onToggle={(source) => void sources.toggleSource(source)}
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
      <SourceEditDialog
        busy={sources.busy}
        onClose={() => setEditingSource(null)}
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
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isProtectedSource(source: Source) {
  return source.id === "assetiweave-library-skills" || source.source_origin === "assetiweave_library";
}
