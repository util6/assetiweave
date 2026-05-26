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

const headerLabelKeys: Partial<Record<string, TranslationKey>> = {
  skills: "nav.header.skills",
  mcp: "nav.header.mcp",
  prompts: "nav.header.prompts",
  rules: "nav.header.rules",
  profiles: "nav.header.profiles",
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

export function railLabel(item: RailMenuItem, t: Translator) {
  return translateByKey(railLabelKeys[item.id], item.label, t);
}

export function headerTabLabel(item: HeaderTabItem, t: Translator) {
  return translateByKey(headerLabelKeys[item.id], item.label, t);
}

export function subNavLabel(item: SubNavItem, t: Translator) {
  return translateByKey(subNavLabelKeys[item.routeKey], item.label, t);
}

function translateByKey(key: TranslationKey | undefined, fallback: string, t: Translator) {
  return key ? t(key) : fallback;
}
