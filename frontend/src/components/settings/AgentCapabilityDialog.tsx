import clsx from "clsx";
import { ArrowUpRight, Bot, Check, CircleHelp, LoaderCircle, PlugZap } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import {
  checkAgentConnection,
  type AgentConnectionResult,
} from "../../services/agentRuntime";
import type { AppShortcut } from "../../types";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { AgentCatalogIcon, resolveAgentIconAccentColor } from "./AgentCatalogIcon";
import {
  agentCatalog,
  registryAgentIds,
  type AgentCatalogItem,
  type AgentConnectionState,
} from "./agentCatalog";

export function AgentCapabilityDialog({
  agentId,
  appShortcuts = [],
  model,
  onAgentChange,
  onClose,
  onOpenAgentSettings,
}: {
  agentId: string;
  appShortcuts?: AppShortcut[];
  model?: string;
  onAgentChange: (agentId: string) => void;
  onClose: () => void;
  onOpenAgentSettings: (agentId: string) => void;
}) {
  const { t } = useI18n();
  const [connectionStates, setConnectionStates] = useState<Record<string, AgentConnectionState>>({});
  const capabilityAgents = useMemo(
    () => agentCatalog.filter((agent) => agent.connectionMode === "registry"),
    [],
  );
  const selectedAgent = capabilityAgents.find((agent) => agent.id === agentId)
    ?? agentCatalog.find((agent) => agent.id === agentId);
  const availableCount = Object.values(connectionStates).filter((state) => state === "available").length;
  const checkingCount = Object.values(connectionStates).filter((state) => state === "checking").length;

  useEffect(() => {
    let cancelled = false;
    const ids = registryAgentIds;
    setConnectionStates(Object.fromEntries(ids.map((id) => [id, "checking"])));

    async function checkAvailability() {
      await Promise.all(ids.map(async (id) => {
        try {
          const result = await checkAgentConnection(id, "installation");
          if (!cancelled) {
            setConnectionStates((current) => ({ ...current, [id]: connectionStateFromResult(result) }));
          }
        } catch {
          if (!cancelled) {
            setConnectionStates((current) => ({ ...current, [id]: "failed" }));
          }
        }
      }));
    }

    void checkAvailability();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <DialogFrame
      className="max-h-[min(34rem,calc(100vh-4rem))]"
      closeLabel={t("common.close")}
      contentClassName="!overflow-y-auto !overflow-x-hidden p-0 overscroll-contain"
      description={t("settings.agentCapabilities.dialogDescription")}
      headerActions={
        <Badge tone="neutral">
          {checkingCount > 0
            ? t("settings.agentCapabilities.dialogChecking", { count: checkingCount })
            : t("settings.agentCapabilities.dialogCount", {
              available: availableCount,
              total: capabilityAgents.length,
            })}
        </Badge>
      }
      icon={<PlugZap size={18} />}
      onBackdropClick={onClose}
      onClose={onClose}
      overlayClassName="z-[60] px-4 py-6"
      size="lg"
      title={t("settings.agentCapabilities.dialogTitle")}
    >
      <div className="grid gap-3 px-4 py-4">
        <div className="flex items-center gap-3 rounded-xl border border-theme-nav-active-border/35 bg-theme-nav-active/10 px-3 py-3">
          <span
            className="grid size-10 shrink-0 place-items-center rounded-lg border border-theme-nav-active-border/45 bg-theme-card text-primary"
            style={{ color: selectedAgent ? resolveAgentIconAccentColor(selectedAgent, appShortcuts) : undefined }}
          >
            {selectedAgent ? (
              <AgentCatalogIcon agent={selectedAgent} appShortcuts={appShortcuts} className="size-5" fallbackSize={20} />
            ) : (
              <Bot aria-hidden="true" size={20} />
            )}
          </span>
          <div className="min-w-0">
            <p className="text-label-caps uppercase text-outline">
              {t("settings.agentCapabilities.selectedLabel")}
            </p>
            <p className="mt-1 truncate text-body-md font-semibold text-on-surface">
              {selectedAgent?.name ?? agentId}
            </p>
            <p className="mt-1 truncate text-body-sm text-on-surface-variant">
              {model
                ? t("settings.agentCapabilities.usingModel", { model })
                : t("settings.agentCapabilities.usingDefaultModel")}
            </p>
          </div>
        </div>

        <div
          aria-label={t("settings.agentCapabilities.dialogTitle")}
          className="min-w-0"
          role="list"
        >
          <div className="grid gap-2">
            {capabilityAgents.map((agent) => (
              <div key={agent.id} role="listitem">
                <CapabilityAgentOption
                  agent={agent}
                  connectionState={connectionStates[agent.id] ?? "not-tested"}
                  currentModel={agent.id === agentId ? model : undefined}
                  onOpenAgentSettings={() => onOpenAgentSettings(agent.id)}
                  onSelect={() => onAgentChange(agent.id)}
                  selected={agent.id === agentId}
                  appShortcuts={appShortcuts}
                  t={t}
                />
              </div>
            ))}
            {capabilityAgents.length === 0 ? (
              <div className="grid min-h-32 place-items-center rounded-xl border border-dashed border-theme-card-border text-body-sm text-outline">
                <CircleHelp size={18} />
                {t("settings.agentCapabilities.empty")}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </DialogFrame>
  );
}

function CapabilityAgentOption({
  agent,
  appShortcuts,
  connectionState,
  currentModel,
  onOpenAgentSettings,
  onSelect,
  selected,
  t,
}: {
  agent: AgentCatalogItem;
  appShortcuts: AppShortcut[];
  connectionState: AgentConnectionState;
  currentModel?: string;
  onOpenAgentSettings: () => void;
  onSelect: () => void;
  selected: boolean;
  t: Translator;
}) {
  const available = connectionState === "available";
  const statusLabel = agentStatusLabel(connectionState, t);
  const statusTone = agentStatusTone(connectionState);

  return (
    <div
      className={clsx(
        "flex items-center gap-3 rounded-xl border px-3 py-3 transition-[border-color,background-color,box-shadow] sm:px-4 sm:py-3",
        selected
          ? "border-theme-nav-active-border bg-theme-nav-active/12 shadow-[0_0_0_1px_rgb(var(--theme-nav-active-border)/0.22),0_12px_30px_rgb(var(--theme-panel-shadow)/0.12)]"
          : "border-theme-card-border/70 bg-theme-control/45 hover:border-theme-nav-active-border/55 hover:bg-theme-control-hover/65",
      )}
    >
      <button
        aria-pressed={selected}
        className="flex min-w-0 flex-1 items-center gap-3 text-left outline-none focus-visible:rounded-lg focus-visible:ring-2 focus-visible:ring-primary-strong/55"
        onClick={onSelect}
        type="button"
      >
        <span
          className="grid size-9 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-card text-primary sm:size-10"
          style={{ color: resolveAgentIconAccentColor(agent, appShortcuts) }}
        >
          <AgentCatalogIcon agent={agent} appShortcuts={appShortcuts} className="size-[19px]" fallbackSize={19} />
        </span>
        <span className="min-w-0">
          <span className="flex flex-wrap items-center gap-2">
            <span className="text-body-md font-semibold text-on-surface">{agent.name}</span>
            <Badge tone={statusTone}>{statusLabel}</Badge>
            {selected ? <Badge tone="primary">{t("settings.agentCapabilities.current")}</Badge> : null}
          </span>
          <span className="mt-1 block truncate text-body-xs text-on-surface-variant">
            {currentModel
              ? t("settings.agentCapabilities.usingModel", { model: currentModel })
              : t("settings.agentCapabilities.usingDefaultModel")}
          </span>
        </span>
      </button>
      <div className="flex shrink-0 items-center gap-1">
        {connectionState === "checking" ? <LoaderCircle className="animate-spin text-outline" size={16} /> : null}
        {selected && available ? <Check aria-hidden="true" className="text-status-create" size={17} /> : null}
        <Button
          aria-label={`${t("settings.agentCapabilities.openAgentSettings")} ${agent.name}`}
          onClick={onOpenAgentSettings}
          size="icon"
          title={t("settings.agentCapabilities.openAgentSettings")}
          type="button"
          variant="ghost"
        >
          <ArrowUpRight size={18} />
        </Button>
      </div>
    </div>
  );
}

function agentStatusLabel(state: AgentConnectionState, t: Translator) {
  if (state === "available") return t("settings.agents.statusAvailable");
  if (state === "checking") return t("settings.agents.statusChecking");
  if (state === "not-installed") return t("settings.agents.statusNotInstalled");
  if (state === "failed") return t("settings.agents.statusUnavailable");
  return t("settings.agents.statusNotTested");
}

function agentStatusTone(state: AgentConnectionState) {
  if (state === "available") return "create" as const;
  if (state === "checking") return "update" as const;
  return "neutral" as const;
}

function connectionStateFromResult(result: AgentConnectionResult): AgentConnectionState {
  if (result.available) return "available";
  if (!result.installed) return "not-installed";
  return "failed";
}
