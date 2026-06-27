import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { AppUpdateDialog } from "../app/updates/AppUpdateDialog";
import { useConversationSync } from "../app/backgroundTasks/ConversationSyncProvider";
import { useSkillBackup } from "../app/backgroundTasks/SkillBackupProvider";
import { SkillBackupBackgroundTaskIndicator } from "../components/backup/SkillBackupProgress";
import { ConversationBackgroundTaskIndicator } from "../components/conversations/ConversationToolbarControls";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { useI18n } from "../i18n/I18nProvider";
import { headerTabLabel, subNavLabel } from "../i18n/navigation";
import { AppLayout } from "../layouts/app/AppLayout";
import { UnderConstructionPage } from "../pages/under-construction/UnderConstructionPage";
import { resolveAppRoute } from "./routes";
import type { HeaderTabItem } from "./types";
import type { SettingsPanelId } from "../store/settings/AppSettingsProvider";

const CatalogPage = lazy(() =>
  import("../pages/catalog/CatalogPage").then((module) => ({
    default: module.CatalogPage,
  })),
);

const ConversationsPage = lazy(() =>
  import("../pages/conversations/ConversationsPage").then((module) => ({
    default: module.ConversationsPage,
  })),
);

const LogViewerModal = lazy(() =>
  import("../components/logs/LogViewerModal").then((module) => ({
    default: module.LogViewerModal,
  })),
);

const ManualPage = lazy(() =>
  import("../manuals/ManualPage").then((module) => ({
    default: module.ManualPage,
  })),
);

const SkillGroupsPage = lazy(() =>
  import("../pages/groups/SkillGroupsPage").then((module) => ({
    default: module.SkillGroupsPage,
  })),
);

const SkillMountsPage = lazy(() =>
  import("../pages/mounts/SkillMountsPage").then((module) => ({
    default: module.SkillMountsPage,
  })),
);

const SourcesPage = lazy(() =>
  import("../pages/sources/SourcesPage").then((module) => ({
    default: module.SourcesPage,
  })),
);

export function AppRouter() {
  const { locale, t } = useI18n();
  const { task: conversationSyncTask } = useConversationSync();
  const { task: skillBackupTask } = useSkillBackup();
  const catalog = useCatalogController();
  const handledSkillBackupTaskId = useRef<string | null>(null);
  const runningSkillBackupTaskIds = useRef(new Set<string>());
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [logViewerOpen, setLogViewerOpen] = useState(false);
  const [manualRouteKey, setManualRouteKey] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPanel, setSettingsPanel] = useState<SettingsPanelId>("general.appearance");

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
    setManualRouteKey(null);
  }, [catalog.navigationModel.activeHeaderTabId, catalog.navigationModel.activeSubNavId]);

  useEffect(() => {
    if (!skillBackupTask) {
      return;
    }
    if (skillBackupTask.status === "running") {
      runningSkillBackupTaskIds.current.add(skillBackupTask.id);
      return;
    }
    if (
      !runningSkillBackupTaskIds.current.has(skillBackupTask.id) ||
      handledSkillBackupTaskId.current === skillBackupTask.id
    ) {
      return;
    }

    handledSkillBackupTaskId.current = skillBackupTask.id;
    runningSkillBackupTaskIds.current.delete(skillBackupTask.id);
    void (async () => {
      try {
        await catalog.refreshOverview();
        catalog.clearDeploymentPlan();
      } catch (error) {
        if (skillBackupTask.status === "completed") {
          catalog.showNotification({
            tone: "error",
            messageKey: "backup.notification.failed",
            messageParams: { message: errorMessage(error) },
          });
          return;
        }
      }

      if (skillBackupTask.status === "failed") {
        catalog.showNotification({
          tone: "error",
          messageKey: "backup.notification.failed",
          messageParams: {
            message: skillBackupTask.error ?? skillBackupTask.errors[0]?.message ?? "Unknown error",
          },
        });
        return;
      }

      catalog.showNotification({
        tone: "success",
        messageKey: "backup.notification.batchCompleted",
        messageParams: { count: skillBackupTask.completed_count },
      });
    })();
  }, [skillBackupTask?.id, skillBackupTask?.status]);

  const routeId = resolveAppRoute(catalog.navigationModel, activeSubNavId);
  const activeHeaderTab = catalog.navigationModel.headerTabs.find((tab) => tab.id === catalog.navigationModel.activeHeaderTabId);
  const activeSubNavItem = catalog.navigationModel.subNavItems[catalog.navigationModel.activeHeaderTabId]?.find(
    (item) => item.id === activeSubNavId,
  );
  const activeHeaderLabel = activeHeaderTab ? headerTabLabel(activeHeaderTab, t, locale) : "";
  const activeSubNavLabel = activeSubNavItem ? subNavLabel(activeSubNavItem, t, locale) : "";
  const underConstructionFeatureLabel = [activeHeaderLabel, activeSubNavLabel].filter(Boolean).join(" / ") || undefined;
  const activeRouteKey = activeSubNavItem?.routeKey ?? `${catalog.navigationModel.activeHeaderTabId}.${activeSubNavId}`;
  const tenantRouteKey = catalog.activeTenant?.id ?? "tenant-loading";

  function handleHeaderTabSelect(tab: HeaderTabItem) {
    const nextSubNavId = catalog.navigationModel.subNavItems[tab.id]?.find((item) => item.enabled)?.id ?? "overview";
    setActiveSubNavId(nextSubNavId);
    setManualRouteKey(null);
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

  function handleSubNavSelect(id: string) {
    setManualRouteKey(null);
    setActiveSubNavId(id);
  }

  function openCurrentManual() {
    setManualRouteKey(activeRouteKey);
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
        onSubNavSelect={handleSubNavSelect}
        settingsPanel={settingsPanel}
        settingsOpen={settingsOpen}
        tenantControls={{
          activeTenant: catalog.activeTenant,
          busy: catalog.tenantBusy,
          error: catalog.error,
          loading: catalog.loading,
          onCreateTenant: catalog.createLocalTenant,
          onSwitchTenant: catalog.switchActiveTenant,
          tenants: catalog.tenants,
        }}
      >
        <div className="contents" key={tenantRouteKey}>
          {manualRouteKey ? (
            <Suspense fallback={<RouteLoadingState />}>
              <ManualPage routeKey={manualRouteKey} onBack={() => setManualRouteKey(null)} />
            </Suspense>
          ) : routeId === "conversations" || routeId === "web-records" ? (
            <Suspense fallback={<RouteLoadingState />}>
              <ConversationsPage
                activeSubNavId={activeSubNavId}
                appShortcuts={catalog.appShortcuts}
                onManualOpen={openCurrentManual}
                onNotify={(notification) => catalog.showNotification(notification)}
                onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
                onOpenSettings={openSettings}
                recordKind={routeId === "web-records" ? "web" : "session"}
              />
            </Suspense>
          ) : routeId === "skill-mounts" ? (
            <Suspense fallback={<RouteLoadingState />}>
              <SkillMountsPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                onCatalogRefresh={catalog.refreshOverview}
                onManualOpen={openCurrentManual}
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
            </Suspense>
          ) : routeId === "skill-groups" ? (
            <Suspense fallback={<RouteLoadingState />}>
              <SkillGroupsPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                expandedAssetIds={catalog.expandedIds}
                onManualOpen={openCurrentManual}
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
            </Suspense>
          ) : routeId === "sources" ? (
            <Suspense fallback={<RouteLoadingState />}>
              <SourcesPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                expandedAssetIds={catalog.expandedIds}
                onAssetReveal={(path) => void catalog.revealPath(path)}
                onApplyAssetUpdate={catalog.applyAssetUpdate}
                onCatalogRefresh={catalog.refreshOverview}
                onClearDeploymentPlan={catalog.clearDeploymentPlan}
                onManualOpen={openCurrentManual}
                onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
                onOpenSettings={() => openSettings("workspace.menu")}
                onRefreshMountStatus={catalog.refreshMountStatus}
                onRemoveAsset={catalog.removeAsset}
                onSetSourceMountProfile={catalog.setMountProfiles}
                onToggleAsset={catalog.toggleAsset}
                onToggleMount={catalog.toggleMountProfile}
                profiles={catalog.profiles}
                refreshingMountStatus={catalog.refreshingMountStatus}
              />
            </Suspense>
          ) : routeId === "under-construction" ? (
            <UnderConstructionPage featureLabel={underConstructionFeatureLabel} onManualOpen={openCurrentManual} routeKey={activeRouteKey} />
          ) : (
            <Suspense fallback={<RouteLoadingState />}>
              <CatalogPage catalog={catalog} onManualOpen={openCurrentManual} onOpenSettings={() => openSettings("general.appearance")} />
            </Suspense>
          )}
        </div>
      </AppLayout>
      {logViewerOpen ? (
        <Suspense fallback={null}>
          <LogViewerModal open={logViewerOpen} onClose={() => setLogViewerOpen(false)} />
        </Suspense>
      ) : null}
      <AppUpdateDialog />
      <div className="pointer-events-none fixed bottom-5 right-5 z-30 grid gap-3">
        <ConversationBackgroundTaskIndicator task={conversationSyncTask} t={t} />
        <SkillBackupBackgroundTaskIndicator task={skillBackupTask} t={t} />
      </div>
    </>
  );
}

function RouteLoadingState() {
  const { t } = useI18n();

  return <div className="grid min-h-[320px] place-items-center text-body-sm text-on-surface-variant">{t("common.loading")}</div>;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
