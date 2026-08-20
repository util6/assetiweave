import clsx from "clsx";
import { BrainCircuit, Info, LoaderCircle, Pencil, PlugZap } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import type { AppShortcut } from "../../types";
import {
  APP_SHORTCUT_ICON_FRAME_CLASS,
  appShortcutIconFrameStyle,
} from "../apps/AppShortcutIcon";
import { Badge } from "../foundation/Badge";
import { Button } from "../ui/button";
import { AgentCatalogIcon, agentIconFrameClass, resolveAgentIconAccentColor } from "./AgentCatalogIcon";
import type { AgentCatalogItem, AgentConnectionState } from "./agentCatalog";

export function AgentConnectionRow({
  agent,
  appShortcuts = [],
  connectionMessage,
  connectionState,
  isTesting,
  isManaging,
  onSelectModel,
  onEdit,
  onTest,
  onInstall,
  onUpdate,
  onReinstall,
  onUninstall,
  selectedModel,
  view = "market",
  t,
}: {
  agent: AgentCatalogItem;
  appShortcuts?: AppShortcut[];
  connectionMessage?: string;
  connectionState: AgentConnectionState;
  isTesting: boolean;
  isManaging?: boolean;
  onSelectModel?: () => void;
  onEdit?: () => void;
  onTest?: () => void;
  onInstall?: () => void;
  onUpdate?: () => void;
  onReinstall?: () => void;
  onUninstall?: () => void;
  selectedModel?: string;
  view?: "market" | "settings";
  t: Translator;
}) {
  const status = statusMeta(connectionState, t);
  const accentColor = resolveAgentIconAccentColor(agent, appShortcuts);
  const lifecycleBlocked = agent.coreCompatible === false || agent.hasSelectableDistribution === false;
  const lifecycleBlockReason = agent.coreCompatible === false
    ? t("settings.agents.coreIncompatible")
    : agent.hasSelectableDistribution === false
      ? t("settings.agents.distributionUnavailable")
      : undefined;
  return (
    <article className="group flex flex-wrap items-center gap-3 rounded-xl border border-theme-card-border/60 bg-theme-control/58 px-3.5 py-3 transition-[background,border-color,transform] hover:-translate-y-px hover:border-theme-nav-active-border/60 hover:bg-theme-control-hover/70 sm:flex-nowrap">
      <span
        className={clsx(APP_SHORTCUT_ICON_FRAME_CLASS, "size-11 shrink-0 bg-theme-card", agentIconFrameClass())}
        style={appShortcutIconFrameStyle(accentColor)}
      >
        <AgentCatalogIcon agent={agent} appShortcuts={appShortcuts} className="size-[22px]" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-title-sm font-semibold text-on-surface">{agent.name}</h3>
          <Badge tone={status.tone}>{status.label}</Badge>
          {lifecycleBlockReason ? <Badge tone="remove">{lifecycleBlockReason}</Badge> : null}
          {connectionState === "not-installed" ? <Info aria-label={t("settings.agents.statusNotInstalled")} className="text-outline" size={15} /> : null}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-code-sm text-outline">
          <span>{agent.command}</span>
          <span aria-hidden="true">·</span>
          <span>{agent.protocol}</span>
          {connectionMessage ? <span className="max-w-[28rem] truncate" title={connectionMessage}>· {connectionMessage}</span> : null}
        </div>
      </div>
      <div className="flex w-full shrink-0 items-center justify-end gap-2 sm:w-auto">
        {view === "market" && onInstall ? (
          <Button disabled={isManaging || lifecycleBlocked} onClick={onInstall} title={lifecycleBlockReason} type="button">
            {isManaging ? t("settings.agents.installing") : agent.installed ? (agent.installed.enabled ? t("settings.agents.disable") : t("settings.agents.enable")) : t("settings.agents.install")}
          </Button>
        ) : null}
        {view === "market" && onUpdate && agent.updateAvailable ? (
          <Button disabled={isManaging || lifecycleBlocked} onClick={onUpdate} title={lifecycleBlockReason} type="button" variant="outline">
            {isManaging ? t("settings.agents.installing") : t("settings.agents.update")}
          </Button>
        ) : null}
        {view === "market" && onReinstall && agent.installed ? (
          <Button disabled={isManaging || lifecycleBlocked} onClick={onReinstall} title={lifecycleBlockReason} type="button" variant="outline">
            {isManaging ? t("settings.agents.installing") : t("settings.agents.reinstall")}
          </Button>
        ) : null}
        {view === "market" && onUninstall && agent.installed ? (
          <Button disabled={isManaging} onClick={onUninstall} type="button" variant="outline">
            {isManaging ? t("settings.agents.installing") : t("settings.agents.uninstall")}
          </Button>
        ) : null}
        {view === "settings" && onTest ? (
          <Button disabled={isTesting} onClick={onTest} type="button" variant="outline">
            {isTesting ? <LoaderCircle className="animate-spin" size={15} /> : <PlugZap size={15} />}
            <span>{isTesting ? t("settings.agents.testing") : t("settings.agents.testConnection")}</span>
          </Button>
        ) : null}
        {view === "settings" && onSelectModel ? (
          <Button
            aria-label={`${t("settings.agents.model")} ${agent.name}`}
            onClick={onSelectModel}
            title={selectedModel || t("settings.agents.model")}
            type="button"
            variant="outline"
          >
            <BrainCircuit size={15} />
            <span className="max-w-36 truncate">{selectedModel || t("settings.agents.model")}</span>
          </Button>
        ) : null}
        {view === "settings" && onEdit ? (
          <Button aria-label={`${t("settings.agents.edit")} ${agent.name}`} onClick={onEdit} type="button" variant="outline">
            <Pencil size={15} />
            <span>{t("settings.agents.edit")}</span>
          </Button>
        ) : null}
      </div>
    </article>
  );
}

function statusMeta(state: AgentConnectionState, t: Translator) {
  if (state === "available") return { label: t("settings.agents.statusAvailable"), tone: "create" as const };
  if (state === "failed") return { label: t("settings.agents.statusUnavailable"), tone: "remove" as const };
  if (state === "not-installed") return { label: t("settings.agents.statusNotInstalled"), tone: "remove" as const };
  if (state === "checking") return { label: t("settings.agents.statusChecking"), tone: "update" as const };
  return { label: t("settings.agents.statusNotTested"), tone: "neutral" as const };
}
