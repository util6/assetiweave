import { useState } from "react";
import { Filter, Grid3X3, LayoutList, Plus, RefreshCw, Settings, SlidersHorizontal, Tag } from "lucide-react";
import { AssetList } from "../../components/assets/AssetList";
import { AssetToolbar, type AssetViewMode } from "../../components/assets/AssetToolbar";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import type { CatalogController } from "../../hooks/catalog/useCatalogController";
import { useI18n } from "../../i18n/I18nProvider";

export function CatalogPage({
  catalog,
  onOpenSettings,
}: {
  catalog: CatalogController;
  onOpenSettings: () => void;
}) {
  const { t } = useI18n();
  const [assetViewMode, setAssetViewMode] = useState<AssetViewMode>("list");

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
          onRevealPath={(path) => void catalog.revealPath(path)}
          onToggleAsset={catalog.toggleAsset}
          onToggleMount={catalog.toggleMountProfile}
          profiles={catalog.profiles}
          sources={catalog.sources}
          viewMode={assetViewMode}
        />
      </section>
    </>
  );
}
