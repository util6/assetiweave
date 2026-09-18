import {
  lazy,
  Suspense,
  useMemo,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import clsx from "clsx";
import {
  AlertCircle,
  CheckCircle2,
  DownloadCloud,
  RefreshCw,
} from "lucide-react";
import { useOptionalTaskCenter } from "../../app/backgroundTasks/TaskCenterProvider";
import {
  useAppUpdater,
  type AppUpdateDialogMode,
  type AppUpdateStatus,
} from "../../app/updates/AppUpdateProvider";
import {
  NotificationBanner,
  type NotificationMessage,
} from "../../components/notifications/NotificationBanner";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  HeaderTabItem,
  NavigationModel,
  RailMenuItem,
} from "../../router/types";
import type { SettingsPanelId } from "../../store/settings/settingsSchema";
import type { AppShortcut, Tenant, TenantCreateParams } from "../../types";
import { TenantSwitcher, TenantSwitcherDialog } from "./TenantSwitcher";
import { WindowTitleBar } from "./WindowTitleBar";
import { SideRail, type SideRailBrandAction } from "./navigation/SideRail";
import { SubNavigation } from "./navigation/SubNavigation";
import { useAppUiStore } from "../../store/ui/appUiStore";
import { ErrorBoundary } from "../../components/foundation/ErrorBoundary";

const GlobalSettingsDialog = lazy(() =>
  import("../../components/settings/GlobalSettingsDialog").then((module) => ({
    default: module.GlobalSettingsDialog,
  })),
);

const TaskCenterModal = lazy(() =>
  import("../../components/tasks/TaskCenterModal").then((module) => ({
    default: module.TaskCenterModal,
  })),
);

export function AppLayout({
  activeRailId,
  activeSubNavId,
  appShortcuts,
  children,
  navigationModel,
  notification,
  onAppShortcutsChange,
  onDismissNotification,
  onLogViewerOpen,
  onHeaderTabSelect,
  onHeaderTabPrefetch,
  onNavigationModelChange,
  onSkillBackupLibraryChange,
  onSettingsClose,
  onSettingsOpen,
  onSubNavSelect,
  onSubNavPrefetch,
  onTasksOpen,
  logViewerOpen,
  settingsPanel,
  settingsOpen,
  tenantControls,
}: {
  activeRailId?: string;
  activeSubNavId: string;
  appShortcuts: AppShortcut[];
  children: ReactNode;
  logViewerOpen?: boolean;
  navigationModel: NavigationModel;
  notification: NotificationMessage | null;
  onAppShortcutsChange: (shortcuts: AppShortcut[]) => void;
  onDismissNotification: (id: string) => void;
  onLogViewerOpen?: () => void;
  onHeaderTabSelect: (tab: HeaderTabItem) => void;
  onHeaderTabPrefetch?: (tab: HeaderTabItem) => void;
  onNavigationModelChange: (navigationModel: NavigationModel) => void;
  onSkillBackupLibraryChange?: () => Promise<void> | void;
  onSettingsClose?: () => void;
  onSettingsOpen?: () => void;
  onSubNavSelect: (id: string) => void;
  onSubNavPrefetch?: (id: string) => void;
  onTasksOpen?: () => void;
  settingsPanel?: SettingsPanelId;
  settingsOpen?: boolean;
  tenantControls: {
    activeTenant: Tenant | null;
    busy: boolean;
    error?: string | null;
    loading: boolean;
    onCreateTenant: (params: TenantCreateParams) => Promise<unknown>;
    onSwitchTenant: (tenantId: string) => Promise<unknown>;
    tenants: Tenant[];
  };
}) {
  const storeLogViewerOpen = useAppUiStore((state) => state.logViewerOpen);
  const storeSetLogViewerOpen = useAppUiStore(
    (state) => state.setLogViewerOpen,
  );
  const storeTaskCenterOpen = useAppUiStore((state) => state.taskCenterOpen);
  const storeSetTaskCenterOpen = useAppUiStore(
    (state) => state.setTaskCenterOpen,
  );
  const storeSettingsPanel = useAppUiStore((state) => state.settingsPanel);
  const storeOpenSettings = useAppUiStore((state) => state.openSettings);
  const storeCloseSettings = useAppUiStore((state) => state.closeSettings);

  const effectiveLogViewerOpen = logViewerOpen ?? storeLogViewerOpen;
  const effectiveSettingsOpen = settingsOpen ?? storeSettingsPanel !== null;
  const effectiveSettingsPanel =
    settingsPanel ?? storeSettingsPanel ?? "general.appearance";
  const handleOpenSettings =
    onSettingsOpen ?? (() => storeOpenSettings("general.appearance"));
  const handleCloseSettings = onSettingsClose ?? storeCloseSettings;
  const handleOpenLogViewer =
    onLogViewerOpen ?? (() => storeSetLogViewerOpen(true));
  const handleOpenTasks = onTasksOpen ?? (() => storeSetTaskCenterOpen(true));

  const { t } = useI18n();
  const { openDialog: openUpdateDialog, state: updateState } = useAppUpdater();
  const [tenantDialogOpen, setTenantDialogOpen] = useState(false);
  const [sideRailExpanded, setSideRailExpanded] = useState(false);
  const activeSubNavItems =
    navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];
  const railItems = ensureSecondaryRailItems(navigationModel.railItems).filter(
    isSupportedRailItem,
  );
  const updateBrandAction = getUpdateBrandAction(
    updateState,
    openUpdateDialog,
    t,
  );
  const layoutStyle = {
    "--app-sidebar-width": sideRailExpanded ? "216px" : "64px",
    // Opt-in bounded routes share the existing titlebar + subnavigation offset.
    "--app-route-viewport-height": "calc(100dvh - var(--app-toolbar-top))",
  } as CSSProperties;

  const taskCenter = useOptionalTaskCenter();
  const activeCount = taskCenter?.activeCount ?? 0;
  const failureCount = taskCenter?.failureCount ?? 0;

  const taskBadge = useMemo(() => {
    if (failureCount > 0) {
      return (
        <span
          className={clsx(
            sideRailExpanded
              ? "rounded-full bg-status-remove/15 px-1.5 py-0.5 text-[10px] font-semibold text-status-remove"
              : "absolute -right-1 -top-1 size-2.5 rounded-full bg-status-remove ring-2 ring-background",
          )}
          aria-label={`失败任务 ${failureCount}`}
        >
          {sideRailExpanded ? failureCount : null}
        </span>
      );
    }
    if (activeCount > 0) {
      return (
        <span
          className={clsx(
            sideRailExpanded
              ? "animate-pulse rounded-full bg-primary/15 px-1.5 py-0.5 text-[10px] font-semibold text-primary"
              : "absolute -right-1 -top-1 size-2.5 animate-pulse rounded-full bg-primary ring-2 ring-background",
          )}
          aria-label={`进行中任务 ${activeCount}`}
        >
          {sideRailExpanded ? activeCount : null}
        </span>
      );
    }
    return null;
  }, [activeCount, failureCount, sideRailExpanded]);

  function handleRailItemSelect(item: RailMenuItem) {
    if (item.id === "tasks") {
      handleOpenTasks();
      return;
    }

    if (item.id === "settings") {
      handleOpenSettings();
      return;
    }

    if (item.id === "logs") {
      handleOpenLogViewer();
    }
  }

  return (
    <div
      className="min-h-screen bg-background text-on-surface"
      style={layoutStyle}
    >
      <WindowTitleBar />
      <div className="flex min-h-[calc(100vh-var(--app-window-titlebar-height))]">
        <SideRail
          activeId={
            activeRailId ??
            (storeTaskCenterOpen
              ? "tasks"
              : effectiveLogViewerOpen
                ? "logs"
                : effectiveSettingsOpen
                  ? "settings"
                  : navigationModel.activeRailId)
          }
          activeHeaderTabId={navigationModel.activeHeaderTabId}
          badges={{ tasks: taskBadge }}
          brandAction={updateBrandAction}
          expanded={sideRailExpanded}
          headerTabs={navigationModel.headerTabs}
          items={railItems}
          onExpandedChange={setSideRailExpanded}
          onHeaderTabPrefetch={onHeaderTabPrefetch}
          onHeaderTabSelect={onHeaderTabSelect}
          onItemSelect={handleRailItemSelect}
          primaryAction={
            <TenantSwitcher
              activeTenant={tenantControls.activeTenant}
              busy={tenantControls.busy}
              loading={tenantControls.loading}
              onOpen={() => setTenantDialogOpen(true)}
              open={tenantDialogOpen}
            />
          }
        />

        <main className="relative ml-[var(--app-sidebar-width)] flex min-h-[calc(100vh-var(--app-window-titlebar-height))] w-[calc(100%-var(--app-sidebar-width))] flex-1 flex-col transition-[margin,width] duration-200">
          <SubNavigation
            activeId={activeSubNavId}
            items={activeSubNavItems}
            onSelect={(item) => onSubNavSelect(item.id)}
            onPrefetch={onSubNavPrefetch}
          />
          <NotificationBanner
            notification={notification}
            onDismiss={onDismissNotification}
          />
          {children}
        </main>
      </div>

      {tenantDialogOpen ? (
        <TenantSwitcherDialog
          activeTenant={tenantControls.activeTenant}
          busy={tenantControls.busy}
          error={tenantControls.error}
          loading={tenantControls.loading}
          onClose={() => setTenantDialogOpen(false)}
          onCreateTenant={tenantControls.onCreateTenant}
          onSwitchTenant={tenantControls.onSwitchTenant}
          tenants={tenantControls.tenants}
        />
      ) : null}

      {effectiveSettingsOpen ? (
        <Suspense fallback={null}>
          <GlobalSettingsDialog
            appShortcuts={appShortcuts}
            initialPanel={effectiveSettingsPanel}
            navigationModel={navigationModel}
            onAppShortcutsChange={onAppShortcutsChange}
            onClose={handleCloseSettings}
            onNavigationModelChange={onNavigationModelChange}
            onSkillBackupLibraryChange={onSkillBackupLibraryChange}
            open={effectiveSettingsOpen}
          />
        </Suspense>
      ) : null}

      {storeTaskCenterOpen ? (
        <ErrorBoundary onReset={() => storeSetTaskCenterOpen(false)}>
          <Suspense fallback={null}>
            <TaskCenterModal
              onClose={() => storeSetTaskCenterOpen(false)}
              open={storeTaskCenterOpen}
            />
          </Suspense>
        </ErrorBoundary>
      ) : null}
    </div>
  );
}

type UpdateLabelKey =
  | "app.title"
  | "update.button.available"
  | "update.button.downloading"
  | "update.button.error"
  | "update.button.installing"
  | "update.button.ready"
  | "update.intro.open";

function getUpdateBrandAction(
  state: {
    info: { version: string } | null;
    status: AppUpdateStatus;
    supported: boolean;
  },
  openDialog: (mode?: AppUpdateDialogMode) => void,
  t: (key: UpdateLabelKey) => string,
): SideRailBrandAction | undefined {
  if (!state.supported) {
    return undefined;
  }

  if (!state.info) {
    const label = t("app.title");
    return {
      ariaLabel: t("update.intro.open"),
      label,
      onClick: () => openDialog("intro"),
      title: t("update.intro.open"),
      tone: "neutral",
    };
  }

  const statusLabel = getUpdateBrandLabel(state.status, t);
  const label = `${statusLabel} v${state.info.version}`;
  const Icon = getUpdateBrandIcon(state.status);

  return {
    ariaLabel: label,
    busy: state.status === "downloading" || state.status === "installing",
    icon: <Icon size={12} />,
    label,
    onClick: () => openDialog("update"),
    title: label,
    tone: getUpdateBrandTone(state.status),
  };
}

function getUpdateBrandLabel(
  status: AppUpdateStatus,
  t: (key: UpdateLabelKey) => string,
) {
  if (status === "downloading") {
    return t("update.button.downloading");
  }
  if (status === "installing") {
    return t("update.button.installing");
  }
  if (status === "ready") {
    return t("update.button.ready");
  }
  if (status === "error") {
    return t("update.button.error");
  }
  return t("update.button.available");
}

function getUpdateBrandIcon(status: AppUpdateStatus) {
  if (status === "ready") {
    return CheckCircle2;
  }
  if (status === "error") {
    return AlertCircle;
  }
  if (status === "downloading" || status === "installing") {
    return RefreshCw;
  }
  return DownloadCloud;
}

function getUpdateBrandTone(
  status: AppUpdateStatus,
): SideRailBrandAction["tone"] {
  if (status === "ready") {
    return "ready";
  }
  if (status === "error") {
    return "error";
  }
  return "update";
}

const tasksRailItem: RailMenuItem = {
  id: "tasks",
  label: "Tasks",
  icon: "tasks",
  scope: "global",
  enabled: true,
  position: "secondary",
};

const logRailItem: RailMenuItem = {
  id: "logs",
  label: "Logs",
  icon: "file-text",
  scope: "global",
  enabled: true,
  position: "secondary",
};

const supportedRailItemIds = new Set(["tasks", "logs", "settings"]);

function isSupportedRailItem(item: RailMenuItem) {
  return supportedRailItemIds.has(item.id);
}

function ensureSecondaryRailItems(items: RailMenuItem[]) {
  const result = [...items];
  if (!result.some((item) => item.id === "tasks")) {
    const logsIndex = result.findIndex((item) => item.id === "logs");
    if (logsIndex !== -1) {
      result.splice(logsIndex, 0, tasksRailItem);
    } else {
      const settingsIndex = result.findIndex((item) => item.id === "settings");
      if (settingsIndex !== -1) {
        result.splice(settingsIndex, 0, tasksRailItem);
      } else {
        result.push(tasksRailItem);
      }
    }
  }

  if (!result.some((item) => item.id === "logs")) {
    const settingsIndex = result.findIndex((item) => item.id === "settings");
    if (settingsIndex !== -1) {
      result.splice(settingsIndex, 0, logRailItem);
    } else {
      result.push(logRailItem);
    }
  }

  return result;
}
