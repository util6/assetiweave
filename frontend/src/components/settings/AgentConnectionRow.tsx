import clsx from "clsx";
import { BrainCircuit, Info, LoaderCircle, Pencil, PlugZap } from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import { Badge } from "../foundation/Badge";
import { Button } from "../ui/button";
import type { AgentCatalogItem, AgentConnectionState } from "./agentCatalog";

export function AgentConnectionRow({
  agent,
  connectionMessage,
  connectionState,
  isTesting,
  onSelectModel,
  onEdit,
  onTest,
  selectedModel,
  t,
}: {
  agent: AgentCatalogItem;
  connectionMessage?: string;
  connectionState: AgentConnectionState;
  isTesting: boolean;
  onSelectModel: () => void;
  onEdit: () => void;
  onTest: () => void;
  selectedModel?: string;
  t: Translator;
}) {
  const Icon = agent.icon;
  const status = statusMeta(connectionState, t);
  return (
    <article className="group flex flex-wrap items-center gap-3 rounded-xl border border-theme-card-border/60 bg-theme-control/58 px-3.5 py-3 transition-[background,border-color,transform] hover:-translate-y-px hover:border-theme-nav-active-border/60 hover:bg-theme-control-hover/70 sm:flex-nowrap">
      <span className={clsx("grid size-11 shrink-0 place-items-center rounded-xl border bg-theme-card", iconToneClass(agent.iconTone))}>
        <Icon aria-hidden="true" size={22} />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-title-sm font-semibold text-on-surface">{agent.name}</h3>
          <Badge tone={status.tone}>{status.label}</Badge>
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
        <Button disabled={isTesting} onClick={onTest} type="button" variant="outline">
          {isTesting ? <LoaderCircle className="animate-spin" size={15} /> : <PlugZap size={15} />}
          <span>{isTesting ? t("settings.agents.testing") : t("settings.agents.testConnection")}</span>
        </Button>
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
        <Button aria-label={`${t("settings.agents.edit")} ${agent.name}`} onClick={onEdit} type="button" variant="outline">
          <Pencil size={15} />
          <span>{t("settings.agents.edit")}</span>
        </Button>
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

function iconToneClass(tone: AgentCatalogItem["iconTone"]) {
  return {
    primary: "border-primary/30 text-primary",
    update: "border-status-update/30 text-status-update",
    create: "border-status-create/30 text-status-create",
    neutral: "border-theme-control-border text-on-surface-variant",
  }[tone];
}
