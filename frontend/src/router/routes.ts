import type { NavigationModel } from "./types";

export type AppRouteId = "catalog" | "conversations" | "sources" | "skill-groups" | "skill-mounts" | "under-construction";

const implementedRoutes: Record<string, AppRouteId> = {
  "conversations.adapters": "conversations",
  "conversations.sessions": "conversations",
  "conversations.sources": "conversations",
  "skills.groups": "skill-groups",
  "skills.mounts": "skill-mounts",
  "skills.overview": "catalog",
  "skills.sources": "sources",
};

export function resolveAppRoute(navigationModel: NavigationModel, activeSubNavId: string): AppRouteId {
  return implementedRoutes[`${navigationModel.activeHeaderTabId}.${activeSubNavId}`] ?? "under-construction";
}
