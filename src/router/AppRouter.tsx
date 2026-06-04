import { useEffect, useState } from "react";
import { LogViewerModal } from "../components/logs/LogViewerModal";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { AppLayout } from "../layouts/app/AppLayout";
import { CatalogPage } from "../pages/catalog/CatalogPage";
import { SkillGroupsPage } from "../pages/groups/SkillGroupsPage";
import { SkillMountsPage } from "../pages/mounts/SkillMountsPage";
import { SourcesPage } from "../pages/sources/SourcesPage";
import { resolveAppRoute } from "./routes";

export function AppRouter() {
  const catalog = useCatalogController();
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [logViewerOpen, setLogViewerOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
  }, [catalog.navigationModel.activeSubNavId]);

  const routeId = resolveAppRoute(catalog.navigationModel, activeSubNavId);

  return (
    <>
      <AppLayout
        activeSubNavId={activeSubNavId}
        appShortcuts={catalog.appShortcuts}
        logViewerOpen={logViewerOpen}
        navigationModel={catalog.navigationModel}
        notification={catalog.notification}
        onAppShortcutsChange={(shortcuts) => void catalog.saveAppShortcuts(shortcuts)}
        onDismissNotification={catalog.dismissNotification}
        onLogViewerOpen={() => setLogViewerOpen(true)}
        onNavigationModelChange={(navigationModel) => void catalog.saveNavigationModel(navigationModel)}
        onSkillBackupLibraryChange={() => catalog.refreshOverview()}
        onSettingsClose={() => setSettingsOpen(false)}
        onSettingsOpen={() => setSettingsOpen(true)}
        onSubNavSelect={setActiveSubNavId}
        overview={catalog.overview}
        settingsOpen={settingsOpen}
      >
        {routeId === "skill-mounts" ? (
          <SkillMountsPage
            appShortcuts={catalog.appShortcuts}
            assetMountStatuses={catalog.assetMountStatuses}
            assets={catalog.assets}
            onCatalogRefresh={catalog.refreshOverview}
            onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
            onOpenSettings={() => setSettingsOpen(true)}
            onRefreshMountStatus={catalog.refreshMountStatus}
            onRefreshProfiles={catalog.refreshProfiles}
            onRevealPath={(path) => void catalog.revealPath(path)}
            onSaveAppShortcuts={catalog.saveAppShortcuts}
            onSetSkillMountProfiles={catalog.setMountProfiles}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            refreshingMountStatus={catalog.refreshingMountStatus}
            sources={catalog.sources}
          />
        ) : routeId === "skill-groups" ? (
          <SkillGroupsPage
            appShortcuts={catalog.appShortcuts}
            assetMountStatuses={catalog.assetMountStatuses}
            assets={catalog.assets}
            expandedAssetIds={catalog.expandedIds}
            onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
            onOpenSettings={() => setSettingsOpen(true)}
            onApplyGroupExclusiveMount={catalog.applyGroupExclusiveMount}
            onPreviewGroupExclusiveMount={catalog.previewGroupExclusiveMount}
            onRefreshMountStatus={catalog.refreshMountStatus}
            onRevealPath={(path) => void catalog.revealPath(path)}
            onSetGroupMountProfile={catalog.setGroupMountProfile}
            onSetSkillMountProfiles={catalog.setMountProfiles}
            onToggleAsset={catalog.toggleAsset}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            refreshingMountStatus={catalog.refreshingMountStatus}
            sources={catalog.sources}
          />
        ) : routeId === "sources" ? (
          <SourcesPage
            appShortcuts={catalog.appShortcuts}
            assetMountStatuses={catalog.assetMountStatuses}
            assets={catalog.assets}
            expandedAssetIds={catalog.expandedIds}
            onAssetReveal={(path) => void catalog.revealPath(path)}
            onCatalogRefresh={catalog.refreshOverview}
            onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
            onOpenSettings={() => setSettingsOpen(true)}
            onRefreshMountStatus={catalog.refreshMountStatus}
            onSetSourceMountProfile={catalog.setMountProfiles}
            onToggleAsset={catalog.toggleAsset}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            refreshingMountStatus={catalog.refreshingMountStatus}
          />
        ) : (
          <CatalogPage catalog={catalog} onOpenSettings={() => setSettingsOpen(true)} />
        )}
      </AppLayout>
      <LogViewerModal open={logViewerOpen} onClose={() => setLogViewerOpen(false)} />
    </>
  );
}
