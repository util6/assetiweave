import type { NavigationModel } from "./types";

export type AppRouteId = "catalog" | "sources" | "skill-groups" | "skill-mounts";

export function resolveAppRoute(navigationModel: NavigationModel, activeSubNavId: string): AppRouteId {
  if (navigationModel.activeHeaderTabId === "skills" && activeSubNavId === "groups") {
    return "skill-groups";
  }

  if (navigationModel.activeHeaderTabId === "skills" && activeSubNavId === "sources") {
    return "sources";
  }

  if (navigationModel.activeHeaderTabId === "skills" && activeSubNavId === "mounts") {
    return "skill-mounts";
  }

  return "catalog";
}
