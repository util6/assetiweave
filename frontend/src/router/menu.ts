import type { NavigationModel } from "./types";

export const navigationModel: NavigationModel = {
  activeRailId: "catalog",
  activeHeaderTabId: "skills",
  activeSubNavId: "overview",
  railItems: [
    { id: "logs", label: "Logs", icon: "file-text", scope: "global", enabled: true, position: "secondary" },
    { id: "settings", label: "Settings", icon: "settings", scope: "settings", enabled: true, position: "secondary" },
  ],
  headerTabs: [
    { id: "skills", label: "Skills", assetKind: "skill", enabled: true },
    { id: "mcp", label: "MCP", assetKind: "mcp", enabled: true },
    { id: "prompts", label: "Prompts", assetKind: "prompt", enabled: true },
    { id: "rules", label: "Rules", assetKind: "rule", enabled: true },
    { id: "profiles", label: "Profiles", assetKind: "profile", enabled: true },
    { id: "conversations", label: "Conversations", enabled: true },
  ],
  subNavItems: {
    skills: [
      { id: "overview", label: "Catalog Overview", routeKey: "skills.overview", enabled: true },
      { id: "groups", label: "Groups", routeKey: "skills.groups", enabled: true },
      { id: "sources", label: "Skill Sources", routeKey: "skills.sources", enabled: true },
      { id: "mounts", label: "Mounts", routeKey: "skills.mounts", enabled: true },
    ],
    mcp: [
      { id: "overview", label: "Service Overview", routeKey: "mcp.overview", enabled: true },
      { id: "servers", label: "Servers", routeKey: "mcp.servers", enabled: true },
      { id: "configs", label: "Config Projection", routeKey: "mcp.configs", enabled: true },
    ],
    prompts: [
      { id: "overview", label: "Prompt Overview", routeKey: "prompts.overview", enabled: true },
      { id: "templates", label: "Templates", routeKey: "prompts.templates", enabled: true },
      { id: "targets", label: "Target Apps", routeKey: "prompts.targets", enabled: true },
    ],
    rules: [
      { id: "overview", label: "Rule Overview", routeKey: "rules.overview", enabled: true },
      { id: "policies", label: "Policies", routeKey: "rules.policies", enabled: true },
      { id: "conflicts", label: "Conflict Detection", routeKey: "rules.conflicts", enabled: true },
    ],
    profiles: [
      { id: "overview", label: "App Overview", routeKey: "profiles.overview", enabled: true },
      { id: "templates", label: "Profile Templates", routeKey: "profiles.templates", enabled: true },
      { id: "plans", label: "Deployment Plans", routeKey: "profiles.plans", enabled: true },
    ],
    conversations: [
      { id: "sessions", label: "Sessions", routeKey: "conversations.sessions", enabled: true },
    ],
  },
};
