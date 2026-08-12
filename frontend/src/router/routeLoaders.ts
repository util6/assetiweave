import type { ComponentType } from "react";
import type { AppRouteId } from "./routes";

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

const routePreloaders: Partial<Record<AppRouteId, () => Promise<unknown>>> = {
  catalog: loadCatalogPage,
  conversations: loadConversationsPage,
  "prompts-overview": loadPromptOverviewPage,
  sources: loadSourcesPage,
  "skill-groups": loadSkillGroupsPage,
  "skill-mounts": loadSkillMountsPage,
  "web-records": loadConversationsPage,
};

export function preloadRoute(routeId: AppRouteId) {
  const loader = routePreloaders[routeId];
  if (!loader) {
    return;
  }

  void loader().catch(() => undefined);
}
