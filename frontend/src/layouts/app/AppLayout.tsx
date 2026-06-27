import { lazy, Suspense, useState, type CSSProperties, type ReactNode } from "react";
import { AlertCircle, CheckCircle2, DownloadCloud, RefreshCw } from "lucide-react";
import { useAppUpdater, type AppUpdateDialogMode, type AppUpdateStatus } from "../../app/updates/AppUpdateProvider";
import { NotificationBanner, type NotificationMessage } from "../../components/notifications/NotificationBanner";
import { useI18n } from "../../i18n/I18nProvider";
import type { HeaderTabItem, NavigationModel, RailMenuItem } from "../../router/types";
import type { SettingsPanelId } from "../../store/settings/AppSettingsProvider";
import type { AppShortcut, Tenant, TenantCreateParams } from "../../types";
import { TenantSwitcher, TenantSwitcherDialog } from "./TenantSwitcher";
import { SideRail, type SideRailBrandAction } from "./navigation/SideRail";
import { SubNavigation } from "./navigation/SubNavigation";

const GlobalSettingsDialog = lazy(() =>
  import("../../components/settings/GlobalSettingsDialog").then((module) => ({
    default: module.GlobalSettingsDialog,
  })),
);

export function AppLayout({
  activeSubNavId,
  appShortcuts,
  children,
  navigationModel,
  notification,
  onAppShortcutsChange,
  onDismissNotification,
  onLogViewerOpen,
  onHeaderTabSelect,
  onNavigationModelChange,
  onSkillBackupLibraryChange,
  onSettingsClose,
  onSettingsOpen,
  onSubNavSelect,
  logViewerOpen,
  settingsPanel,
  settingsOpen,
  tenantControls,
}: {
  activeSubNavId: string;
  appShortcuts: AppShortcut[];
  children: ReactNode;
  logViewerOpen: boolean;
  navigationModel: NavigationModel;
  notification: NotificationMessage | null;
  onAppShortcutsChange: (shortcuts: AppShortcut[]) => void;
  onDismissNotification: (id: string) => void;
  onLogViewerOpen: () => void;
  onHeaderTabSelect: (tab: HeaderTabItem) => void;
  onNavigationModelChange: (navigationModel: NavigationModel) => void;
  onSkillBackupLibraryChange?: () => Promise<void> | void;
  onSettingsClose: () => void;
  onSettingsOpen: () => void;
  onSubNavSelect: (id: string) => void;
  settingsPanel: SettingsPanelId;
  settingsOpen: boolean;
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
  const { t } = useI18n();
  const { openDialog: openUpdateDialog, state: updateState } = useAppUpdater();
  const [tenantDialogOpen, setTenantDialogOpen] = useState(false);
  const [sideRailExpanded, setSideRailExpanded] = useState(false);
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];
  const railItems = ensureLogRailItem(navigationModel.railItems).filter(isSupportedRailItem);
  const updateBrandAction = getUpdateBrandAction(updateState, openUpdateDialog, t);
  const layoutStyle = {
    "--app-sidebar-width": sideRailExpanded ? "216px" : "64px",
    "--app-notification-offset": notification ? "78px" : "0px",
  } as CSSProperties;

  function handleRailItemSelect(item: RailMenuItem) {
    if (item.id === "settings") {
      onSettingsOpen();
      return;
    }

    if (item.id === "logs") {
      onLogViewerOpen();
    }
  }

  return (
    <div className="grid-texture flex min-h-screen bg-background text-on-surface" style={layoutStyle}>
      <SideRail
        activeId={logViewerOpen ? "logs" : settingsOpen ? "settings" : navigationModel.activeRailId}
        activeHeaderTabId={navigationModel.activeHeaderTabId}
        brandAction={updateBrandAction}
        expanded={sideRailExpanded}
        headerTabs={navigationModel.headerTabs}
        items={railItems}
        onExpandedChange={setSideRailExpanded}
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

      <main className="ml-[var(--app-sidebar-width)] flex min-h-screen w-[calc(100%-var(--app-sidebar-width))] flex-1 flex-col transition-[margin,width] duration-200">
        <SubNavigation
          activeId={activeSubNavId}
          items={activeSubNavItems}
          onSelect={(item) => onSubNavSelect(item.id)}
        />
        <NotificationBanner notification={notification} onDismiss={onDismissNotification} />
        {children}
      </main>

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

      {settingsOpen ? (
        <Suspense fallback={null}>
          <GlobalSettingsDialog
            appShortcuts={appShortcuts}
            initialPanel={settingsPanel}
            navigationModel={navigationModel}
            onAppShortcutsChange={onAppShortcutsChange}
            onClose={onSettingsClose}
            onNavigationModelChange={onNavigationModelChange}
            onSkillBackupLibraryChange={onSkillBackupLibraryChange}
            open={settingsOpen}
          />
        </Suspense>
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

function getUpdateBrandLabel(status: AppUpdateStatus, t: (key: UpdateLabelKey) => string) {
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

function getUpdateBrandTone(status: AppUpdateStatus): SideRailBrandAction["tone"] {
  if (status === "ready") {
    return "ready";
  }
  if (status === "error") {
    return "error";
  }
  return "update";
}

const logRailItem: RailMenuItem = {
  id: "logs",
  label: "Logs",
  icon: "file-text",
  scope: "global",
  enabled: true,
  position: "secondary",
};

const supportedRailItemIds = new Set(["logs", "settings"]);

function isSupportedRailItem(item: RailMenuItem) {
  return supportedRailItemIds.has(item.id);
}

function ensureLogRailItem(items: RailMenuItem[]) {
  if (items.some((item) => item.id === "logs")) {
    return items;
  }

  const settingsIndex = items.findIndex((item) => item.id === "settings" && item.position === "secondary");
  if (settingsIndex === -1) {
    return [...items, logRailItem];
  }

  return [...items.slice(0, settingsIndex), logRailItem, ...items.slice(settingsIndex)];
}
