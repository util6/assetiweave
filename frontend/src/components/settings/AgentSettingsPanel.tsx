import clsx from "clsx";
import {
  ChevronDown,
  CircleHelp,
  Code2,
  LoaderCircle,
  Plus,
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
  type AgentModelsResult,
} from "../../services/agentRuntime";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { AgentConnectionRow } from "./AgentConnectionRow";
import {
  agentCatalog,
  initialConnectionStates,
  registryAgentIds,
  type AgentCatalogItem,
  type AgentConnectionState,
  type AgentFilter,
  type AgentId,
} from "./agentCatalog";

export function AgentSettingsPanel({
  appShortcuts = [],
  focusAgentId,
  selectedModels,
  onModelChange,
}: {
  appShortcuts?: AppShortcut[];
  focusAgentId?: string | null;
  selectedModels: Record<string, string>;
  onModelChange: (agentId: AgentId, modelId: string) => void;
}) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<AgentFilter>("all");
  const [connectionStates, setConnectionStates] = useState(() => ({ ...initialConnectionStates }));
  const [connectionMessages, setConnectionMessages] = useState<Record<string, string>>({});
  const [testingAgentId, setTestingAgentId] = useState<AgentId | null>(null);
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const [infoAgent, setInfoAgent] = useState<AgentCatalogItem | null>(null);
  const [modelAgent, setModelAgent] = useState<AgentCatalogItem | null>(null);
  const [modelResult, setModelResult] = useState<AgentModelsResult | null>(null);
  const [modelQuery, setModelQuery] = useState("");
  const [modelLoading, setModelLoading] = useState(false);
  const [modelError, setModelError] = useState("");
  const modelRequestId = useRef(0);
  const agentRowRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const [customDialogOpen, setCustomDialogOpen] = useState(false);
  const addMenuRef = useRef<HTMLDivElement>(null);
  const addMenuItemRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let disposed = false;
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

  useEffect(() => {
    if (!addMenuOpen) return;

    function handlePointerDown(event: PointerEvent) {
      if (!addMenuRef.current?.contains(event.target as Node)) {
        setAddMenuOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setAddMenuOpen(false);
      }
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    addMenuItemRef.current?.focus();
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [addMenuOpen]);

  const filteredAgents = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return agentCatalog.filter((agent) => {
      const state = connectionStates[agent.id];
      const matchesFilter = filter === "all"
        || (filter === "available" && state === "available")
        || (filter === "unavailable" && state !== "available" && state !== "checking");
      const matchesQuery = !normalizedQuery
        || `${agent.name} ${agent.command} ${agent.protocol} ${agent.description}`.toLowerCase().includes(normalizedQuery);
      return matchesFilter && matchesQuery;
    });
  }, [connectionStates, filter, query]);

  const availableCount = Object.values(connectionStates).filter((state) => state === "available").length;
  const unavailableCount = Object.values(connectionStates).filter(
    (state) => state !== "available" && state !== "checking",
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
        <div className="relative shrink-0" ref={addMenuRef}>
          <Button
            aria-expanded={addMenuOpen}
            aria-haspopup="menu"
            onClick={() => setAddMenuOpen((open) => !open)}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setAddMenuOpen(true);
              }
            }}
            type="button"
            variant="outline"
          >
            <Plus size={16} />
            {t("settings.agents.addCustom")}
            <ChevronDown className={clsx("transition-transform", addMenuOpen && "rotate-180")} size={15} />
          </Button>
          {addMenuOpen ? (
            <div
              aria-label={t("settings.agents.addCustom")}
              className="absolute right-0 top-12 z-10 w-64 rounded-xl border border-theme-card-border bg-theme-card p-1 shadow-[var(--theme-shadow-dialog)]"
              role="menu"
            >
              <button
                className="flex w-full items-start gap-3 rounded-lg px-3 py-2.5 text-left text-body-sm text-on-surface-variant outline-none transition-colors focus:bg-theme-control-hover focus:text-on-surface"
                onClick={() => {
                  setAddMenuOpen(false);
                  setCustomDialogOpen(true);
                }}
                ref={addMenuItemRef}
                role="menuitem"
                type="button"
              >
                <Code2 className="mt-0.5 shrink-0 text-primary" size={16} />
                <span>
                  <span className="block font-semibold text-on-surface">{t("settings.agents.customDefinition")}</span>
                  <span className="mt-0.5 block text-code-sm">{t("settings.agents.customDefinitionHint")}</span>
                </span>
              </button>
            </div>
          ) : null}
        </div>
      </div>

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
          <AgentFilterButton count={agentCatalog.length} filter="all" onChange={setFilter} selected={filter} t={t} />
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
                onEdit={() => setInfoAgent(agent)}
                onSelectModel={() => openModelDialog(agent)}
                onTest={() => void testConnection(agent)}
                selectedModel={selectedModels[agent.id]}
                t={t}
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

      {customDialogOpen ? (
        <DialogFrame
          closeLabel={t("common.close")}
          contentClassName="grid gap-4"
          description={t("settings.agents.customDialogDescription")}
          footer={
            <Button onClick={() => setCustomDialogOpen(false)} type="button" variant="outline">
              {t("common.close")}
            </Button>
          }
          icon={<Code2 size={18} />}
          onClose={() => setCustomDialogOpen(false)}
          size="lg"
          title={t("settings.agents.customDialogTitle")}
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <DefinitionValue label={t("settings.agents.field.agentId")} value="my-agent" />
            <DefinitionValue label={t("settings.agents.field.displayName")} value="My Agent" />
            <DefinitionValue label={t("settings.agents.command")} value="my-agent --acp" />
            <DefinitionValue label={t("settings.agents.protocol")} value="ACP" />
            <div className="sm:col-span-2">
              <DefinitionValue label={t("settings.agents.field.arguments")} value="--acp" />
            </div>
          </div>
          <p className="rounded-xl border border-status-update/25 bg-status-update/10 px-3 py-3 text-body-sm leading-6 text-on-surface-variant">
            {t("settings.agents.customDialogNotice")}
          </p>
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
  const message = result.connection_method === "cli_fallback"
    ? `${result.version || t("settings.agents.connectionAvailable")} · ${t("settings.agents.connectionCliFallback")}`
    : result.available
      ? result.version || t("settings.agents.connectionAvailable")
      : result.error || t("settings.agents.connectionFailed");
  setConnectionStates((current) => ({ ...current, [agentId]: state }));
  setConnectionMessages((current) => ({ ...current, [agentId]: message }));
}
