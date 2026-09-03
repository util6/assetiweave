import type { NavigationModel } from "./types";
import { normalizeNavigationModelRoutes } from "./routes";

const routeKeyToPath: Record<string, string> = {
  "skills.overview": "/skills/overview",
  "skills.sources": "/skills/sources",
  "skills.groups": "/skills/groups",
  "skills.mounts": "/skills/mounts",
  "conversations.sessions": "/conversations/sessions",
  "conversations.web-records": "/conversations/web-records",
  "prompts.overview": "/prompts/overview",
  "memory.recent": "/memory/recent",
  "memory.recall": "/memory/recall",
  "team.overview": "/team/overview",
};

const retiredRouteKeys = new Set([
  "conversations.sources",
  "conversations.adapters",
]);

export function navigationPath(
  model: NavigationModel,
  activeSubNavId: string,
): string {
  const normalized = normalizeNavigationModelRoutes(model);
  const routeKey = `${normalized.activeHeaderTabId}.${activeSubNavId}`;
  if (retiredRouteKeys.has(routeKey)) {
    return "/under-construction";
  }
  return routeKeyToPath[routeKey] ?? "/under-construction";
}
