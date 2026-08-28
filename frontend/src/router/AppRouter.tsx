import { lazy, Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { AppUpdateDialog } from "../app/updates/AppUpdateDialog";
import { useConversationSync } from "../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../app/backgroundTasks/SearchIndexProvider";
import { useSkillBackup } from "../app/backgroundTasks/SkillBackupProvider";
import { useMemoryTasks } from "../app/backgroundTasks/MemoryTaskProvider";
import { SkillBackupBackgroundTaskIndicator } from "../components/backup/SkillBackupProgress";
import { ConversationsPageSkeleton } from "../components/conversations/ConversationSkeleton";
import { ConversationBackgroundTaskIndicator } from "../components/conversations/ConversationToolbarControls";
import { AppSkeleton, type SkeletonLayoutName } from "../components/foundation/skeleton";
import { useCatalogController, type CatalogController } from "../hooks/catalog/useCatalogController";
import { useI18n } from "../i18n/I18nProvider";
import { headerTabLabel, subNavLabel } from "../i18n/navigation";
import { AppLayout } from "../layouts/app/AppLayout";
import { UnderConstructionPage } from "../pages/under-construction/UnderConstructionPage";
import { resolveAppRoute, type AppRouteId } from "./routes";
import {
  loadCatalogPage,
  loadConversationsPage,
  loadLogViewerModal,
  loadManualPage,
  loadMemoryPage,
  loadPromptOverviewPage,
  loadSkillGroupsPage,
  loadSkillMountsPage,
  preloadRoute,
  loadSourcesPage,
  routeRegistry,
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

const LogViewerModal = lazy(loadLogViewerModal);
const ManualPage = lazy(loadManualPage);
const MemoryPage = lazy(loadMemoryPage);

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
            message: skillBackupTask.error?.message ?? skillBackupTask.errors[0]?.error.message ?? "Unknown error",
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
              <Suspense fallback={<RouteLoadingState layout="list" />}>
                <ManualPage routeKey={manualRouteKey} onBack={() => setManualRouteKey(null)} />
              </Suspense>
            ) : (
              routeRenderers[routeId]({
                activeRouteKey,
                activeSubNavId,
                appShortcuts: catalog.appShortcuts,
                catalog,
                conversationNavigationTarget,
                completeRouteTransition,
                handleMemoryEvidenceOpen,
                loadingLabel: t("common.loading"),
                onManualOpen: openCurrentManual,
                onOpenSettings: openSettings,
                routeTransitionId: routeTransition?.id,
                setConversationNavigationTarget,
                underConstructionFeatureLabel,
              })
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

function RouteLoadingState({ layout }: { layout: SkeletonLayoutName }) {
  const { t } = useI18n();

  return <AppSkeleton label={t("common.loading")} layout={layout} />;
}

interface RouteRenderContext {
  activeRouteKey: string;
  activeSubNavId: string;
  appShortcuts: CatalogController["appShortcuts"];
  catalog: CatalogController;
  conversationNavigationTarget: ConversationNavigationTarget | null;
  completeRouteTransition: (id?: number) => void;
  handleMemoryEvidenceOpen: (evidence: MemoryEvidenceSnapshot) => void;
  loadingLabel: string;
  onManualOpen: () => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
  routeTransitionId?: number;
  setConversationNavigationTarget: (
    updater: (current: ConversationNavigationTarget | null) => ConversationNavigationTarget | null,
  ) => void;
  underConstructionFeatureLabel?: string;
}

const routeRenderers: Record<AppRouteId, (context: RouteRenderContext) => ReactNode> = {
  catalog: (context) => (
    <RouteSuspense layout={routeRegistry.catalog.skeleton}>
      <CatalogPage
        catalog={context.catalog}
        onManualOpen={context.onManualOpen}
        onOpenSettings={() => context.onOpenSettings("general.appearance")}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
      />
    </RouteSuspense>
  ),
  conversations: (context) => renderConversationRoute(context, "session"),
  "web-records": (context) => renderConversationRoute(context, "web"),
  "skill-mounts": (context) => (
    <RouteSuspense layout={routeRegistry["skill-mounts"].skeleton}>
      <SkillMountsPage
        appShortcuts={context.appShortcuts}
        assetMountStatuses={context.catalog.assetMountStatuses}
        assets={context.catalog.assets}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
        onCatalogRefresh={context.catalog.refreshOverview}
        onManualOpen={context.onManualOpen}
        onNotifyError={(message) => context.catalog.showNotification({ tone: "error", message })}
        onOpenSettings={() => context.onOpenSettings("general.storage")}
        onRefreshMountStatus={context.catalog.refreshMountStatus}
        onRefreshProfiles={context.catalog.refreshProfiles}
        onRevealPath={(path) => void context.catalog.revealPath(path)}
        onSaveAppShortcuts={context.catalog.saveAppShortcuts}
        onSetSkillMountProfiles={context.catalog.setMountProfiles}
        onToggleMount={context.catalog.toggleMountProfile}
        profiles={context.catalog.profiles}
        refreshingMountStatus={context.catalog.refreshingMountStatus}
        sources={context.catalog.sources}
      />
    </RouteSuspense>
  ),
  "skill-groups": (context) => (
    <RouteSuspense layout={routeRegistry["skill-groups"].skeleton}>
      <SkillGroupsPage
        appShortcuts={context.appShortcuts}
        assetMountStatuses={context.catalog.assetMountStatuses}
        assets={context.catalog.assets}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
        expandedAssetIds={context.catalog.expandedIds}
        onManualOpen={context.onManualOpen}
        onNotifyError={(message) => context.catalog.showNotification({ tone: "error", message })}
        onOpenSettings={() => context.onOpenSettings("general.storage")}
        onApplyGroupExclusiveMount={context.catalog.applyGroupExclusiveMount}
        onPreviewGroupExclusiveMount={context.catalog.previewGroupExclusiveMount}
        onRefreshMountStatus={context.catalog.refreshMountStatus}
        onRevealPath={(path) => void context.catalog.revealPath(path)}
        onSetGroupMountProfile={context.catalog.setGroupMountProfile}
        onSetSkillMountProfiles={context.catalog.setMountProfiles}
        onToggleAsset={context.catalog.toggleAsset}
        onToggleMount={context.catalog.toggleMountProfile}
        profiles={context.catalog.profiles}
        refreshingMountStatus={context.catalog.refreshingMountStatus}
        sources={context.catalog.sources}
      />
    </RouteSuspense>
  ),
  "prompts-overview": (context) => (
    <RouteSuspense layout={routeRegistry["prompts-overview"].skeleton}>
      <PromptOverviewPage
        onManualOpen={context.onManualOpen}
        onNotifyError={(message) => context.catalog.showNotification({ tone: "error", message })}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
      />
    </RouteSuspense>
  ),
  sources: (context) => (
    <RouteSuspense layout={routeRegistry.sources.skeleton}>
      <SourcesPage
        appShortcuts={context.appShortcuts}
        assetMountStatuses={context.catalog.assetMountStatuses}
        assets={context.catalog.assets}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
        expandedAssetIds={context.catalog.expandedIds}
        onAssetReveal={(path) => void context.catalog.revealPath(path)}
        onApplyAssetUpdate={context.catalog.applyAssetUpdate}
        onCatalogRefresh={context.catalog.refreshOverview}
        onClearDeploymentPlan={context.catalog.clearDeploymentPlan}
        onManualOpen={context.onManualOpen}
        onNotifyError={(message) => context.catalog.showNotification({ tone: "error", message })}
        onOpenSettings={() => context.onOpenSettings("workspace.menu")}
        onRefreshMountStatus={context.catalog.refreshMountStatus}
        onRemoveAsset={context.catalog.removeAsset}
        onSetSourceMountProfile={context.catalog.setMountProfiles}
        onToggleAsset={context.catalog.toggleAsset}
        onToggleMount={context.catalog.toggleMountProfile}
        profiles={context.catalog.profiles}
        refreshingMountStatus={context.catalog.refreshingMountStatus}
      />
    </RouteSuspense>
  ),
  memory: (context) => (
    <Suspense fallback={<RouteLoadingState layout={memorySkeletonLayout(context.activeSubNavId)} />}>
      <MemoryPage activeSubNavId={context.activeSubNavId} onEvidenceOpen={context.handleMemoryEvidenceOpen} />
    </Suspense>
  ),
  "under-construction": (context) => (
    <UnderConstructionPage
      featureLabel={context.underConstructionFeatureLabel}
      onManualOpen={context.onManualOpen}
      routeKey={context.activeRouteKey}
    />
  ),
};

function renderConversationRoute(context: RouteRenderContext, recordKind: "session" | "web") {
  return (
    <Suspense fallback={<ConversationsPageSkeleton label={context.loadingLabel} />}>
      <ConversationsPage
        activeSubNavId={context.activeSubNavId}
        appShortcuts={context.appShortcuts}
        onReady={() => context.completeRouteTransition(context.routeTransitionId)}
        onManualOpen={context.onManualOpen}
        navigationTarget={context.conversationNavigationTarget?.recordKind === recordKind
          ? context.conversationNavigationTarget
          : null}
        onNavigationTargetConsumed={(nonce) =>
          context.setConversationNavigationTarget((current) => current?.nonce === nonce ? null : current)
        }
        onNotify={(notification) => context.catalog.showNotification(notification)}
        onNotifyError={(message) => context.catalog.showNotification({ tone: "error", message })}
        onOpenSettings={context.onOpenSettings}
        recordKind={recordKind}
      />
    </Suspense>
  );
}

function RouteSuspense({
  children,
  layout,
}: {
  children: ReactNode;
  layout: SkeletonLayoutName;
}) {
  return (
    <Suspense fallback={<RouteLoadingState layout={layout} />}>
      {children}
    </Suspense>
  );
}

function memorySkeletonLayout(activeSubNavId: string): SkeletonLayoutName {
  return activeSubNavId === "overview" ? "cards" : "columns";
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
  const routeId = resolveNavigationRoute(navigationModel, headerTabId, subNavId);
  const transition = routeRegistry[routeId]?.transition;
  return transition === "memory" ? memorySkeletonLayout(subNavId) : transition;
}
