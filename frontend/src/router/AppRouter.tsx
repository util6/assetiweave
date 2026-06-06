import { useEffect, useState } from "react";
import { AppUpdateDialog } from "../app/updates/AppUpdateDialog";
import { LogViewerModal } from "../components/logs/LogViewerModal";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { useI18n } from "../i18n/I18nProvider";
import { headerTabLabel, subNavLabel } from "../i18n/navigation";
import { AppLayout } from "../layouts/app/AppLayout";
import { CatalogPage } from "../pages/catalog/CatalogPage";
import { ConversationsPage } from "../pages/conversations/ConversationsPage";
import { SkillGroupsPage } from "../pages/groups/SkillGroupsPage";
import { SkillMountsPage } from "../pages/mounts/SkillMountsPage";
import { SourcesPage } from "../pages/sources/SourcesPage";
import { UnderConstructionPage } from "../pages/under-construction/UnderConstructionPage";
import { resolveAppRoute } from "./routes";
import type { HeaderTabItem } from "./types";
import type { SettingsPanelId } from "../store/settings/AppSettingsProvider";

export function AppRouter() {
  const { locale, t } = useI18n();
  const catalog = useCatalogController();
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [logViewerOpen, setLogViewerOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPanel, setSettingsPanel] = useState<SettingsPanelId>("general.appearance");

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
  }, [catalog.navigationModel.activeSubNavId]);

  const routeId = resolveAppRoute(catalog.navigationModel, activeSubNavId);
  const activeHeaderTab = catalog.navigationModel.headerTabs.find((tab) => tab.id === catalog.navigationModel.activeHeaderTabId);
  const activeSubNavItem = catalog.navigationModel.subNavItems[catalog.navigationModel.activeHeaderTabId]?.find(
    (item) => item.id === activeSubNavId,
  );
  const activeHeaderLabel = activeHeaderTab ? headerTabLabel(activeHeaderTab, t, locale) : "";
  const activeSubNavLabel = activeSubNavItem ? subNavLabel(activeSubNavItem, t, locale) : "";
  const underConstructionFeatureLabel = [activeHeaderLabel, activeSubNavLabel].filter(Boolean).join(" / ") || undefined;
  const underConstructionRouteKey = activeSubNavItem?.routeKey ?? `${catalog.navigationModel.activeHeaderTabId}.${activeSubNavId}`;

  function handleHeaderTabSelect(tab: HeaderTabItem) {
    const nextSubNavId = catalog.navigationModel.subNavItems[tab.id]?.find((item) => item.enabled)?.id ?? "overview";
    setActiveSubNavId(nextSubNavId);
    void catalog.saveNavigationModel({
      ...catalog.navigationModel,
      activeHeaderTabId: tab.id,
      activeSubNavId: nextSubNavId,
    });
  }

  function openSettings(panel: SettingsPanelId = "general.appearance") {
    setSettingsPanel(panel);
    setSettingsOpen(true);
  }

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
        onHeaderTabSelect={handleHeaderTabSelect}
        onLogViewerOpen={() => setLogViewerOpen(true)}
        onNavigationModelChange={(navigationModel) => void catalog.saveNavigationModel(navigationModel)}
        onSkillBackupLibraryChange={() => catalog.refreshOverview()}
        onSettingsClose={() => setSettingsOpen(false)}
        onSettingsOpen={() => openSettings()}
        onSubNavSelect={setActiveSubNavId}
        overview={catalog.overview}
        settingsPanel={settingsPanel}
        settingsOpen={settingsOpen}
      >
        {routeId === "conversations" ? (
          <ConversationsPage
            activeSubNavId={activeSubNavId}
            onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
            onOpenSettings={openSettings}
          />
        ) : routeId === "skill-mounts" ? (
          <SkillMountsPage
            appShortcuts={catalog.appShortcuts}
            assetMountStatuses={catalog.assetMountStatuses}
            assets={catalog.assets}
            onCatalogRefresh={catalog.refreshOverview}
            onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
            onOpenSettings={() => openSettings("workspace.deployment")}
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
            onOpenSettings={() => openSettings("workspace.deployment")}
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
            onOpenSettings={() => openSettings("workspace.menu")}
            onRefreshMountStatus={catalog.refreshMountStatus}
            onSetSourceMountProfile={catalog.setMountProfiles}
            onToggleAsset={catalog.toggleAsset}
            onToggleMount={catalog.toggleMountProfile}
            profiles={catalog.profiles}
            refreshingMountStatus={catalog.refreshingMountStatus}
          />
        ) : routeId === "under-construction" ? (
          <UnderConstructionPage featureLabel={underConstructionFeatureLabel} routeKey={underConstructionRouteKey} />
        ) : (
          <CatalogPage catalog={catalog} onOpenSettings={() => openSettings("general.appearance")} />
        )}
      </AppLayout>
      <LogViewerModal open={logViewerOpen} onClose={() => setLogViewerOpen(false)} />
      <AppUpdateDialog />
    </>
  );
}
