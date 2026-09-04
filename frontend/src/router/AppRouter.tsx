import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { RouterProvider } from "@tanstack/react-router";
import { AppUpdateDialog } from "../app/updates/AppUpdateDialog";
import { useConversationSync } from "../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../app/backgroundTasks/SearchIndexProvider";
import { useSkillBackup } from "../app/backgroundTasks/SkillBackupProvider";
import { useMemoryTasks } from "../app/backgroundTasks/MemoryTaskProvider";
import { useOptionalTeamTasks } from "../app/backgroundTasks/TeamTaskProvider";
import { SkillBackupBackgroundTaskIndicator } from "../components/backup/SkillBackupProgress";
import { ConversationBackgroundTaskIndicator } from "../components/conversations/ConversationToolbarControls";
import { AppSkeleton } from "../components/foundation/skeleton";
import { useCatalogController } from "../hooks/catalog/useCatalogController";
import { useI18n } from "../i18n/I18nProvider";
import { AppLayout } from "../layouts/app/AppLayout";
import { createAppRouter } from "./createAppRouter";
import { navigationPath } from "./navigationPath";
import {
  WorkspaceContext,
  type WorkspaceContextValue,
} from "./WorkspaceContext";
import type { HeaderTabItem, NavigationModel } from "./types";
import type { SettingsPanelId } from "../store/settings/settingsSchema";
import type { MemoryNavigationTarget } from "../types/memory";
import {
  conversationSubNavId,
  createConversationNavigationTarget,
  type ConversationNavigationTarget,
} from "./navigationTargets";
import { useAppUiStore } from "../store/ui/appUiStore";

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

export function AppRouter() {
  const { t } = useI18n();
  const { tasks: conversationSyncTasks } = useConversationSync();
  const { task: searchIndexTask } = useSearchIndex();
  const { task: skillBackupTask } = useSkillBackup();
  const { publicTasks: memoryTasks } = useMemoryTasks();
  const teamTaskContext = useOptionalTeamTasks();
  const teamTasks = teamTaskContext?.tasks ?? [];
  const catalog = useCatalogController();
  const handledSkillBackupTaskId = useRef<string | null>(null);
  const runningSkillBackupTaskIds = useRef(new Set<string>());
  const [activeSubNavId, setActiveSubNavId] = useState(
    catalog.navigationModel.activeSubNavId,
  );
  const [manualRouteKey, setManualRouteKey] = useState<string | null>(null);
  const [conversationNavigationTarget, setConversationNavigationTarget] =
    useState<ConversationNavigationTarget | null>(null);
  const logViewerOpen = useAppUiStore((state) => state.logViewerOpen);
  const setLogViewerOpen = useAppUiStore((state) => state.setLogViewerOpen);
  const openSettings = useAppUiStore((state) => state.openSettings);

  const initialPath = useMemo(
    () =>
      navigationPath(
        catalog.navigationModel,
        catalog.navigationModel.activeSubNavId,
      ),
    [],
  );
  const router = useMemo(() => createAppRouter(initialPath), [initialPath]);

  useEffect(() => {
    setActiveSubNavId(catalog.navigationModel.activeSubNavId);
    setManualRouteKey(null);
    const targetPath = navigationPath(
      catalog.navigationModel,
      catalog.navigationModel.activeSubNavId,
    );
    if (router.state.location.pathname !== targetPath) {
      void router.navigate({ to: targetPath });
    }
  }, [
    catalog.navigationModel.activeHeaderTabId,
    catalog.navigationModel.activeSubNavId,
    router,
  ]);

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
            message:
              skillBackupTask.error?.message ??
              skillBackupTask.errors[0]?.error.message ??
              "Unknown error",
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

  const activeSubNavItem = catalog.navigationModel.subNavItems[
    catalog.navigationModel.activeHeaderTabId
  ]?.find((item) => item.id === activeSubNavId);
  const activeRouteKey =
    activeSubNavItem?.routeKey ??
    `${catalog.navigationModel.activeHeaderTabId}.${activeSubNavId}`;
  const tenantRouteKey = catalog.activeTenant?.id ?? "tenant-loading";

  function handleHeaderTabSelect(tab: HeaderTabItem) {
    const nextSubNavId =
      catalog.navigationModel.subNavItems[tab.id]?.find((item) => item.enabled)
        ?.id ?? "overview";
    if (
      tab.id === catalog.navigationModel.activeHeaderTabId &&
      nextSubNavId === activeSubNavId
    ) {
      return;
    }
    const nextModel: NavigationModel = {
      ...catalog.navigationModel,
      activeHeaderTabId: tab.id,
      activeSubNavId: nextSubNavId,
    };
    const targetPath = navigationPath(nextModel, nextSubNavId);
    void router.navigate({ to: targetPath });
    setActiveSubNavId(nextSubNavId);
    setManualRouteKey(null);
    setConversationNavigationTarget(null);
    persistNavigationModel(nextModel);
  }

  function handleSubNavSelect(id: string) {
    if (id === activeSubNavId) {
      return;
    }
    const targetPath = navigationPath(catalog.navigationModel, id);
    void router.navigate({ to: targetPath });
    setManualRouteKey(null);
    setConversationNavigationTarget(null);
    setActiveSubNavId(id);
    persistNavigationModel({
      ...catalog.navigationModel,
      activeSubNavId: id,
    });
  }

  function handleMemoryNavigation(memoryTarget: MemoryNavigationTarget) {
    const target = createConversationNavigationTarget({
      blockId:
        memoryTarget.block_id ??
        memoryTarget.turn_id ??
        memoryTarget.session_id,
      questionId: memoryTarget.question_id ?? undefined,
      recordKind: memoryTarget.record_kind,
      sessionId: memoryTarget.session_id,
    });
    const nextSubNavId = conversationSubNavId(target.recordKind);
    const targetPath =
      target.recordKind === "web"
        ? "/conversations/web-records"
        : "/conversations/sessions";
    void router.navigate({ to: targetPath });
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
    const nextSubNavId =
      catalog.navigationModel.subNavItems[tab.id]?.find((item) => item.enabled)
        ?.id ?? "overview";
    const path = navigationPath(
      { ...catalog.navigationModel, activeHeaderTabId: tab.id },
      nextSubNavId,
    );
    void router.preloadRoute({ to: path });
  }

  function handleSubNavPrefetch(id: string) {
    const path = navigationPath(catalog.navigationModel, id);
    void router.preloadRoute({ to: path });
  }

  function persistNavigationModel(nextNavigationModel: NavigationModel) {
    if (typeof catalog.deferNavigationModelSave === "function") {
      catalog.deferNavigationModelSave(nextNavigationModel);
      return;
    }
    void catalog.saveNavigationModel(nextNavigationModel);
  }

  const workspaceContextValue: WorkspaceContextValue = {
    activeSubNavId,
    catalog,
    conversationNavigationTarget,
    handleMemoryNavigation,
    onManualOpen: openCurrentManual,
    onOpenSettings: (panel?: SettingsPanelId) =>
      openSettings(panel ?? "general.appearance"),
    setConversationNavigationTarget,
  };

  return (
    <>
      <AppLayout
        activeSubNavId={activeSubNavId}
        appShortcuts={catalog.appShortcuts}
        navigationModel={catalog.navigationModel}
        notification={catalog.notification}
        onAppShortcutsChange={(shortcuts) =>
          void catalog.saveAppShortcuts(shortcuts)
        }
        onDismissNotification={catalog.dismissNotification}
        onHeaderTabSelect={handleHeaderTabSelect}
        onHeaderTabPrefetch={handleHeaderTabPrefetch}
        onNavigationModelChange={(navigationModel) =>
          void catalog.saveNavigationModel(navigationModel)
        }
        onSkillBackupLibraryChange={() => catalog.refreshOverview()}
        onSubNavSelect={handleSubNavSelect}
        onSubNavPrefetch={handleSubNavPrefetch}
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
            {manualRouteKey ? (
              <Suspense
                fallback={
                  <AppSkeleton label={t("common.loading")} layout="list" />
                }
              >
                <ManualPage
                  routeKey={manualRouteKey}
                  onBack={() => setManualRouteKey(null)}
                />
              </Suspense>
            ) : (
              <WorkspaceContext.Provider value={workspaceContextValue}>
                <RouterProvider router={router} />
              </WorkspaceContext.Provider>
            )}
          </div>
        </div>
      </AppLayout>
      {logViewerOpen ? (
        <Suspense fallback={null}>
          <LogViewerModal
            open={logViewerOpen}
            onClose={() => setLogViewerOpen(false)}
          />
        </Suspense>
      ) : null}
      <AppUpdateDialog />
      <div className="pointer-events-none fixed bottom-5 right-5 z-30 grid gap-3">
        {teamTasks
          .filter((task) =>
            ["Pending", "Running", "Cancelling"].includes(task.state),
          )
          .map((task) => (
            <div
              className="rounded-lg border border-primary/30 bg-surface-container px-4 py-3 text-body-sm text-on-surface shadow-lg"
              key={task.task_id}
            >
              <div className="font-medium">{t("team.task.global")}</div>
              <div className="mt-1 text-on-surface-variant">
                {task.progress
                  ? `${task.progress.current}/${task.progress.total ?? "?"}`
                  : t("team.task.active")}
                {task.progress?.note ? ` · ${task.progress.note}` : ""}
              </div>
            </div>
          ))}
        {searchIndexTask?.status === "running" ? (
          <div className="aurora-task-indicator rounded-xl border px-4 py-3 text-body-sm text-on-surface">
            {t("conversation.searchIndex.building")}
          </div>
        ) : null}
        {conversationSyncTasks.map((task) => (
          <ConversationBackgroundTaskIndicator
            key={task.id}
            task={task}
            t={t}
          />
        ))}
        {memoryTasks
          .filter((task) =>
            ["pending", "running", "cancelling"].includes(task.status),
          )
          .map((task) => (
            <div
              className="rounded-lg border border-outline-variant bg-surface-container px-4 py-3 text-body-sm text-on-surface shadow-lg"
              key={task.id}
            >
              <div className="font-medium">{t("memory.task.running")}</div>
              <div className="mt-1 text-on-surface-variant">
                {task.progress?.note ?? task.kind} ·{" "}
                {task.progress?.current ?? 0}/{task.progress?.total ?? "?"}
              </div>
            </div>
          ))}
        <SkillBackupBackgroundTaskIndicator task={skillBackupTask} t={t} />
      </div>
    </>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
