import clsx from "clsx";
import {
  CircleHelp,
  Code2,
  LoaderCircle,
  RefreshCw,
  Search,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { useI18n, type Translator } from "../../i18n/I18nProvider";
import type { AppShortcut } from "../../types";
import {
  checkAgentConnection,
  listAgentCatalog,
  listAgentModels,
  type AgentModelOption,
  type AgentConnectionResult,
  type AgentInstallPreview,
  type AgentModelsResult,
  type AgentUninstallPreview,
} from "../../services/agentRuntime";
import * as agentRuntime from "../../services/agentRuntime";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { AgentConnectionRow } from "./AgentConnectionRow";
import { AgentInstallPreviewDialog } from "./AgentInstallPreviewDialog";
import { AgentUninstallPreviewDialog } from "./AgentUninstallPreviewDialog";
import {
  agentCatalog,
  initialConnectionStates,
  registryAgentIds,
  type AgentCatalogItem,
  type AgentConnectionState,
  type AgentFilter,
  type AgentId,
  marketItemToCatalogItem,
} from "./agentCatalog";

export function AgentSettingsPanel({
  appShortcuts = [],
  focusAgentId,
  selectedModels,
  onModelChange,
  view = "market",
}: {
  appShortcuts?: AppShortcut[];
  focusAgentId?: string | null;
  selectedModels: Record<string, string>;
  onModelChange: (agentId: AgentId, modelId: string) => void;
  view?: "market" | "settings";
}) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<AgentFilter>("all");
  const [connectionStates, setConnectionStates] = useState(() => ({ ...initialConnectionStates }));
  const [marketCatalog, setMarketCatalog] = useState<AgentCatalogItem[] | null>(null);
  const [connectionMessages, setConnectionMessages] = useState<Record<string, string>>({});
  const [testingAgentId, setTestingAgentId] = useState<AgentId | null>(null);
  const [marketBusyAgentId, setMarketBusyAgentId] = useState<AgentId | null>(null);
  const [marketRefreshBusy, setMarketRefreshBusy] = useState(false);
  const [pendingLifecycle, setPendingLifecycle] = useState<{
    agent: AgentCatalogItem;
    action: "install" | "update" | "reinstall";
    preview: AgentInstallPreview;
  } | null>(null);
  const [pendingUninstall, setPendingUninstall] = useState<{
    agent: AgentCatalogItem;
    preview: AgentUninstallPreview;
  } | null>(null);
  const [infoAgent, setInfoAgent] = useState<AgentCatalogItem | null>(null);
  const [modelAgent, setModelAgent] = useState<AgentCatalogItem | null>(null);
  const [modelResult, setModelResult] = useState<AgentModelsResult | null>(null);
  const [modelQuery, setModelQuery] = useState("");
  const [modelLoading, setModelLoading] = useState(false);
  const [modelError, setModelError] = useState("");
  const modelRequestId = useRef(0);
  const agentRowRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const canRefreshAgentMarket = Object.prototype.hasOwnProperty.call(agentRuntime, "refreshAgentMarket");
  const canPreviewAgentUninstall = Object.prototype.hasOwnProperty.call(agentRuntime, "previewAgentUninstall");
  const canManageAgentLifecycle = typeof agentRuntime.listAgentMarket === "function";
  const settingsOnly = view === "settings";

  useEffect(() => {
    let disposed = false;
    if (typeof agentRuntime.listAgentMarket === "function") {
      void agentRuntime.listAgentMarket()
        .then((items) => {
          if (disposed) return;
          if (items.length === 0) return;
          const dynamicCatalog = items.map(marketItemToCatalogItem);
          setMarketCatalog(dynamicCatalog);
          setConnectionStates(Object.fromEntries(items.map((item) => [
            item.id,
            item.installed?.executionReady ? "available" : item.installed ? "failed" : "not-installed",
          ])));
        })
        .catch(() => undefined);
      return () => {
        disposed = true;
      };
    }

    async function checkInstalledAgents() {
      let ids = registryAgentIds;
      try {
        const runtimeCatalog = await listAgentCatalog();
        const runtimeIds = runtimeCatalog
          .map((entry) => entry.id)
          .filter((id): id is AgentId => registryAgentIds.includes(id as AgentId));
        if (runtimeIds.length > 0) {
          ids = runtimeIds;
        }
      } catch {
        // Browser preview and older desktop builds use the static registry
        // list; the individual probes below still report their real result.
      }

      if (disposed) return;
      setConnectionStates((current) => ({
        ...current,
        ...Object.fromEntries(ids.map((id) => [id, "checking"])),
      }));

      await Promise.all(ids.map(async (id) => {
        try {
          const result = await checkAgentConnection(id, "installation");
          if (disposed) return;
          applyConnectionResult(id, result, "installation", t, setConnectionStates, setConnectionMessages);
        } catch (error) {
          if (disposed) return;
          setConnectionStates((current) => ({ ...current, [id]: "failed" }));
          setConnectionMessages((current) => ({ ...current, [id]: errorMessage(error) }));
        }
      }));
    }

    void checkInstalledAgents();

    return () => {
      disposed = true;
    };
  }, [t]);

  const displayedCatalog = (marketCatalog ?? agentCatalog).filter((agent) =>
    view === "market" || Boolean(agent.installed));

  const filteredAgents = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return displayedCatalog.filter((agent) => {
      const state = connectionStates[agent.id];
      const matchesFilter = filter === "all"
        || (filter === "available" && state === "available")
        || (filter === "unavailable" && state !== "available" && state !== "checking");
      const matchesQuery = !normalizedQuery
        || `${agent.name} ${agent.command} ${agent.protocol} ${agent.description}`.toLowerCase().includes(normalizedQuery);
      return matchesFilter && matchesQuery;
    });
  }, [connectionStates, displayedCatalog, filter, query]);

  const displayedAgentIds = new Set(displayedCatalog.map((agent) => agent.id));
  const availableCount = Object.entries(connectionStates).filter(
    ([id, state]) => displayedAgentIds.has(id) && state === "available",
  ).length;
  const unavailableCount = Object.entries(connectionStates).filter(
    ([id, state]) => displayedAgentIds.has(id) && state !== "available" && state !== "checking",
  ).length;

  useEffect(() => {
    if (!focusAgentId) {
      return;
    }
    const row = agentRowRefs.current[focusAgentId];
    if (!row) {
      return;
    }
    row.scrollIntoView?.({ block: "center", behavior: "smooth" });
  }, [filteredAgents, focusAgentId]);

  async function testConnection(agent: AgentCatalogItem) {
    setTestingAgentId(agent.id);
    setConnectionStates((current) => ({ ...current, [agent.id]: "checking" }));
    setConnectionMessages((current) => ({ ...current, [agent.id]: "" }));
    try {
      const result = await checkAgentConnection(agent.id, "connection");
      applyConnectionResult(agent.id, result, "connection", t, setConnectionStates, setConnectionMessages);
    } catch (error) {
      setConnectionStates((current) => ({ ...current, [agent.id]: "failed" }));
      setConnectionMessages((current) => ({ ...current, [agent.id]: errorMessage(error) }));
    } finally {
      setTestingAgentId(null);
    }
  }

  async function reloadMarketCatalog() {
    const items = await agentRuntime.listAgentMarket();
    setMarketCatalog(items.map(marketItemToCatalogItem));
    setConnectionStates(Object.fromEntries(items.map((item) => [
      item.id,
      item.installed?.executionReady ? "available" : item.installed ? "failed" : "not-installed",
    ])));
  }

  async function refreshMarketCatalog() {
    if (typeof agentRuntime.refreshAgentMarket !== "function") return;
    setMarketRefreshBusy(true);
    try {
      let snapshot = await agentRuntime.refreshAgentMarket();
      while (snapshot.state === "running") {
        await new Promise((resolve) => window.setTimeout(resolve, 300));
        snapshot = await agentRuntime.getAgentMarketRefreshTask(snapshot.id);
      }
      if (snapshot.state === "failed") {
        setConnectionMessages((current) => ({ ...current, _market: snapshot.error || t("settings.agents.refreshFailed") }));
        return;
      }
      await reloadMarketCatalog();
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, _market: errorMessage(error) }));
    } finally {
      setMarketRefreshBusy(false);
    }
  }

  async function runMarketLifecycle(agent: AgentCatalogItem, action: "install" | "update" | "reinstall") {
    if (typeof agentRuntime.listAgentMarket !== "function") return;
    setMarketBusyAgentId(agent.id);
    try {
      if (action === "install" && agent.installed) {
        const result = agent.installed.enabled
          ? await agentRuntime.disableAgent(agent.id)
          : await agentRuntime.enableAgent(agent.id);
        setConnectionStates((current) => ({
          ...current,
          [agent.id]: result.executionReady ? "available" : "failed",
        }));
        setMarketCatalog((current) => current?.map((item) => item.id === agent.id
          ? { ...item, installed: result, updateAvailable: result.updateAvailable }
          : item) ?? current);
        return;
      }

      const preview = await agentRuntime.previewAgentInstallation({ agentId: agent.id, action });
      if (preview.conflicts.length > 0) {
        setConnectionMessages((current) => ({ ...current, [agent.id]: preview.conflicts.join(", ") }));
        return;
      }
      setPendingLifecycle({ agent, action, preview });
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, [agent.id]: errorMessage(error) }));
    } finally {
      setMarketBusyAgentId(null);
    }
  }

  async function selectLifecycleDistribution(distributionId: string) {
    if (!pendingLifecycle || distributionId === pendingLifecycle.preview.selectedDistribution.distributionId) return;
    setMarketBusyAgentId(pendingLifecycle.agent.id);
    try {
      const preview = await agentRuntime.previewAgentInstallation({
        agentId: pendingLifecycle.agent.id,
        action: pendingLifecycle.action,
        distributionId,
      });
      setPendingLifecycle((current) => current ? { ...current, preview } : current);
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, [pendingLifecycle.agent.id]: errorMessage(error) }));
    } finally {
      setMarketBusyAgentId(null);
    }
  }

  async function confirmLifecycle() {
    if (!pendingLifecycle) return;
    const { agent, action, preview } = pendingLifecycle;
    setPendingLifecycle(null);
    setMarketBusyAgentId(agent.id);
    try {
      const request = {
        agentId: agent.id,
        catalogVersion: preview.catalogVersion,
        agentVersion: preview.targetVersion,
        distributionId: preview.selectedDistribution.distributionId,
        previewToken: preview.previewToken,
      };
      const task = action === "update"
        ? await agentRuntime.startAgentUpdate(request)
        : action === "reinstall"
          ? await agentRuntime.startAgentReinstallation(request)
          : await agentRuntime.startAgentInstallation(request);
      let snapshot = task;
      while (snapshot.state === "queued" || snapshot.state === "running") {
        await new Promise((resolve) => window.setTimeout(resolve, 300));
        snapshot = await agentRuntime.getAgentLifecycleTask(snapshot.id);
      }
      if (snapshot.state === "failed") {
        setConnectionMessages((current) => ({ ...current, [agent.id]: snapshot.error?.message || "安装失败" }));
      }
      await reloadMarketCatalog();
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, [agent.id]: errorMessage(error) }));
    } finally {
      setMarketBusyAgentId(null);
    }
  }

  async function previewUninstall(agent: AgentCatalogItem) {
    if (typeof agentRuntime.previewAgentUninstall !== "function") return;
    setMarketBusyAgentId(agent.id);
    try {
      const preview = await agentRuntime.previewAgentUninstall(agent.id);
      setPendingUninstall({ agent, preview });
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, [agent.id]: errorMessage(error) }));
    } finally {
      setMarketBusyAgentId(null);
    }
  }

  async function confirmUninstall(clearCapabilityAssignments: string[]) {
    if (!pendingUninstall) return;
    const { agent, preview } = pendingUninstall;
    setPendingUninstall(null);
    setMarketBusyAgentId(agent.id);
    try {
      const task = await agentRuntime.startAgentUninstall({
        agentId: agent.id,
        clearCapabilityAssignments,
        previewToken: preview.previewToken,
      });
      let snapshot = task;
      while (snapshot.state === "queued" || snapshot.state === "running") {
        await new Promise((resolve) => window.setTimeout(resolve, 300));
        snapshot = await agentRuntime.getAgentLifecycleTask(snapshot.id);
      }
      if (snapshot.state === "failed") {
        setConnectionMessages((current) => ({ ...current, [agent.id]: snapshot.error?.message || t("settings.agents.uninstallFailed") }));
      }
      await reloadMarketCatalog();
    } catch (error) {
      setConnectionMessages((current) => ({ ...current, [agent.id]: errorMessage(error) }));
    } finally {
      setMarketBusyAgentId(null);
    }
  }

  async function manageMarketAgent(agent: AgentCatalogItem) {
    await runMarketLifecycle(agent, "install");
  }

  function openModelDialog(agent: AgentCatalogItem) {
    const requestId = modelRequestId.current + 1;
    modelRequestId.current = requestId;
    setModelAgent(agent);
    setModelResult(null);
    setModelQuery("");
    setModelError("");
    setModelLoading(true);
    void listAgentModels(agent.id)
      .then((result) => {
        if (modelRequestId.current !== requestId) return;
        setModelResult(result);
        setModelError(result.error || "");
      })
      .catch((error: unknown) => {
        if (modelRequestId.current !== requestId) return;
        setModelError(errorMessage(error));
      })
      .finally(() => {
        if (modelRequestId.current === requestId) {
          setModelLoading(false);
        }
      });
  }

  function closeModelDialog() {
    modelRequestId.current += 1;
    setModelAgent(null);
    setModelResult(null);
    setModelQuery("");
    setModelError("");
    setModelLoading(false);
  }

  const allModelOptions = modelResult?.models || [];
  const modelOptions = allModelOptions.filter((model) => {
    const normalizedQuery = modelQuery.trim().toLowerCase();
    return !normalizedQuery || `${model.label} ${model.id} ${model.description || ""}`.toLowerCase().includes(normalizedQuery);
  }) || [];
  const selectedModel = modelAgent
    ? selectedModels[modelAgent.id] || modelResult?.current_model_id || ""
    : "";
  const selectedModelOption = allModelOptions.find((model) => model.id === selectedModel);
  const unselectedModelOptions = modelOptions.filter((model) => model.id !== selectedModel);

  return (
    <div className="mx-auto w-full max-w-5xl space-y-5 pb-8">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-3">
            <h1 className="text-h1 text-on-surface">{t("settings.agents.title")}</h1>
            <Badge tone="primary">ACP</Badge>
          </div>
          <p className="mt-2 max-w-3xl text-body-md leading-6 text-on-surface-variant">
            {t("settings.agents.description")}
          </p>
          <p className="mt-1 text-body-sm text-outline">{t("settings.agents.registryHint")}</p>
        </div>
      </div>

      {view === "market" && canRefreshAgentMarket ? (
        <div className="flex justify-end">
          <Button
            aria-label={t("settings.agents.refresh")}
            disabled={marketRefreshBusy}
            onClick={() => void refreshMarketCatalog()}
            size="icon"
            title={t("settings.agents.refresh")}
            type="button"
            variant="ghost"
          >
            <RefreshCw className={marketRefreshBusy ? "animate-spin" : undefined} size={16} />
          </Button>
        </div>
      ) : null}
      {connectionMessages._market ? (
        <p className="rounded-xl border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove">
          {connectionMessages._market}
        </p>
      ) : null}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <label className="relative min-w-48 max-w-md flex-1">
            <Search aria-hidden="true" className="absolute left-3 top-1/2 -translate-y-1/2 text-outline" size={17} />
            <Input
              aria-label={t("settings.agents.search")}
              className="h-10 pl-10"
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("settings.agents.searchPlaceholder")}
              type="search"
              value={query}
            />
          </label>
        </div>
        <div className="flex shrink-0 items-center gap-1 rounded-xl border border-theme-card-border bg-theme-card/55 p-1" role="tablist">
          <AgentFilterButton count={displayedCatalog.length} filter="all" onChange={setFilter} selected={filter} t={t} />
          <AgentFilterButton count={availableCount} filter="available" onChange={setFilter} selected={filter} t={t} />
          <AgentFilterButton count={unavailableCount} filter="unavailable" onChange={setFilter} selected={filter} t={t} />
        </div>
      </div>

      <section aria-label={t("settings.agents.listLabel")} className="rounded-2xl border border-theme-card-border bg-theme-card/65 p-2.5 shadow-[var(--theme-shadow-card)]">
        <div className="space-y-2">
          {filteredAgents.map((agent) => (
            <div
              className={clsx(focusAgentId === agent.id && "rounded-xl ring-2 ring-theme-nav-active-border/70 ring-offset-2 ring-offset-theme-card")}
              key={agent.id}
              ref={(element) => {
                agentRowRefs.current[agent.id] = element;
              }}
            >
              <AgentConnectionRow
                agent={agent}
                appShortcuts={appShortcuts}
                connectionMessage={connectionMessages[agent.id]}
                connectionState={connectionStates[agent.id]}
                isTesting={testingAgentId === agent.id}
                isManaging={marketBusyAgentId === agent.id}
                onInstall={!settingsOnly && canManageAgentLifecycle ? () => void manageMarketAgent(agent) : undefined}
                onUpdate={!settingsOnly && canManageAgentLifecycle ? () => void runMarketLifecycle(agent, "update") : undefined}
                onReinstall={!settingsOnly && canManageAgentLifecycle ? () => void runMarketLifecycle(agent, "reinstall") : undefined}
                onUninstall={!settingsOnly && canPreviewAgentUninstall ? () => void previewUninstall(agent) : undefined}
                onEdit={settingsOnly ? () => setInfoAgent(agent) : undefined}
                onSelectModel={settingsOnly ? () => openModelDialog(agent) : undefined}
                onTest={settingsOnly ? () => void testConnection(agent) : undefined}
                selectedModel={selectedModels[agent.id]}
                t={t}
                view={view}
              />
            </div>
          ))}
          {filteredAgents.length === 0 ? (
            <div className="grid min-h-40 place-items-center rounded-xl border border-dashed border-theme-card-border px-4 text-center">
              <div>
                <CircleHelp className="mx-auto text-outline" size={22} />
                <p className="mt-2 text-body-sm font-semibold text-on-surface">{t("settings.agents.emptyTitle")}</p>
                <p className="mt-1 text-body-sm text-on-surface-variant">{t("settings.agents.emptyDescription")}</p>
              </div>
            </div>
          ) : null}
        </div>
      </section>

      {modelAgent ? (
        <DialogFrame
          closeLabel={t("common.close")}
          contentClassName="grid gap-4"
          description={t("settings.agents.modelDialogDescription")}
          footer={
            <Button onClick={closeModelDialog} type="button" variant="outline">
              {t("common.close")}
            </Button>
          }
          icon={<Code2 size={18} />}
          onClose={closeModelDialog}
          size="lg"
          title={`${t("settings.agents.modelDialogTitle")} · ${modelAgent.name}`}
        >
          <div className="grid gap-3">
            <Input
              aria-label={t("settings.agents.modelSearch")}
              onChange={(event) => setModelQuery(event.target.value)}
              placeholder={t("settings.agents.modelSearchPlaceholder")}
              type="search"
              value={modelQuery}
            />
            {modelLoading ? (
              <div className="flex items-center gap-2 rounded-xl border border-dashed border-theme-card-border px-4 py-8 text-body-sm text-on-surface-variant">
                <LoaderCircle className="animate-spin" size={17} />
                {t("settings.agents.modelLoading")}
              </div>
            ) : allModelOptions.length > 0 ? (
              <div className="grid gap-3">
                {selectedModelOption ? (
                  <section
                    aria-label={t("settings.agents.modelCurrentSection")}
                    className="rounded-xl border border-theme-nav-active-border/65 bg-theme-nav-active/12 p-2.5"
                  >
                    <p className="mb-2 px-1 text-label-caps uppercase text-primary">
                      {t("settings.agents.modelCurrentSection")}
                    </p>
                    <div role="radiogroup">
                      <ModelOptionButton
                        model={selectedModelOption}
                        onSelect={() => undefined}
                        selected
                        t={t}
                      />
                    </div>
                  </section>
                ) : null}
                <section aria-label={t("settings.agents.modelAvailableSection")} className="grid gap-2" role="radiogroup">
                  <p className="px-1 text-label-caps uppercase text-outline">
                    {t("settings.agents.modelAvailableSection")}
                  </p>
                  {unselectedModelOptions.length > 0 ? unselectedModelOptions.map((model) => (
                    <ModelOptionButton
                      key={model.id}
                      model={model}
                      onSelect={() => {
                        onModelChange(modelAgent.id, model.id);
                        setModelResult((current) => current
                          ? { ...current, current_model_id: model.id }
                          : current);
                      }}
                      selected={false}
                      t={t}
                    />
                  )) : (
                    <p className="rounded-xl border border-dashed border-theme-card-border px-3 py-4 text-center text-body-sm text-on-surface-variant">
                      {t("settings.agents.modelNoMatches")}
                    </p>
                  )}
                </section>
              </div>
            ) : (
              <div className="rounded-xl border border-dashed border-theme-card-border px-4 py-8 text-center text-body-sm text-on-surface-variant">
                <p>{modelError || t("settings.agents.modelEmpty")}</p>
                {modelResult?.available && modelResult.models.length === 0 ? (
                  <p className="mt-1 text-code-sm text-outline">{t("settings.agents.modelEmptyHint")}</p>
                ) : null}
              </div>
            )}
            {modelError && allModelOptions.length > 0 ? (
              <p className="text-body-sm text-status-remove">{modelError}</p>
            ) : null}
          </div>
        </DialogFrame>
      ) : null}

      {pendingLifecycle ? (
        <AgentInstallPreviewDialog
          agent={pendingLifecycle.agent}
          busy={marketBusyAgentId === pendingLifecycle.agent.id}
          onClose={() => setPendingLifecycle(null)}
          onConfirm={() => void confirmLifecycle()}
          onSelectDistribution={(distributionId) => void selectLifecycleDistribution(distributionId)}
          preview={pendingLifecycle.preview}
        />
      ) : null}

      {pendingUninstall ? (
        <AgentUninstallPreviewDialog
          agent={pendingUninstall.agent}
          busy={marketBusyAgentId === pendingUninstall.agent.id}
          onClose={() => setPendingUninstall(null)}
          onConfirm={(assignments) => void confirmUninstall(assignments)}
          preview={pendingUninstall.preview}
        />
      ) : null}

      {infoAgent ? (
        <DialogFrame
          closeLabel={t("common.close")}
          contentClassName="grid gap-4"
          description={t("settings.agents.definitionDialogDescription")}
          footer={
            <Button onClick={() => setInfoAgent(null)} type="button" variant="outline">
              {t("common.close")}
            </Button>
          }
          icon={<Code2 size={18} />}
          onClose={() => setInfoAgent(null)}
          size="md"
          title={infoAgent.name}
        >
          <div className="grid gap-3 text-body-sm">
            <DefinitionValue label={t("settings.agents.command")} value={infoAgent.command} />
            <DefinitionValue label={t("settings.agents.protocol")} value={infoAgent.protocol} />
            <p className="rounded-xl border border-status-update/25 bg-status-update/10 px-3 py-3 leading-6 text-on-surface-variant">
              {t("settings.agents.definitionEditingHint")}
            </p>
          </div>
        </DialogFrame>
      ) : null}

    </div>
  );
}

function ModelOptionButton({
  model,
  onSelect,
  selected,
  t,
}: {
  model: AgentModelOption;
  onSelect: () => void;
  selected: boolean;
  t: Translator;
}) {
  return (
    <button
      aria-checked={selected}
      className={clsx(
        "flex items-start gap-3 rounded-xl border px-3.5 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55",
        selected
          ? "border-theme-nav-active-border bg-theme-nav-active/20"
          : "border-theme-card-border bg-theme-control/60 hover:border-theme-nav-active-border/60 hover:bg-theme-control-hover",
      )}
      onClick={onSelect}
      role="radio"
      type="button"
    >
      <span className="mt-0.5 grid size-5 shrink-0 place-items-center rounded-full border border-theme-control-border">
        {selected ? <span className="size-2.5 rounded-full bg-primary" /> : null}
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-2 text-body-sm font-semibold text-on-surface">
          <span className="truncate">{model.label}</span>
          {selected ? <Badge tone="primary">{t("settings.agents.modelSelected")}</Badge> : null}
        </span>
        <span className="mt-1 block truncate font-mono text-code-sm text-outline">{model.id}</span>
        {model.description ? <span className="mt-1 block text-body-sm text-on-surface-variant">{model.description}</span> : null}
      </span>
    </button>
  );
}

function AgentFilterButton({
  count,
  filter,
  onChange,
  selected,
  t,
}: {
  count: number;
  filter: AgentFilter;
  onChange: (filter: AgentFilter) => void;
  selected: AgentFilter;
  t: Translator;
}) {
  const label = filter === "all"
    ? t("settings.agents.filterAll")
    : filter === "available"
      ? t("settings.agents.filterAvailable")
      : t("settings.agents.filterUnavailable");
  return (
    <button
      aria-selected={selected === filter}
      className={clsx(
        "inline-flex h-9 items-center gap-1.5 rounded-lg px-3 text-body-sm font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55",
        selected === filter
          ? "bg-theme-nav-active text-theme-nav-active-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.18)]"
          : "text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
      )}
      onClick={() => onChange(filter)}
      role="tab"
      type="button"
    >
      {label}
      <span className="rounded-full bg-theme-control/80 px-1.5 py-0.5 text-code-sm">{count}</span>
    </button>
  );
}

function DefinitionValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <code className="rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-code-sm text-on-surface">{value}</code>
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function applyConnectionResult(
  agentId: AgentId,
  result: AgentConnectionResult,
  mode: "installation" | "connection",
  t: Translator,
  setConnectionStates: Dispatch<SetStateAction<Record<AgentId, AgentConnectionState>>>,
  setConnectionMessages: Dispatch<SetStateAction<Record<string, string>>>,
) {
  const state: AgentConnectionState = result.available
    ? "available"
    : !result.installed || (mode === "installation" && result.error_code === "command_not_found")
      ? "not-installed"
      : "failed";
  const message = result.available
      ? result.version || t("settings.agents.connectionAvailable")
      : result.error || t("settings.agents.connectionFailed");
  setConnectionStates((current) => ({ ...current, [agentId]: state }));
  setConnectionMessages((current) => ({ ...current, [agentId]: message }));
}
