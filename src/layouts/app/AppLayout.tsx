import type { CSSProperties, ReactNode } from "react";
import { GlobalSettingsDialog } from "../../components/settings/GlobalSettingsDialog";
import { NotificationBanner, type NotificationMessage } from "../../components/notifications/NotificationBanner";
import type { HeaderTabItem, NavigationModel, RailMenuItem } from "../../router/types";
import type { AppOverview, AppShortcut } from "../../types";
import { AppHeader } from "./AppHeader";
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
  overview,
  logViewerOpen,
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
  overview: AppOverview | null;
  settingsOpen: boolean;
}) {
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];
  const railItems = ensureLogRailItem(navigationModel.railItems);
  const mainStyle = {
    "--app-notification-offset": notification ? "4.5rem" : "0px",
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
    <div className="grid-texture flex min-h-screen bg-background text-on-surface">
      <SideRail
        activeId={logViewerOpen ? "logs" : settingsOpen ? "settings" : navigationModel.activeRailId}
        items={railItems}
        onItemSelect={handleRailItemSelect}
      />

      <main className="ml-sidebar-width flex min-h-screen w-[calc(100%-64px)] flex-1 flex-col" style={mainStyle}>
        <AppHeader navigationModel={navigationModel} onHeaderTabSelect={onHeaderTabSelect} overview={overview} />
        <SubNavigation activeId={activeSubNavId} items={activeSubNavItems} onSelect={(item) => onSubNavSelect(item.id)} />
        <NotificationBanner notification={notification} onDismiss={onDismissNotification} />
        {children}
      </main>

      <GlobalSettingsDialog
        appShortcuts={appShortcuts}
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
