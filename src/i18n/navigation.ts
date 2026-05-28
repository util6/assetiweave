import type { HeaderTabItem, RailMenuItem, SubNavItem } from "../navigation/types";
import type { Translator } from "./I18nProvider";
import type { TranslationKey } from "./messages";

const railLabelKeys: Partial<Record<string, TranslationKey>> = {
  home: "nav.rail.home",
  dashboard: "nav.rail.dashboard",
  routes: "nav.rail.routes",
  knowledge: "nav.rail.knowledge",
  sources: "nav.rail.sources",
  profiles: "nav.rail.profiles",
  commands: "nav.rail.commands",
  automation: "nav.rail.automation",
  catalog: "nav.rail.catalog",
  apps: "nav.rail.apps",
  security: "nav.rail.security",
  docs: "nav.rail.docs",
  settings: "nav.rail.settings",
};

const railDefaultLabels: Partial<Record<string, string[]>> = {
  home: ["Launchpad", "启动台"],
  dashboard: ["Overview", "运行概览"],
  routes: ["Routes", "路由"],
  knowledge: ["Knowledge Assets", "知识资产"],
  sources: ["Source Management", "来源管理", "Source 管理"],
  profiles: ["Profiles", "目标配置", "Profile"],
  commands: ["Commands", "命令"],
  automation: ["Automation", "自动化"],
  catalog: ["Asset Catalog", "资产目录"],
  apps: ["App Management", "应用管理", "App 管理"],
  security: ["Security Policies", "安全策略"],
  docs: ["Docs", "文档"],
  settings: ["Settings", "设置"],
};

const headerLabelKeys: Partial<Record<string, TranslationKey>> = {
  skills: "nav.header.skills",
  mcp: "nav.header.mcp",
  prompts: "nav.header.prompts",
  rules: "nav.header.rules",
  profiles: "nav.header.profiles",
};

const headerDefaultLabels: Partial<Record<string, string[]>> = {
  skills: ["Skills", "技能"],
  mcp: ["MCP"],
  prompts: ["Prompts", "提示词"],
  rules: ["Rules", "规则"],
  profiles: ["Profiles", "目标配置"],
};

const subNavLabelKeys: Partial<Record<string, TranslationKey>> = {
  "skills.overview": "nav.sub.skills.overview",
  "skills.groups": "nav.sub.skills.groups",
  "skills.sources": "nav.sub.skills.sources",
  "skills.mounts": "nav.sub.skills.mounts",
  "mcp.overview": "nav.sub.mcp.overview",
  "mcp.servers": "nav.sub.mcp.servers",
  "mcp.configs": "nav.sub.mcp.configs",
  "prompts.overview": "nav.sub.prompts.overview",
  "prompts.templates": "nav.sub.prompts.templates",
  "prompts.targets": "nav.sub.prompts.targets",
  "rules.overview": "nav.sub.rules.overview",
  "rules.policies": "nav.sub.rules.policies",
  "rules.conflicts": "nav.sub.rules.conflicts",
  "profiles.overview": "nav.sub.profiles.overview",
  "profiles.templates": "nav.sub.profiles.templates",
  "profiles.plans": "nav.sub.profiles.plans",
};

const subNavDefaultLabels: Partial<Record<string, string[]>> = {
  "skills.overview": ["Catalog Overview", "目录总览"],
  "skills.groups": ["Groups", "分组管理"],
  "skills.sources": ["Skill Sources", "技能源管理", "Skill 源管理"],
  "skills.mounts": ["Mounts", "挂载管理"],
  "mcp.overview": ["Service Overview", "服务总览"],
  "mcp.servers": ["Servers", "服务管理", "Server 管理"],
  "mcp.configs": ["Config Projection", "配置投影"],
  "prompts.overview": ["Prompt Overview", "提示词总览", "Prompt 总览"],
  "prompts.templates": ["Templates", "模板管理"],
  "prompts.targets": ["Target Apps", "目标应用", "目标 App"],
  "rules.overview": ["Rule Overview", "规则总览"],
  "rules.policies": ["Policies", "启用策略"],
  "rules.conflicts": ["Conflict Detection", "冲突检测"],
  "profiles.overview": ["App Overview", "应用总览", "App 总览"],
  "profiles.templates": ["Profile Templates", "配置模板", "Profile 模板"],
  "profiles.plans": ["Deployment Plans", "部署计划"],
};

export function railLabel(item: RailMenuItem, t: Translator) {
  return translateByKey(railLabelKeys[item.id], item.label, t, railDefaultLabels[item.id]);
}

export function headerTabLabel(item: HeaderTabItem, t: Translator) {
  return translateByKey(headerLabelKeys[item.id], item.label, t, headerDefaultLabels[item.id]);
}

export function subNavLabel(item: SubNavItem, t: Translator) {
  return translateByKey(subNavLabelKeys[item.routeKey], item.label, t, subNavDefaultLabels[item.routeKey]);
}

function translateByKey(key: TranslationKey | undefined, fallback: string, t: Translator, defaultLabels: string[] = []) {
  if (!key) {
    return fallback;
  }

  return isDefaultLabel(fallback, defaultLabels) ? t(key) : fallback;
}

function isDefaultLabel(label: string, defaultLabels: string[]) {
  return defaultLabels.includes(label.trim());
}
