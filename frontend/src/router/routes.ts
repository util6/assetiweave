import type { NavigationModel } from "./types";

export type AppRouteId = "catalog" | "conversations" | "web-records" | "sources" | "skill-groups" | "skill-mounts" | "under-construction";

const retiredRouteKeys = new Set(["conversations.sources", "conversations.adapters"]);

const implementedRoutes: Record<string, AppRouteId> = {
  "conversations.sessions": "conversations",
  "conversations.web-records": "web-records",
  "skills.groups": "skill-groups",
  "skills.mounts": "skill-mounts",
  "skills.overview": "catalog",
  "skills.sources": "sources",
};

export function normalizeNavigationModelRoutes(navigationModel: NavigationModel): NavigationModel {
  let changed = false;
  const subNavItems = Object.fromEntries(
    Object.entries(navigationModel.subNavItems).map(([parentId, items]) => {
      const activeItems = items.filter((item) => !retiredRouteKeys.has(item.routeKey));
      if (activeItems.length !== items.length) {
        changed = true;
      }
      return [parentId, activeItems];
    }),
  );
  const activeItems = subNavItems[navigationModel.activeHeaderTabId] ?? [];
  const activeSubNavStillVisible = activeItems.some(
    (item) => item.id === navigationModel.activeSubNavId && item.enabled,
  );
  const activeSubNavId = activeSubNavStillVisible
    ? navigationModel.activeSubNavId
    : activeItems.find((item) => item.enabled)?.id ?? navigationModel.activeSubNavId;

  if (!changed && activeSubNavId === navigationModel.activeSubNavId) {
    return navigationModel;
  }

  return {
    ...navigationModel,
    activeSubNavId,
    subNavItems,
  };
}

export function resolveAppRoute(navigationModel: NavigationModel, activeSubNavId: string): AppRouteId {
  return implementedRoutes[`${navigationModel.activeHeaderTabId}.${activeSubNavId}`] ?? "under-construction";
}
