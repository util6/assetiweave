import { create, type StoreApi, type UseBoundStore } from "zustand";
import type { SettingsPanelId } from "../settings/settingsSchema";

export interface AppUiState {
  settingsPanel: SettingsPanelId | null;
  logViewerOpen: boolean;
  updateDialogMode: "intro" | "update" | null;
  openSettings(panel?: SettingsPanelId): void;
  closeSettings(): void;
  setLogViewerOpen(open: boolean): void;
  openUpdateDialog(mode: "intro" | "update"): void;
  closeUpdateDialog(): void;
}

export const useAppUiStore: UseBoundStore<StoreApi<AppUiState>> =
  create<AppUiState>()((set) => ({
    settingsPanel: null,
    logViewerOpen: false,
    updateDialogMode: null,
    openSettings: (panel: SettingsPanelId = "general.appearance") =>
      set({ settingsPanel: panel }),
    closeSettings: () => set({ settingsPanel: null }),
    setLogViewerOpen: (open: boolean) => set({ logViewerOpen: open }),
    openUpdateDialog: (mode: "intro" | "update") =>
      set({ updateDialogMode: mode }),
    closeUpdateDialog: () => set({ updateDialogMode: null }),
  }));
