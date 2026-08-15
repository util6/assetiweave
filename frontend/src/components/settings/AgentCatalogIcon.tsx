import clsx from "clsx";
import { AppShortcutIcon, appIconToken, supportsAppIcon } from "../apps/AppShortcutIcon";
import type { AgentCatalogItem } from "./agentCatalog";

const AGENT_ICON_TONE_CLASSES = {
  primary: {
    border: "border-primary/30",
    color: "text-primary",
  },
  update: {
    border: "border-status-update/30",
    color: "text-status-update",
  },
  create: {
    border: "border-status-create/30",
    color: "text-status-create",
  },
  neutral: {
    border: "border-theme-control-border",
    color: "text-on-surface-variant",
  },
} as const;

export function agentIconColorClass(tone: AgentCatalogItem["iconTone"]) {
  return AGENT_ICON_TONE_CLASSES[tone].color;
}

export function agentIconFrameClass(tone: AgentCatalogItem["iconTone"]) {
  const classes = AGENT_ICON_TONE_CLASSES[tone];
  return clsx(classes.border, classes.color);
}

export function AgentCatalogIcon({
  agent,
  className,
  fallbackSize = 22,
}: {
  agent: AgentCatalogItem;
  className?: string;
  fallbackSize?: number;
}) {
  const iconClassName = clsx(className, agentIconColorClass(agent.iconTone));

  if (supportsAppIcon(agent.id)) {
    return <AppShortcutIcon appKind={agent.id} className={iconClassName} displayIcon={appIconToken(agent.id)} />;
  }

  const FallbackIcon = agent.icon;
  return <FallbackIcon aria-hidden="true" className={iconClassName} size={fallbackSize} />;
}
