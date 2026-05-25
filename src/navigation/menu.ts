import type { NavigationModel } from "./types";

export const navigationModel: NavigationModel = {
  activeRailId: "catalog",
  activeHeaderTabId: "skills",
  activeSubNavId: "overview",
  railItems: [
    { id: "home", label: "启动台", icon: "rocket", scope: "global", enabled: true, position: "primary" },
    { id: "dashboard", label: "运行概览", icon: "gauge", scope: "global", enabled: true, position: "primary" },
    { id: "routes", label: "路由", icon: "navigation", scope: "global", enabled: true, position: "primary" },
    { id: "knowledge", label: "知识资产", icon: "brain", scope: "asset-catalog", enabled: true, position: "primary" },
    { id: "sources", label: "Source 管理", icon: "layers", scope: "asset-catalog", enabled: true, position: "primary" },
    { id: "profiles", label: "Profile", icon: "boxes", scope: "profile", enabled: true, position: "primary" },
    { id: "commands", label: "命令", icon: "command", scope: "asset-catalog", enabled: true, position: "primary" },
    { id: "automation", label: "自动化", icon: "sparkles", scope: "global", enabled: true, position: "primary" },
    { id: "catalog", label: "资产目录", icon: "archive", scope: "asset-catalog", enabled: true, position: "primary" },
    { id: "apps", label: "App 管理", icon: "grid", scope: "profile", enabled: true, position: "primary" },
    { id: "security", label: "安全策略", icon: "shield", scope: "settings", enabled: true, position: "secondary" },
    { id: "docs", label: "文档", icon: "file-code", scope: "global", enabled: true, position: "secondary" },
    { id: "settings", label: "设置", icon: "settings", scope: "settings", enabled: true, position: "secondary" },
  ],
  headerTabs: [
    { id: "skills", label: "Skills", assetKind: "skill", enabled: true },
    { id: "mcp", label: "MCP", assetKind: "mcp", enabled: true },
    { id: "prompts", label: "Prompts", assetKind: "prompt", enabled: true },
    { id: "rules", label: "Rules", assetKind: "rule", enabled: true },
    { id: "profiles", label: "Profiles", assetKind: "profile", enabled: true },
  ],
  subNavItems: {
    skills: [
      { id: "overview", label: "目录总览", routeKey: "skills.overview", enabled: true },
      { id: "groups", label: "分组管理", routeKey: "skills.groups", enabled: true },
      { id: "sources", label: "Skill 源管理", routeKey: "skills.sources", enabled: true },
      { id: "mounts", label: "挂载管理", routeKey: "skills.mounts", enabled: true },
    ],
    mcp: [
      { id: "overview", label: "服务总览", routeKey: "mcp.overview", enabled: true },
      { id: "servers", label: "Server 管理", routeKey: "mcp.servers", enabled: true },
      { id: "configs", label: "配置投影", routeKey: "mcp.configs", enabled: true },
    ],
    prompts: [
      { id: "overview", label: "Prompt 总览", routeKey: "prompts.overview", enabled: true },
      { id: "templates", label: "模板管理", routeKey: "prompts.templates", enabled: true },
      { id: "targets", label: "目标 App", routeKey: "prompts.targets", enabled: true },
    ],
    rules: [
      { id: "overview", label: "规则总览", routeKey: "rules.overview", enabled: true },
      { id: "policies", label: "启用策略", routeKey: "rules.policies", enabled: true },
      { id: "conflicts", label: "冲突检测", routeKey: "rules.conflicts", enabled: true },
    ],
    profiles: [
      { id: "overview", label: "App 总览", routeKey: "profiles.overview", enabled: true },
      { id: "templates", label: "Profile 模板", routeKey: "profiles.templates", enabled: true },
      { id: "plans", label: "部署计划", routeKey: "profiles.plans", enabled: true },
    ],
  },
};
