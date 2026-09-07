import { createContext, useContext } from "react";
import type { CatalogController } from "../hooks/catalog/useCatalogController";
import type { ConversationNavigationTarget } from "./navigationTargets";
import type { SettingsPanelId } from "../store/settings/settingsSchema";
import type { MemoryNavigationTarget } from "../types/memory";

export interface WorkspaceContextValue {
  catalog: CatalogController;
  activeSubNavId: string;
  conversationNavigationTarget: ConversationNavigationTarget | null;
  onManualOpen: () => void;
  onOpenSettings: (panel?: SettingsPanelId) => void;
  setConversationNavigationTarget: (
    updater: (
      current: ConversationNavigationTarget | null,
    ) => ConversationNavigationTarget | null,
  ) => void;
  handleMemoryNavigation: (target: MemoryNavigationTarget) => void;
}

export const WorkspaceContext = createContext<WorkspaceContextValue | null>(
  null,
);

export function useWorkspaceContext(): WorkspaceContextValue {
  const context = useContext(WorkspaceContext);
  if (!context) {
    throw new Error(
      "useWorkspaceContext must be used within a WorkspaceContext.Provider",
    );
  }
  return context;
}
