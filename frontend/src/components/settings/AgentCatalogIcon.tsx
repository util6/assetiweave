import clsx from "clsx";
import {
  AppShortcutIcon,
  appIconToken,
  resolveAppShortcutAccentColor,
  supportsAppIcon,
} from "../apps/AppShortcutIcon";
import type { AppShortcut } from "../../types";
import type { AgentCatalogItem } from "./agentCatalog";

export function resolveAgentIconAccentColor(agent: AgentCatalogItem, appShortcuts: AppShortcut[] = []) {
  return resolveAppShortcutAccentColor(
    { appKind: agent.id, profileName: agent.name },
    appShortcuts,
  );
}

export function agentIconFrameClass() {
  return "border-theme-control-border text-primary";
}

export function AgentCatalogIcon({
  agent,
  appShortcuts = [],
  className,
  fallbackSize = 22,
}: {
  agent: AgentCatalogItem;
  appShortcuts?: AppShortcut[];
  className?: string;
  fallbackSize?: number;
}) {
  const accentColor = resolveAgentIconAccentColor(agent, appShortcuts);
  const iconClassName = clsx(className, !accentColor && "text-primary");
  const style = accentColor ? { color: accentColor } : undefined;

  if (supportsAppIcon(agent.id)) {
    return (
      <AppShortcutIcon
        appKind={agent.id}
        className={iconClassName}
        displayIcon={appIconToken(agent.id)}
        style={style}
      />
    );
  }

  const FallbackIcon = agent.icon;
  return <FallbackIcon aria-hidden="true" className={iconClassName} size={fallbackSize} style={style} />;
}
