import type { NavigationModel } from "./types";

export type AppRouteId = "catalog" | "sources";

export function resolveAppRoute(navigationModel: NavigationModel, activeSubNavId: string): AppRouteId {
  if (navigationModel.activeHeaderTabId === "skills" && activeSubNavId === "sources") {
    return "sources";
  }

  return "catalog";
}
