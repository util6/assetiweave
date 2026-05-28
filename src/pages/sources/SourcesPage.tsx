import { DatabaseZap } from "lucide-react";
import { SourceList } from "../../components/sources/SourceList";
import { SourceSummary } from "../../components/sources/SourceSummary";
import { SourceToolbar } from "../../components/sources/SourceToolbar";
import { useSourcesController } from "../../hooks/sources/useSourcesController";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, TargetProfile } from "../../types";

export function SourcesPage({
  appShortcuts,
  assetMountStatuses,
  assets,
  expandedAssetIds,
  onAssetReveal,
  onCatalogRefresh,
  onToggleAsset,
  onToggleMount,
  profiles,
  selectedMounts,
}: {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  expandedAssetIds: Set<string>;
  onAssetReveal: (path: string) => void;
  onCatalogRefresh: (assets?: Asset[]) => Promise<void>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  selectedMounts: Record<string, string[]>;
}) {
  const { t } = useI18n();
  const sources = useSourcesController(assets, onCatalogRefresh);

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

      <SourceToolbar
        busy={sources.busy}
        onQueryChange={sources.setQuery}
        onScan={sources.scanAllSources}
        query={sources.query}
      />

      <SourceList
        appShortcuts={appShortcuts}
        assetMountStatuses={assetMountStatuses}
        assets={assets}
        busy={sources.busy}
        expandedAssetIds={expandedAssetIds}
        onDelete={(source) => void sources.removeSource(source, t("source.confirmDelete", { name: source.name }))}
        onAssetReveal={onAssetReveal}
        onReveal={(path) => void sources.revealPath(path)}
        onToggleAsset={onToggleAsset}
        onToggleMount={onToggleMount}
        onToggle={(source) => void sources.toggleSource(source)}
        profiles={profiles}
        selectedMounts={selectedMounts}
        sources={sources.filteredSources}
      />
    </section>
  );
}
