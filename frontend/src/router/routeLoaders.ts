import type { ComponentType } from "react";
import type { AppRouteId } from "./routes";
import type { SkeletonLayoutName } from "../components/foundation/skeleton";
import type { RouteTransitionKind } from "./RouteTransition";

type PageModule<T extends ComponentType<any>> = Promise<{ default: T }>;

function createCachedLoader<T extends ComponentType<any>>(loader: () => PageModule<T>) {
  let pending: PageModule<T> | null = null;

  return () => {
    pending ??= loader().catch((error) => {
      pending = null;
      throw error;
    });
    return pending;
  };
}

export const loadCatalogPage = createCachedLoader(() =>
  import("../pages/catalog/CatalogPage").then((module) => ({ default: module.CatalogPage })),
);

export const loadConversationsPage = createCachedLoader(() =>
  import("../pages/conversations/ConversationsPage").then((module) => ({ default: module.ConversationsPage })),
);

export const loadSkillGroupsPage = createCachedLoader(() =>
  import("../pages/groups/SkillGroupsPage").then((module) => ({ default: module.SkillGroupsPage })),
);

export const loadPromptOverviewPage = createCachedLoader(() =>
  import("../pages/prompts/PromptOverviewPage").then((module) => ({ default: module.PromptOverviewPage })),
);

export const loadSkillMountsPage = createCachedLoader(() =>
  import("../pages/mounts/SkillMountsPage").then((module) => ({ default: module.SkillMountsPage })),
);

export const loadSourcesPage = createCachedLoader(() =>
  import("../pages/sources/SourcesPage").then((module) => ({ default: module.SourcesPage })),
);

export const loadMemoryPage = createCachedLoader(() =>
  import("../pages/memory/MemoryPage").then((module) => ({ default: module.MemoryPage })),
);

export const loadLogViewerModal = createCachedLoader(() =>
  import("../components/logs/LogViewerModal").then((module) => ({ default: module.LogViewerModal })),
);

export const loadManualPage = createCachedLoader(() =>
  import("../manuals/ManualPage").then((module) => ({ default: module.ManualPage })),
);

export interface RouteDefinition {
  loader?: () => Promise<{ default: ComponentType<any> }>;
  skeleton: SkeletonLayoutName;
  transition: RouteTransitionKind | "memory" | null;
}

export const routeRegistry: Record<AppRouteId, RouteDefinition> = {
  catalog: { loader: loadCatalogPage, skeleton: "list", transition: "list" },
  conversations: { loader: loadConversationsPage, skeleton: "columns", transition: "columns" },
  "prompts-overview": { loader: loadPromptOverviewPage, skeleton: "cards", transition: "cards" },
  sources: { loader: loadSourcesPage, skeleton: "list", transition: "list" },
  "skill-groups": { loader: loadSkillGroupsPage, skeleton: "columns", transition: "columns" },
  "skill-mounts": { loader: loadSkillMountsPage, skeleton: "columns", transition: "columns" },
  "web-records": { loader: loadConversationsPage, skeleton: "columns", transition: "columns" },
  memory: { loader: loadMemoryPage, skeleton: "columns", transition: "memory" },
  "under-construction": { skeleton: "list", transition: null },
};

export function preloadRoute(routeId: AppRouteId) {
  const loader = routeRegistry[routeId]?.loader;
  if (!loader) {
    return;
  }

  void loader().catch(() => undefined);
}
