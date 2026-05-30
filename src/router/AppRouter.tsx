import { useEffect, useState } from "react";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { AppLayout } from "../layouts/app/AppLayout";
import { CatalogPage } from "../pages/catalog/CatalogPage";
import { SourcesPage } from "../pages/sources/SourcesPage";
import { resolveAppRoute } from "./routes";

export function AppRouter() {
  const catalog = useCatalogController();
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
  }, [catalog.navigationModel.activeSubNavId]);

  const routeId = resolveAppRoute(catalog.navigationModel, activeSubNavId);

  return (
    <AppLayout
      activeSubNavId={activeSubNavId}
      appShortcuts={catalog.appShortcuts}
      navigationModel={catalog.navigationModel}
      notification={catalog.notification}
      onAppShortcutsChange={(shortcuts) => void catalog.saveAppShortcuts(shortcuts)}
      onDismissNotification={catalog.dismissNotification}
      onNavigationModelChange={(navigationModel) => void catalog.saveNavigationModel(navigationModel)}
      onSettingsClose={() => setSettingsOpen(false)}
      onSettingsOpen={() => setSettingsOpen(true)}
      onSubNavSelect={setActiveSubNavId}
      overview={catalog.overview}
      settingsOpen={settingsOpen}
    >
      {routeId === "sources" ? (
        <SourcesPage
          appShortcuts={catalog.appShortcuts}
          assetMountStatuses={catalog.assetMountStatuses}
          assets={catalog.assets}
          expandedAssetIds={catalog.expandedIds}
          onAssetReveal={(path) => void catalog.revealPath(path)}
          onCatalogRefresh={catalog.refreshOverview}
          onOpenSettings={() => setSettingsOpen(true)}
          onSetSourceMountProfile={catalog.setMountProfiles}
          onToggleAsset={catalog.toggleAsset}
          onToggleMount={catalog.toggleMountProfile}
          profiles={catalog.profiles}
          selectedMounts={catalog.selectedMounts}
        />
      ) : (
        <CatalogPage catalog={catalog} onOpenSettings={() => setSettingsOpen(true)} />
      )}
    </AppLayout>
  );
}
