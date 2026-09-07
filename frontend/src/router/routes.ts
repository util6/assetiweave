import type { NavigationModel } from "./types";

export type AppRouteId =
  | "catalog"
  | "conversations"
  | "prompts-overview"
  | "web-records"
  | "sources"
  | "skill-groups"
  | "skill-mounts"
  | "memory"
  | "team"
  | "under-construction";

const retiredRouteKeys = new Set([
  "conversations.sources",
  "conversations.adapters",
]);

export function normalizeNavigationModelRoutes(
  navigationModel: NavigationModel,
): NavigationModel {
  let changed = false;
  const subNavItems = Object.fromEntries(
    Object.entries(navigationModel.subNavItems).map(([parentId, items]) => {
      const activeItems = items.filter(
        (item) => !retiredRouteKeys.has(item.routeKey),
      );
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
    : (activeItems.find((item) => item.enabled)?.id ??
      navigationModel.activeSubNavId);

  if (!changed && activeSubNavId === navigationModel.activeSubNavId) {
    return navigationModel;
  }

  return {
    ...navigationModel,
    activeSubNavId,
    subNavItems,
  };
}
