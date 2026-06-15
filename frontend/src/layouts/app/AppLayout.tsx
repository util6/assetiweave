import { useState, type CSSProperties, type ReactNode } from "react";
import { GlobalSettingsDialog } from "../../components/settings/GlobalSettingsDialog";
import { NotificationBanner, type NotificationMessage } from "../../components/notifications/NotificationBanner";
import type { HeaderTabItem, NavigationModel, RailMenuItem } from "../../router/types";
import type { SettingsPanelId } from "../../store/settings/AppSettingsProvider";
import type { AppShortcut } from "../../types";
import { SideRail } from "./navigation/SideRail";
import { SubNavigation } from "./navigation/SubNavigation";

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
}) {
  const [sideRailExpanded, setSideRailExpanded] = useState(false);
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];
  const railItems = ensureLogRailItem(navigationModel.railItems).filter(isSupportedRailItem);
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
        expanded={sideRailExpanded}
        headerTabs={navigationModel.headerTabs}
        items={railItems}
        onExpandedChange={setSideRailExpanded}
        onHeaderTabSelect={onHeaderTabSelect}
        onItemSelect={handleRailItemSelect}
      />

      <main className="ml-[var(--app-sidebar-width)] flex min-h-screen w-[calc(100%-var(--app-sidebar-width))] flex-1 flex-col transition-[margin,width] duration-200">
        <SubNavigation activeId={activeSubNavId} items={activeSubNavItems} onSelect={(item) => onSubNavSelect(item.id)} />
        <NotificationBanner notification={notification} onDismiss={onDismissNotification} />
        {children}
      </main>

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
    </div>
  );
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
