import type { ReactNode } from "react";
import { NotificationBanner, type NotificationMessage } from "../../components/notifications/NotificationBanner";
import { GlobalSettingsDialog } from "../../components/settings/GlobalSettingsDialog";
import type { NavigationModel, RailMenuItem } from "../../router/types";
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
  onNavigationModelChange,
  onSettingsClose,
  onSettingsOpen,
  onSubNavSelect,
  overview,
  settingsOpen,
}: {
  activeSubNavId: string;
  appShortcuts: AppShortcut[];
  children: ReactNode;
  navigationModel: NavigationModel;
  notification: NotificationMessage | null;
  onAppShortcutsChange: (shortcuts: AppShortcut[]) => void;
  onDismissNotification: (id: string) => void;
  onNavigationModelChange: (navigationModel: NavigationModel) => void;
  onSettingsClose: () => void;
  onSettingsOpen: () => void;
  onSubNavSelect: (id: string) => void;
  overview: AppOverview | null;
  settingsOpen: boolean;
}) {
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];

  function handleRailItemSelect(item: RailMenuItem) {
    if (item.id === "settings") {
      onSettingsOpen();
    }
  }

  return (
    <div className="grid-texture flex min-h-screen bg-background text-on-surface">
      <SideRail
        activeId={settingsOpen ? "settings" : navigationModel.activeRailId}
        items={navigationModel.railItems}
        onItemSelect={handleRailItemSelect}
      />

      <main className="ml-sidebar-width flex min-h-screen w-[calc(100%-64px)] flex-1 flex-col">
        <AppHeader navigationModel={navigationModel} overview={overview} />
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
        open={settingsOpen}
      />
    </div>
  );
}
