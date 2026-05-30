import { AssetList } from "../../components/assets/AssetList";
import { AssetToolbar } from "../../components/assets/AssetToolbar";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import type { CatalogController } from "../../hooks/catalog/useCatalogController";

export function CatalogPage({
  catalog,
  onOpenSettings,
}: {
  catalog: CatalogController;
  onOpenSettings: () => void;
}) {
  return (
    <>
      <AssetToolbar
        assetCount={catalog.overview?.asset_count ?? catalog.assets.length}
        busy={catalog.busy}
        onCreatePlan={catalog.createDeploymentPlan}
        onOpenSettings={onOpenSettings}
        onQueryChange={catalog.setQuery}
        onScan={catalog.scan}
        query={catalog.query}
        sourceCount={catalog.sources.length > 0 ? catalog.sources.length : (catalog.overview?.source_count ?? 0)}
        supportAppCount={catalog.profiles.length > 0 ? catalog.profiles.length : (catalog.overview?.profile_count ?? 0)}
      />

      <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
        <DeploymentPlanPanel plan={catalog.plan} />
        <AssetList
          appShortcuts={catalog.appShortcuts}
          assetMountStatuses={catalog.assetMountStatuses}
          assets={catalog.filteredAssets}
          expandedIds={catalog.expandedIds}
          onRevealPath={(path) => void catalog.revealPath(path)}
          onToggleAsset={catalog.toggleAsset}
          onToggleMount={catalog.toggleMountProfile}
          profiles={catalog.profiles}
          selectedMounts={catalog.selectedMounts}
          sources={catalog.sources}
        />
      </section>
    </>
  );
}
