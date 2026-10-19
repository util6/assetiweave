import clsx from "clsx";
import {
  BrainCircuit,
  Info,
  LoaderCircle,
  Pencil,
  PlugZap,
} from "lucide-react";
import type { Translator } from "../../i18n/I18nProvider";
import { isActiveAgentLifecycleTask } from "../../app/backgroundTasks/AgentLifecycleTaskProvider";
import type { AppShortcut } from "../../types";
import {
  APP_SHORTCUT_ICON_FRAME_CLASS,
  appShortcutIconFrameStyle,
} from "../apps/AppShortcutIcon";
import { Badge } from "../foundation/Badge";
import { Button } from "../ui/button";
import {
  AgentCatalogIcon,
  agentIconFrameClass,
  resolveAgentIconAccentColor,
} from "./AgentCatalogIcon";
import type { AgentCatalogItem, AgentConnectionState } from "./agentCatalog";
import type {
  AgentLifecycleTaskSnapshot,
  AgentMarketErrorView,
} from "../../services/agentRuntime";

export function AgentConnectionRow({
  agent,
  appShortcuts = [],
  connectionMessage,
  connectionState,
  isTesting,
  isManaging,
  lifecycleTask,
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
  lifecycleTask?: AgentLifecycleTaskSnapshot | null;
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
  const lifecycleBlocked = agent.hasSelectableDistribution === false;
  const reinstallRequired =
    agent.installed?.installationStatus === "incompatible";
  const lifecycleBlockReason = lifecycleBlocked
    ? t("settings.agents.distributionUnavailable")
    : undefined;

  const isTaskActive = lifecycleTask
    ? isActiveAgentLifecycleTask(lifecycleTask)
    : false;
  const managingActive = Boolean(isManaging || isTaskActive);

  let progressPercent: number | null = null;
  let managingLabel = t("settings.agents.installing");
  let managingTooltip: string | undefined;

  if (isTaskActive && lifecycleTask) {
    const phase = lifecycleTask.phase;
    const downloaded = lifecycleTask.progress?.downloadedBytes;
    const total = lifecycleTask.progress?.totalBytes;

    if (phase === "downloading") {
      if (downloaded != null && total != null && total > 0) {
        progressPercent = Math.min(
          100,
          Math.max(0, Math.round((downloaded / total) * 100)),
        );
        managingLabel = t("settings.agents.lifecycle.downloading", {
          percent: progressPercent,
        });
        managingTooltip = t("settings.agents.lifecycle.downloadProgress", {
          downloaded: formatBytes(downloaded),
          total: formatBytes(total),
        });
      } else if (downloaded != null && downloaded > 0) {
        managingLabel = `${t("settings.agents.lifecycle.downloading", { percent: "" }).replace("%", "").trim()} (${formatBytes(downloaded)})`;
        managingTooltip = formatBytes(downloaded);
      } else {
        managingLabel = t("settings.agents.lifecycle.downloading", {
          percent: 0,
        });
      }
    } else if (phase === "installing") {
      managingLabel = t("settings.agents.lifecycle.installing");
    } else if (
      phase === "probing_protocol" ||
      phase === "validating_integrity" ||
      phase === "validating_layout"
    ) {
      managingLabel = t("settings.agents.lifecycle.verifying");
    } else if (phase === "cleaning_up") {
      managingLabel = t("settings.agents.lifecycle.cleaningUp");
    } else if (phase === "preparing" || phase === "queued") {
      managingLabel = t("settings.agents.lifecycle.preparing");
    }
  }

  const classifiedError = classifyLifecycleError(
    lifecycleTask,
    agent.installed?.error,
    connectionMessage,
    t,
  );

  return (
    <article className="group flex flex-wrap items-center gap-3 rounded-xl border border-theme-card-border/60 bg-theme-control/58 px-3.5 py-3 transition-[background,border-color,transform] hover:-translate-y-px hover:border-theme-nav-active-border/60 hover:bg-theme-control-hover/70 sm:flex-nowrap">
      <span
        className={clsx(
          APP_SHORTCUT_ICON_FRAME_CLASS,
          "size-11 shrink-0 bg-theme-card",
          agentIconFrameClass(),
        )}
        style={appShortcutIconFrameStyle(accentColor)}
      >
        <AgentCatalogIcon
          agent={agent}
          appShortcuts={appShortcuts}
          className="size-[22px]"
        />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-title-sm font-semibold text-on-surface">
            {agent.name}
          </h3>
          <Badge tone={status.tone}>{status.label}</Badge>
          {classifiedError ? (
            <Badge tone="remove" title={classifiedError.detail}>
              {classifiedError.badgeLabel}
            </Badge>
          ) : null}
          {lifecycleBlockReason ? (
            <Badge tone="remove">{lifecycleBlockReason}</Badge>
          ) : null}
          {connectionState === "not-installed" ? (
            <Info
              aria-label={t("settings.agents.statusNotInstalled")}
              className="text-outline"
              size={15}
            />
          ) : null}
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-code-sm text-outline">
          <span>{agent.command}</span>
          <span aria-hidden="true">·</span>
          <span>{agent.protocol}</span>
          {classifiedError ? (
            <span
              className="max-w-[28rem] truncate font-medium text-status-remove"
              title={classifiedError.detail}
            >
              · {classifiedError.detail}
            </span>
          ) : connectionMessage ? (
            <span className="max-w-[28rem] truncate" title={connectionMessage}>
              · {connectionMessage}
            </span>
          ) : null}
        </div>
      </div>
      <div className="flex w-full shrink-0 items-center justify-end gap-2 sm:w-auto">
        {view === "market" && managingActive ? (
          <Button
            aria-busy="true"
            className="relative h-10 min-w-36 overflow-hidden rounded-xl border border-theme-primary/30 bg-theme-primary/10 px-4 font-medium text-theme-primary shadow-sm select-none"
            disabled
            title={managingTooltip}
            type="button"
          >
            {progressPercent !== null ? (
              <span
                className="absolute inset-y-0 left-0 bg-theme-primary/25 transition-[width] duration-300 ease-out"
                style={{ width: `${progressPercent}%` }}
              />
            ) : (
              <span className="absolute inset-0 animate-pulse bg-gradient-to-r from-transparent via-theme-primary/15 to-transparent" />
            )}
            <span className="relative z-10 flex items-center gap-2">
              <LoaderCircle className="size-4 shrink-0 animate-spin text-theme-primary" />
              <span className="tracking-wide">{managingLabel}</span>
            </span>
          </Button>
        ) : null}
        {view === "market" && !managingActive ? (
          <>
            {onInstall ? (
              <Button
                disabled={
                  lifecycleBlocked ||
                  (reinstallRequired && !onReinstall)
                }
                onClick={reinstallRequired ? onReinstall : onInstall}
                title={lifecycleBlockReason}
                type="button"
              >
                {reinstallRequired
                  ? t("settings.agents.reinstall")
                  : agent.installed
                    ? agent.installed.enabled
                      ? t("settings.agents.disable")
                      : t("settings.agents.enable")
                    : t("settings.agents.install")}
              </Button>
            ) : null}
            {agent.installed && !reinstallRequired ? (
              agent.updateAvailable && onUpdate ? (
                <Button
                  disabled={lifecycleBlocked}
                  onClick={onUpdate}
                  title={lifecycleBlockReason}
                  type="button"
                  variant="default"
                  className="border-status-success/30 bg-status-success/20 text-status-success hover:bg-status-success/30"
                >
                  {t("settings.agents.update")}
                </Button>
              ) : onReinstall ? (
                <Button
                  disabled={lifecycleBlocked}
                  onClick={onReinstall}
                  title={lifecycleBlockReason}
                  type="button"
                  variant="outline"
                >
                  {t("settings.agents.reinstall")}
                </Button>
              ) : null
            ) : null}
            {onUninstall && agent.installed ? (
              <Button
                onClick={onUninstall}
                type="button"
                variant="outline"
              >
                {t("settings.agents.uninstall")}
              </Button>
            ) : null}
          </>
        ) : null}
        {view === "settings" && onTest ? (
          <Button
            disabled={isTesting}
            onClick={onTest}
            type="button"
            variant="outline"
          >
            {isTesting ? (
              <LoaderCircle className="animate-spin" size={15} />
            ) : (
              <PlugZap size={15} />
            )}
            <span>
              {isTesting
                ? t("settings.agents.testing")
                : t("settings.agents.testConnection")}
            </span>
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
            <span className="max-w-36 truncate">
              {selectedModel || t("settings.agents.model")}
            </span>
          </Button>
        ) : null}
        {view === "settings" && onEdit ? (
          <Button
            aria-label={`${t("settings.agents.edit")} ${agent.name}`}
            onClick={onEdit}
            type="button"
            variant="outline"
          >
            <Pencil size={15} />
            <span>{t("settings.agents.edit")}</span>
          </Button>
        ) : null}
      </div>
    </article>
  );
}

function statusMeta(state: AgentConnectionState, t: Translator) {
  if (state === "available")
    return {
      label: t("settings.agents.statusAvailable"),
      tone: "create" as const,
    };
  if (state === "failed")
    return {
      label: t("settings.agents.statusUnavailable"),
      tone: "remove" as const,
    };
  if (state === "not-installed")
    return {
      label: t("settings.agents.statusNotInstalled"),
      tone: "remove" as const,
    };
  if (state === "checking")
    return {
      label: t("settings.agents.statusChecking"),
      tone: "update" as const,
    };
  return {
    label: t("settings.agents.statusNotTested"),
    tone: "neutral" as const,
  };
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

interface ClassifiedError {
  type:
    | "upstream_not_found"
    | "network_error"
    | "install_failed"
    | "protocol_failed"
    | "cancelled"
    | "general";
  badgeLabel: string;
  detail: string;
}

function classifyLifecycleError(
  task: AgentLifecycleTaskSnapshot | null | undefined,
  installedError: AgentMarketErrorView | null | undefined,
  connectionMessage: string | undefined,
  t: Translator,
): ClassifiedError | null {
  const taskErr = task?.state === "failed" ? task.error : null;
  const err = taskErr || installedError;
  const code = err?.code || "";
  const rawMsg =
    err?.message ||
    (connectionMessage &&
    (connectionMessage.toLowerCase().includes("fail") ||
      connectionMessage.toLowerCase().includes("error") ||
      connectionMessage.includes("404"))
      ? connectionMessage
      : "");

  if (!code && !rawMsg && !taskErr && !installedError) {
    return null;
  }

  if (code === "upstream_version_not_found" || code === "not_found") {
    return {
      type: "upstream_not_found",
      badgeLabel: t("settings.agents.lifecycle.error.upstreamNotFound"),
      detail:
        rawMsg || t("settings.agents.lifecycle.error.upstreamNotFoundDesc"),
    };
  }

  if (code === "protocol_handshake_failed" || code === "probe_failed") {
    return {
      type: "protocol_failed",
      badgeLabel: t("settings.agents.lifecycle.error.protocolHandshakeFailed"),
      detail:
        rawMsg || t("settings.agents.lifecycle.error.protocolHandshakeFailed"),
    };
  }

  if (code === "install_failed") {
    return {
      type: "install_failed",
      badgeLabel: t("settings.agents.lifecycle.error.installFailed"),
      detail: rawMsg || t("settings.agents.lifecycle.error.installFailed"),
    };
  }

  if (code === "network_error" || code === "timeout") {
    return {
      type: "network_error",
      badgeLabel: t("settings.agents.lifecycle.error.networkError"),
      detail: rawMsg || t("settings.agents.lifecycle.error.networkErrorDesc"),
    };
  }

  if (code === "cancelled") {
    return {
      type: "cancelled",
      badgeLabel: t("settings.agents.lifecycle.error.cancelled"),
      detail: rawMsg || t("settings.agents.lifecycle.error.cancelled"),
    };
  }

  if (
    rawMsg.includes("404") ||
    rawMsg.includes("upstream_version_not_found")
  ) {
    return {
      type: "upstream_not_found",
      badgeLabel: t("settings.agents.lifecycle.error.upstreamNotFound"),
      detail:
        rawMsg || t("settings.agents.lifecycle.error.upstreamNotFoundDesc"),
    };
  }

  if (rawMsg.includes("ACP") || rawMsg.includes("handshake")) {
    return {
      type: "protocol_failed",
      badgeLabel: t("settings.agents.lifecycle.error.protocolHandshakeFailed"),
      detail:
        rawMsg || t("settings.agents.lifecycle.error.protocolHandshakeFailed"),
    };
  }

  if (
    rawMsg.includes("install_failed") ||
    rawMsg.includes("exit code") ||
    rawMsg.includes("ENOENT")
  ) {
    return {
      type: "install_failed",
      badgeLabel: t("settings.agents.lifecycle.error.installFailed"),
      detail: rawMsg || t("settings.agents.lifecycle.error.installFailed"),
    };
  }

  if (
    rawMsg.includes("network_error") ||
    rawMsg.toLowerCase().includes("connection refused") ||
    rawMsg.toLowerCase().includes("dns") ||
    rawMsg.toLowerCase().includes("timeout") ||
    rawMsg.toLowerCase().includes("timed out")
  ) {
    return {
      type: "network_error",
      badgeLabel: t("settings.agents.lifecycle.error.networkError"),
      detail: rawMsg || t("settings.agents.lifecycle.error.networkErrorDesc"),
    };
  }

  if (rawMsg.includes("cancelled")) {
    return {
      type: "cancelled",
      badgeLabel: t("settings.agents.lifecycle.error.cancelled"),
      detail: rawMsg || t("settings.agents.lifecycle.error.cancelled"),
    };
  }

  if (rawMsg) {
    return {
      type: "general",
      badgeLabel: t("settings.agents.statusUnavailable"),
      detail: rawMsg,
    };
  }

  return null;
}

