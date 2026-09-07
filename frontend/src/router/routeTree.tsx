import {
  createRootRoute,
  createRoute,
  lazyRouteComponent,
  Navigate,
  Outlet,
} from "@tanstack/react-router";
import { AppSkeleton } from "../components/foundation/skeleton";
import { useWorkspaceContext } from "./WorkspaceContext";

const CatalogPage = lazyRouteComponent(
  () => import("../pages/catalog/CatalogPage"),
  "CatalogPage",
);
const ConversationsPage = lazyRouteComponent(
  () => import("../pages/conversations/ConversationsPage"),
  "ConversationsPage",
);
const SkillGroupsPage = lazyRouteComponent(
  () => import("../pages/groups/SkillGroupsPage"),
  "SkillGroupsPage",
);
const SkillMountsPage = lazyRouteComponent(
  () => import("../pages/mounts/SkillMountsPage"),
  "SkillMountsPage",
);
const PromptOverviewPage = lazyRouteComponent(
  () => import("../pages/prompts/PromptOverviewPage"),
  "PromptOverviewPage",
);
const SourcesPage = lazyRouteComponent(
  () => import("../pages/sources/SourcesPage"),
  "SourcesPage",
);
const MemoryPage = lazyRouteComponent(
  () => import("../pages/memory/MemoryPage"),
  "MemoryPage",
);
const TeamPage = lazyRouteComponent(
  () => import("../pages/team/TeamPage"),
  "TeamPage",
);
const UnderConstructionPage = lazyRouteComponent(
  () => import("../pages/under-construction/UnderConstructionPage"),
  "UnderConstructionPage",
);

function ListPending() {
  return <AppSkeleton label="Loading" layout="list" />;
}

function ColumnsPending() {
  return <AppSkeleton label="Loading" layout="columns" />;
}

function CardsPending() {
  return <AppSkeleton label="Loading" layout="cards" />;
}

function CatalogView() {
  const { catalog, onManualOpen, onOpenSettings } = useWorkspaceContext();
  return (
    <CatalogPage
      catalog={catalog}
      onManualOpen={onManualOpen}
      onOpenSettings={() => onOpenSettings("general.appearance")}
    />
  );
}

function SourcesView() {
  const { catalog, onManualOpen, onOpenSettings } = useWorkspaceContext();
  return (
    <SourcesPage
      appShortcuts={catalog.appShortcuts}
      assetMountStatuses={catalog.assetMountStatuses}
      assets={catalog.assets}
      expandedAssetIds={catalog.expandedIds}
      onAssetReveal={(path) => void catalog.revealPath(path)}
      onApplyAssetUpdate={catalog.applyAssetUpdate}
      onCatalogRefresh={catalog.refreshOverview}
      onClearDeploymentPlan={catalog.clearDeploymentPlan}
      onManualOpen={onManualOpen}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
      onOpenSettings={() => onOpenSettings("workspace.menu")}
      onRefreshMountStatus={catalog.refreshMountStatus}
      onRemoveAsset={catalog.removeAsset}
      onSetSourceMountProfile={catalog.setMountProfiles}
      onToggleAsset={catalog.toggleAsset}
      onToggleMount={catalog.toggleMountProfile}
      profiles={catalog.profiles}
      refreshingMountStatus={catalog.refreshingMountStatus}
    />
  );
}

function SkillGroupsView() {
  const { catalog, onManualOpen, onOpenSettings } = useWorkspaceContext();
  return (
    <SkillGroupsPage
      appShortcuts={catalog.appShortcuts}
      assetMountStatuses={catalog.assetMountStatuses}
      assets={catalog.assets}
      expandedAssetIds={catalog.expandedIds}
      onManualOpen={onManualOpen}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
      onOpenSettings={() => onOpenSettings("general.storage")}
      onApplyGroupExclusiveMount={catalog.applyGroupExclusiveMount}
      onPreviewGroupExclusiveMount={catalog.previewGroupExclusiveMount}
      onRefreshMountStatus={catalog.refreshMountStatus}
      onRevealPath={(path) => void catalog.revealPath(path)}
      onSetGroupMountProfile={catalog.setGroupMountProfile}
      onSetSkillMountProfiles={catalog.setMountProfiles}
      onToggleAsset={catalog.toggleAsset}
      onToggleMount={catalog.toggleMountProfile}
      profiles={catalog.profiles}
      refreshingMountStatus={catalog.refreshingMountStatus}
      sources={catalog.sources}
    />
  );
}

function SkillMountsView() {
  const { catalog, onManualOpen, onOpenSettings } = useWorkspaceContext();
  return (
    <SkillMountsPage
      appShortcuts={catalog.appShortcuts}
      assetMountStatuses={catalog.assetMountStatuses}
      assets={catalog.assets}
      onCatalogRefresh={catalog.refreshOverview}
      onManualOpen={onManualOpen}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
      onOpenSettings={() => onOpenSettings("general.storage")}
      onRefreshMountStatus={catalog.refreshMountStatus}
      onRefreshProfiles={catalog.refreshProfiles}
      onRevealPath={(path) => void catalog.revealPath(path)}
      onSaveAppShortcuts={catalog.saveAppShortcuts}
      onSetSkillMountProfiles={catalog.setMountProfiles}
      onToggleMount={catalog.toggleMountProfile}
      profiles={catalog.profiles}
      refreshingMountStatus={catalog.refreshingMountStatus}
      sources={catalog.sources}
    />
  );
}

function ConversationsSessionsView() {
  const {
    activeSubNavId,
    catalog,
    conversationNavigationTarget,
    onManualOpen,
    onOpenSettings,
    setConversationNavigationTarget,
  } = useWorkspaceContext();
  return (
    <ConversationsPage
      activeSubNavId={activeSubNavId}
      appShortcuts={catalog.appShortcuts}
      onManualOpen={onManualOpen}
      navigationTarget={
        conversationNavigationTarget?.recordKind === "session"
          ? conversationNavigationTarget
          : null
      }
      onNavigationTargetConsumed={(nonce) =>
        setConversationNavigationTarget((current) =>
          current?.nonce === nonce ? null : current,
        )
      }
      onNotify={(notification) => catalog.showNotification(notification)}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
      onOpenSettings={onOpenSettings}
      recordKind="session"
    />
  );
}

function ConversationsWebRecordsView() {
  const {
    activeSubNavId,
    catalog,
    conversationNavigationTarget,
    onManualOpen,
    onOpenSettings,
    setConversationNavigationTarget,
  } = useWorkspaceContext();
  return (
    <ConversationsPage
      activeSubNavId={activeSubNavId}
      appShortcuts={catalog.appShortcuts}
      onManualOpen={onManualOpen}
      navigationTarget={
        conversationNavigationTarget?.recordKind === "web"
          ? conversationNavigationTarget
          : null
      }
      onNavigationTargetConsumed={(nonce) =>
        setConversationNavigationTarget((current) =>
          current?.nonce === nonce ? null : current,
        )
      }
      onNotify={(notification) => catalog.showNotification(notification)}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
      onOpenSettings={onOpenSettings}
      recordKind="web"
    />
  );
}

function PromptsOverviewView() {
  const { onManualOpen, catalog } = useWorkspaceContext();
  return (
    <PromptOverviewPage
      onManualOpen={onManualOpen}
      onNotifyError={(message) =>
        catalog.showNotification({ tone: "error", message })
      }
    />
  );
}

function MemoryRecentView() {
  const { handleMemoryNavigation } = useWorkspaceContext();
  return (
    <MemoryPage activeSubNavId="recent" onNavigate={handleMemoryNavigation} />
  );
}

function MemoryRecallView() {
  const { handleMemoryNavigation } = useWorkspaceContext();
  return (
    <MemoryPage activeSubNavId="recall" onNavigate={handleMemoryNavigation} />
  );
}

function TeamOverviewView() {
  return <TeamPage />;
}

function UnderConstructionView() {
  const { onManualOpen } = useWorkspaceContext();
  return <UnderConstructionPage onManualOpen={onManualOpen} />;
}

export const rootRoute = createRootRoute({
  component: () => <Outlet />,
});

export const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => <Navigate to="/skills/overview" />,
});

export const skillsOverviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skills/overview",
  component: CatalogView,
  pendingComponent: ListPending,
});

export const skillsSourcesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skills/sources",
  component: SourcesView,
  pendingComponent: ListPending,
});

export const skillsGroupsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skills/groups",
  component: SkillGroupsView,
  pendingComponent: ColumnsPending,
});

export const skillsMountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/skills/mounts",
  component: SkillMountsView,
  pendingComponent: ColumnsPending,
});

export const conversationsSessionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/conversations/sessions",
  component: ConversationsSessionsView,
  pendingComponent: ColumnsPending,
});

export const conversationsWebRecordsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/conversations/web-records",
  component: ConversationsWebRecordsView,
  pendingComponent: ColumnsPending,
});

export const promptsOverviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/prompts/overview",
  component: PromptsOverviewView,
  pendingComponent: CardsPending,
});

export const memoryRecentRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/memory/recent",
  component: MemoryRecentView,
  pendingComponent: ColumnsPending,
});

export const memoryRecallRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/memory/recall",
  component: MemoryRecallView,
  pendingComponent: ColumnsPending,
});

export const teamOverviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/team/overview",
  component: TeamOverviewView,
  pendingComponent: ColumnsPending,
});

export const underConstructionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/under-construction",
  component: UnderConstructionView,
  pendingComponent: ListPending,
});

export const wildcardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "*",
  component: () => <Navigate to="/under-construction" />,
});

export const routeTree = rootRoute.addChildren([
  indexRoute,
  skillsOverviewRoute,
  skillsSourcesRoute,
  skillsGroupsRoute,
  skillsMountsRoute,
  conversationsSessionsRoute,
  conversationsWebRecordsRoute,
  promptsOverviewRoute,
  memoryRecentRoute,
  memoryRecallRoute,
  teamOverviewRoute,
  underConstructionRoute,
  wildcardRoute,
]);
