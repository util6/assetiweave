import { lazy, Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { AppUpdateDialog } from "../app/updates/AppUpdateDialog";
import { useConversationSync } from "../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../app/backgroundTasks/SearchIndexProvider";
import { useSkillBackup } from "../app/backgroundTasks/SkillBackupProvider";
import { useMemoryTasks } from "../app/backgroundTasks/MemoryTaskProvider";
import { SkillBackupBackgroundTaskIndicator } from "../components/backup/SkillBackupProgress";
import { ConversationBackgroundTaskIndicator } from "../components/conversations/ConversationToolbarControls";
import { PageSkeleton, type PageSkeletonKind } from "../components/foundation/Skeleton";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { useI18n } from "../i18n/I18nProvider";
import { headerTabLabel, subNavLabel } from "../i18n/navigation";
import { AppLayout } from "../layouts/app/AppLayout";
import { UnderConstructionPage } from "../pages/under-construction/UnderConstructionPage";
import { resolveAppRoute } from "./routes";
import {
  loadCatalogPage,
  loadConversationsPage,
  loadPromptOverviewPage,
  loadSkillGroupsPage,
  loadSkillMountsPage,
  loadSourcesPage,
  preloadRoute,
} from "./routeLoaders";
import {
  RouteTransitionOverlay,
  useRouteTransition,
  type RouteTransitionKind,
} from "./RouteTransition";
import type { HeaderTabItem, NavigationModel } from "./types";
import type { SettingsPanelId } from "../store/settings/AppSettingsProvider";
import type { MemoryEvidenceSnapshot } from "../types/memory";
import {
  conversationSubNavId,
  createConversationNavigationTarget,
  type ConversationNavigationTarget,
} from "./navigationTargets";

const CatalogPage = lazy(loadCatalogPage);

const ConversationsPage = lazy(loadConversationsPage);

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

const MemoryPage = lazy(() =>
  import("../pages/memory/MemoryPage").then((module) => ({
    default: module.MemoryPage,
  })),
);

const SkillGroupsPage = lazy(loadSkillGroupsPage);

const PromptOverviewPage = lazy(loadPromptOverviewPage);

const SkillMountsPage = lazy(loadSkillMountsPage);

const SourcesPage = lazy(loadSourcesPage);

export function AppRouter() {
  const { locale, t } = useI18n();
  const { tasks: conversationSyncTasks } = useConversationSync();
  const { task: searchIndexTask } = useSearchIndex();
  const { task: skillBackupTask } = useSkillBackup();
  const { tasks: memoryTasks } = useMemoryTasks();
  const catalog = useCatalogController();
  const handledSkillBackupTaskId = useRef<string | null>(null);
  const runningSkillBackupTaskIds = useRef(new Set<string>());
  const [activeSubNavId, setActiveSubNavId] = useState(catalog.navigationModel.activeSubNavId);
  const [logViewerOpen, setLogViewerOpen] = useState(false);
  const [manualRouteKey, setManualRouteKey] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPanel, setSettingsPanel] = useState<SettingsPanelId>("general.appearance");
  const [conversationNavigationTarget, setConversationNavigationTarget] = useState<ConversationNavigationTarget | null>(null);
  const {
    completeTransition: completeRouteTransition,
    startTransition: startRouteTransition,
    transition: routeTransition,
  } = useRouteTransition();

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
    if (tab.id === catalog.navigationModel.activeHeaderTabId && nextSubNavId === activeSubNavId) {
      return;
    }
    startNavigationTransition(tab.id, nextSubNavId);
    setActiveSubNavId(nextSubNavId);
    setManualRouteKey(null);
    setConversationNavigationTarget(null);
    persistNavigationModel({
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
    if (id === activeSubNavId) {
      return;
    }
    startNavigationTransition(catalog.navigationModel.activeHeaderTabId, id);
    setManualRouteKey(null);
    setConversationNavigationTarget(null);
    setActiveSubNavId(id);
    persistNavigationModel({
      ...catalog.navigationModel,
      activeSubNavId: id,
    });
  }

  function handleMemoryEvidenceOpen(evidence: MemoryEvidenceSnapshot) {
    const target = createConversationNavigationTarget({
      blockId: evidence.block_id,
      questionId: evidence.question_id ?? undefined,
      recordKind: evidence.record_kind,
      sessionId: evidence.session_id,
    });
    const nextSubNavId = conversationSubNavId(target.recordKind);
    startNavigationTransition("conversations", nextSubNavId);
    setConversationNavigationTarget(target);
    setManualRouteKey(null);
    setActiveSubNavId(nextSubNavId);
    persistNavigationModel({
      ...catalog.navigationModel,
      activeHeaderTabId: "conversations",
      activeSubNavId: nextSubNavId,
    });
  }

  function openCurrentManual() {
    setManualRouteKey(activeRouteKey);
  }

  function handleHeaderTabPrefetch(tab: HeaderTabItem) {
    const nextSubNavId = catalog.navigationModel.subNavItems[tab.id]?.find((item) => item.enabled)?.id ?? "overview";
    preloadRoute(resolveNavigationRoute(catalog.navigationModel, tab.id, nextSubNavId));
  }

  function handleSubNavPrefetch(id: string) {
    preloadRoute(resolveNavigationRoute(catalog.navigationModel, catalog.navigationModel.activeHeaderTabId, id));
  }

  function startNavigationTransition(headerTabId: string, subNavId: string) {
    const kind = routeTransitionKind(headerTabId, subNavId, catalog.navigationModel);
    if (kind) {
      startRouteTransition(kind, t("common.loading"));
    }
  }

  function persistNavigationModel(nextNavigationModel: NavigationModel) {
    if (typeof catalog.deferNavigationModelSave === "function") {
      catalog.deferNavigationModelSave(nextNavigationModel);
      return;
    }
    void catalog.saveNavigationModel(nextNavigationModel);
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
        onHeaderTabPrefetch={handleHeaderTabPrefetch}
        onLogViewerOpen={() => setLogViewerOpen(true)}
        onNavigationModelChange={(navigationModel) => void catalog.saveNavigationModel(navigationModel)}
        onSkillBackupLibraryChange={() => catalog.refreshOverview()}
        onSettingsClose={() => setSettingsOpen(false)}
        onSettingsOpen={() => openSettings()}
        onSubNavSelect={handleSubNavSelect}
        onSubNavPrefetch={handleSubNavPrefetch}
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
          <div className="relative min-h-0 flex-1">
            <RouteTransitionOverlay transition={routeTransition} />
            {manualRouteKey ? (
              <Suspense fallback={<RouteLoadingState kind="manual" />}>
                <ManualPage routeKey={manualRouteKey} onBack={() => setManualRouteKey(null)} />
              </Suspense>
          ) : routeId === "conversations" || routeId === "web-records" ? (
            <RouteSuspense
              kind={routeId === "web-records" ? "web-records" : "conversations"}
            >
              <ConversationsPage
                activeSubNavId={activeSubNavId}
                appShortcuts={catalog.appShortcuts}
                onReady={() => completeRouteTransition(routeTransition?.id)}
                onManualOpen={openCurrentManual}
                navigationTarget={conversationNavigationTarget?.recordKind === (routeId === "web-records" ? "web" : "session")
                  ? conversationNavigationTarget
                  : null}
                onNavigationTargetConsumed={(nonce) =>
                  setConversationNavigationTarget((current) => current?.nonce === nonce ? null : current)
                }
                onNotify={(notification) => catalog.showNotification(notification)}
                onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
                onOpenSettings={openSettings}
                recordKind={routeId === "web-records" ? "web" : "session"}
              />
            </RouteSuspense>
          ) : routeId === "skill-mounts" ? (
            <RouteSuspense kind="mounts">
              <SkillMountsPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                onReady={() => completeRouteTransition(routeTransition?.id)}
                onCatalogRefresh={catalog.refreshOverview}
                onManualOpen={openCurrentManual}
                onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
                onOpenSettings={() => openSettings("general.storage")}
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
            </RouteSuspense>
          ) : routeId === "skill-groups" ? (
            <RouteSuspense kind="groups">
              <SkillGroupsPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                onReady={() => completeRouteTransition(routeTransition?.id)}
                expandedAssetIds={catalog.expandedIds}
                onManualOpen={openCurrentManual}
                onNotifyError={(message) => catalog.showNotification({ tone: "error", message })}
                onOpenSettings={() => openSettings("general.storage")}
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
            </RouteSuspense>
          ) : routeId === "prompts-overview" ? (
            <RouteSuspense kind="prompts">
              <PromptOverviewPage onManualOpen={openCurrentManual} onReady={() => completeRouteTransition(routeTransition?.id)} />
            </RouteSuspense>
          ) : routeId === "sources" ? (
            <RouteSuspense kind="sources">
              <SourcesPage
                appShortcuts={catalog.appShortcuts}
                assetMountStatuses={catalog.assetMountStatuses}
                assets={catalog.assets}
                onReady={() => completeRouteTransition(routeTransition?.id)}
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
            </RouteSuspense>
          ) : routeId === "memory" ? (
            <Suspense fallback={<RouteLoadingState kind={memorySkeletonKind(activeSubNavId)} />}>
              <MemoryPage activeSubNavId={activeSubNavId} onEvidenceOpen={handleMemoryEvidenceOpen} />
            </Suspense>
          ) : routeId === "under-construction" ? (
            <UnderConstructionPage featureLabel={underConstructionFeatureLabel} onManualOpen={openCurrentManual} routeKey={activeRouteKey} />
          ) : (
            <RouteSuspense kind="catalog">
              <CatalogPage
                catalog={catalog}
                onManualOpen={openCurrentManual}
                onOpenSettings={() => openSettings("general.appearance")}
                onReady={() => completeRouteTransition(routeTransition?.id)}
              />
            </RouteSuspense>
            )}
          </div>
        </div>
      </AppLayout>
      {logViewerOpen ? (
        <Suspense fallback={null}>
          <LogViewerModal open={logViewerOpen} onClose={() => setLogViewerOpen(false)} />
        </Suspense>
      ) : null}
      <AppUpdateDialog />
      <div className="pointer-events-none fixed bottom-5 right-5 z-30 grid gap-3">
        {searchIndexTask?.status === "running" ? (
          <div className="aurora-task-indicator rounded-xl border px-4 py-3 text-body-sm text-on-surface">
            {t("conversation.searchIndex.building")}
          </div>
        ) : null}
        {conversationSyncTasks.map((task) => (
          <ConversationBackgroundTaskIndicator key={task.id} task={task} t={t} />
        ))}
        {memoryTasks.filter((task) => task.status === "running").map((task) => (
          <div
            className="rounded-lg border border-outline-variant bg-surface-container px-4 py-3 text-body-sm text-on-surface shadow-lg"
            key={task.id}
          >
            <div className="font-medium">{t("memory.task.running")}</div>
            <div className="mt-1 text-on-surface-variant">
              {task.phase} · {task.processed_count}/{task.total_count || "?"}
            </div>
          </div>
        ))}
        <SkillBackupBackgroundTaskIndicator task={skillBackupTask} t={t} />
      </div>
    </>
  );
}

function RouteLoadingState({ kind }: { kind: PageSkeletonKind }) {
  const { t } = useI18n();

  return <PageSkeleton kind={kind} label={t("common.loading")} />;
}

function RouteSuspense({
  children,
  kind,
}: {
  children: ReactNode;
  kind: PageSkeletonKind;
}) {
  return (
    <Suspense fallback={<RouteLoadingState kind={kind} />}>
      {children}
    </Suspense>
  );
}

function memorySkeletonKind(activeSubNavId: string): "memory-library" | "memory-overview" | "memory-dreams" | "memory-recall" {
  if (activeSubNavId === "library") return "memory-library";
  if (activeSubNavId === "dreams") return "memory-dreams";
  if (activeSubNavId === "recall") return "memory-recall";
  return "memory-overview";
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function resolveNavigationRoute(navigationModel: NavigationModel, headerTabId: string, subNavId: string) {
  return resolveAppRoute({ ...navigationModel, activeHeaderTabId: headerTabId }, subNavId);
}

function routeTransitionKind(
  headerTabId: string,
  subNavId: string,
  navigationModel: NavigationModel,
): RouteTransitionKind | null {
  switch (resolveNavigationRoute(navigationModel, headerTabId, subNavId)) {
    case "catalog":
      return "catalog";
    case "conversations":
      return "conversations";
    case "prompts-overview":
      return "prompts";
    case "sources":
      return "sources";
    case "skill-groups":
      return "groups";
    case "skill-mounts":
      return "mounts";
    case "web-records":
      return "web-records";
    default:
      return null;
  }
}
