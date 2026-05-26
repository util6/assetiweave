import { AssetList } from "../../components/assets/AssetList";
import { AppHeader } from "../../components/layout/AppHeader";
import { AssetToolbar } from "../../components/layout/AssetToolbar";
import { SideRail } from "../../components/navigation/SideRail";
import { SubNavigation } from "../../components/navigation/SubNavigation";
import { NotificationBanner } from "../../components/notifications/NotificationBanner";
import { DashboardMetrics } from "../../components/plans/DashboardMetrics";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import { useCatalogController } from "../../hooks/catalog/useCatalogController";

export function CatalogPage() {
  const catalog = useCatalogController();
  const activeSubNavItems = catalog.navigationModel.subNavItems[catalog.navigationModel.activeHeaderTabId] ?? [];

  return (
    <div className="grid-texture flex min-h-screen bg-background text-on-surface">
      <SideRail activeId={catalog.navigationModel.activeRailId} items={catalog.navigationModel.railItems} />

      <main className="ml-sidebar-width flex min-h-screen w-[calc(100%-64px)] flex-1 flex-col">
        <AppHeader navigationModel={catalog.navigationModel} overview={catalog.overview} />
        <SubNavigation activeId={catalog.navigationModel.activeSubNavId} items={activeSubNavItems} />
        <NotificationBanner notification={catalog.notification} onDismiss={catalog.dismissNotification} />
        <AssetToolbar
          assetCount={catalog.overview?.asset_count ?? catalog.assets.length}
          busy={catalog.busy}
          hasPlan={Boolean(catalog.plan)}
          onCreatePlan={catalog.createDeploymentPlan}
          onExecutePlan={catalog.executeDeploymentPlan}
          onQueryChange={catalog.setQuery}
          onScan={catalog.scan}
          query={catalog.query}
        />

        <section className="flex flex-1 flex-col gap-4 px-8 py-6">
          <DashboardMetrics assets={catalog.assets} executionResult={catalog.executionResult} overview={catalog.overview} plan={catalog.plan} />
          <DeploymentPlanPanel plan={catalog.plan} />
          <AssetList
            appShortcuts={catalog.appShortcuts}
            assets={catalog.filteredAssets}
            expandedIds={catalog.expandedIds}
            onRevealPath={(path) => void catalog.revealPath(path)}
            onToggleAsset={catalog.toggleAsset}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            selectedMounts={catalog.selectedMounts}
          />
        </section>
      </main>
    </div>
  );
}
