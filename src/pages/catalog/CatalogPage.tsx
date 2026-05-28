import { useEffect, useState } from "react";
import { AssetList } from "../../components/assets/AssetList";
import { AppHeader } from "../../components/layout/AppHeader";
import { AssetToolbar } from "../../components/layout/AssetToolbar";
import { SideRail } from "../../components/navigation/SideRail";
import { SubNavigation } from "../../components/navigation/SubNavigation";
import { NotificationBanner } from "../../components/notifications/NotificationBanner";
import { DeploymentPlanPanel } from "../../components/plans/DeploymentPlanPanel";
import { GlobalSettingsDialog } from "../../components/settings/GlobalSettingsDialog";
import { useCatalogController } from "../../hooks/catalog/useCatalogController";
import type { RailMenuItem } from "../../navigation/types";
import { SourcesPage } from "../sources/SourcesPage";

export function CatalogPage() {
  const catalog = useCatalogController();
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
  }, [catalog.navigationModel.activeSubNavId]);

  const activeSubNavItems = catalog.navigationModel.subNavItems[catalog.navigationModel.activeHeaderTabId] ?? [];
  const showSourcesPage = catalog.navigationModel.activeHeaderTabId === "skills" && activeSubNavId === "sources";

  function handleRailItemSelect(item: RailMenuItem) {
    if (item.id === "settings") {
      setSettingsOpen(true);
    }
  }

  return (
    <div className="grid-texture flex min-h-screen bg-background text-on-surface">
      <SideRail
        activeId={settingsOpen ? "settings" : catalog.navigationModel.activeRailId}
        items={catalog.navigationModel.railItems}
        onItemSelect={handleRailItemSelect}
      />

      <main className="ml-sidebar-width flex min-h-screen w-[calc(100%-64px)] flex-1 flex-col">
        <AppHeader navigationModel={catalog.navigationModel} overview={catalog.overview} />
        <SubNavigation activeId={activeSubNavId} items={activeSubNavItems} onSelect={(item) => setActiveSubNavId(item.id)} />
        <NotificationBanner notification={catalog.notification} onDismiss={catalog.dismissNotification} />

        {showSourcesPage ? (
          <SourcesPage
            appShortcuts={catalog.appShortcuts}
            assetMountStatuses={catalog.assetMountStatuses}
            assets={catalog.assets}
            expandedAssetIds={catalog.expandedIds}
            onAssetReveal={(path) => void catalog.revealPath(path)}
            onCatalogRefresh={catalog.refreshOverview}
            onToggleAsset={catalog.toggleAsset}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            selectedMounts={catalog.selectedMounts}
          />
        ) : (
          <>
            <AssetToolbar
              assetCount={catalog.overview?.asset_count ?? catalog.assets.length}
              busy={catalog.busy}
              onCreatePlan={catalog.createDeploymentPlan}
              onOpenSettings={() => setSettingsOpen(true)}
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
        )}
      </main>

      <GlobalSettingsDialog
        appShortcuts={catalog.appShortcuts}
        navigationModel={catalog.navigationModel}
        onAppShortcutsChange={(shortcuts) => void catalog.saveAppShortcuts(shortcuts)}
        onClose={() => setSettingsOpen(false)}
        onNavigationModelChange={(navigationModel) => void catalog.saveNavigationModel(navigationModel)}
        open={settingsOpen}
      />
    </div>
  );
}
