import { listen } from "@tauri-apps/api/event";
import clsx from "clsx";
import {
  Check,
  CircleAlert,
  Download,
  ExternalLink,
  Loader2,
  PackageCheck,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  getConversationScriptInstallTask,
  installConversationScript,
  listConversationScriptCatalog,
  type ConversationScriptCatalogEntry,
  type ConversationScriptInstallTaskSnapshot,
} from "../../services/conversations";
import type { ConversationRecordKind } from "../../types";
import type { NotificationMessage } from "../notifications/NotificationBanner";
import { Badge } from "../foundation/Badge";
import { Button } from "../ui/button";

const SCRIPT_INSTALL_TASK_UPDATED_EVENT = "conversation-script-install-task-updated";
const SCRIPT_INSTALL_POLL_INTERVAL_MS = 1000;

type ScriptResourceNotification = Omit<NotificationMessage, "id">;

export function ConversationScriptResourcePanel({
  disabled = false,
  onInstalled,
  onManifestSelect,
  onNotify,
  onNotifyError,
  recordKind,
}: {
  disabled?: boolean;
  onInstalled: () => Promise<void> | void;
  onManifestSelect: (manifestPath: string) => void;
  onNotify: (notification: ScriptResourceNotification) => void;
  onNotifyError: (message: string) => void;
  recordKind: ConversationRecordKind;
}) {
  const { t } = useI18n();
  const [entries, setEntries] = useState<ConversationScriptCatalogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installTask, setInstallTask] = useState<ConversationScriptInstallTaskSnapshot | null>(null);
  const handledInstallTaskIds = useRef(new Set<string>());

  const loadCatalog = useCallback(
    async (mode: "initial" | "refresh" = "refresh") => {
      if (mode === "initial") {
        setLoading(true);
      } else {
        setRefreshing(true);
      }
      try {
        const nextEntries = await listConversationScriptCatalog();
        setEntries(nextEntries);
        setError(null);
      } catch (loadError) {
        const message = errorMessage(loadError);
        setError(message);
        onNotifyError(message);
      } finally {
        setLoading(false);
        setRefreshing(false);
      }
    },
    [onNotifyError],
  );

  useEffect(() => {
    void loadCatalog("initial");
  }, [loadCatalog]);

  useEffect(() => {
    let cancelled = false;
    void getConversationScriptInstallTask()
      .then((snapshot) => {
        if (cancelled) {
          return;
        }
        if (snapshot && snapshot.status !== "running") {
          handledInstallTaskIds.current.add(snapshot.id);
        }
        setInstallTask(snapshot);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<ConversationScriptInstallTaskSnapshot>(
      SCRIPT_INSTALL_TASK_UPDATED_EVENT,
      (event) => {
        if (!cancelled) {
          setInstallTask(event.payload);
        }
      },
    )
      .then((removeListener) => {
        if (cancelled) {
          removeListener();
        } else {
          unlisten = removeListener;
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (installTask?.status !== "running") {
      return;
    }

    let polling = false;
    const intervalId = window.setInterval(() => {
      if (polling) {
        return;
      }
      polling = true;
      void getConversationScriptInstallTask()
        .then((snapshot) => {
          if (snapshot) {
            setInstallTask(snapshot);
          }
        })
        .catch(() => {})
        .finally(() => {
          polling = false;
        });
    }, SCRIPT_INSTALL_POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [installTask?.id, installTask?.status]);

  useEffect(() => {
    if (!installTask || installTask.status === "running" || handledInstallTaskIds.current.has(installTask.id)) {
      return;
    }
    handledInstallTaskIds.current.add(installTask.id);
    if (installTask.status === "completed") {
      const manifestPath = manifestPathFromInstallResult(installTask.result);
      if (manifestPath) {
        onManifestSelect(manifestPath);
      }
      onNotify({
        message: t("conversation.scriptMarket.installCompleted"),
        tone: "success",
      });
      void Promise.resolve(onInstalled()).finally(() => {
        void loadCatalog("refresh");
      });
      return;
    }

    onNotifyError(installTask.error || t("conversation.scriptMarket.installFailed"));
  }, [installTask, loadCatalog, onInstalled, onManifestSelect, onNotify, onNotifyError, t]);

  const visibleEntries = useMemo(
    () => entries.filter((entry) => entry.item.record_kind === recordKind),
    [entries, recordKind],
  );
  const installRunning = installTask?.status === "running";

  async function handleInstall(entry: ConversationScriptCatalogEntry) {
    try {
      const task = await installConversationScript({ itemId: entry.item.id });
      setInstallTask(task);
      if (task.status === "running") {
        onNotify({
          message: t("conversation.scriptMarket.installStarted"),
          tone: "info",
        });
      }
    } catch (installError) {
      onNotifyError(errorMessage(installError));
    }
  }

  function handleUse(entry: ConversationScriptCatalogEntry) {
    const manifestPath = manifestPathForEntry(entry);
    if (!manifestPath) {
      return;
    }
    onManifestSelect(manifestPath);
    onNotify({
      message: t("conversation.scriptMarket.manifestSelected"),
      tone: "info",
    });
  }

  return (
    <section className="rounded-lg border border-theme-card-border bg-theme-card/55 p-3">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <h3 className="flex min-w-0 items-center gap-2 text-body-sm font-semibold text-on-surface">
            <PackageCheck className="shrink-0 text-primary" size={16} />
            <span>{t("conversation.scriptMarket.inlineTitle")}</span>
          </h3>
          <p className="mt-1 text-body-sm text-on-surface-variant">
            {t("conversation.scriptMarket.inlineDescription")}
          </p>
        </div>
        <button
          className="inline-flex h-9 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
          disabled={disabled || refreshing || installRunning}
          onClick={() => void loadCatalog("refresh")}
          type="button"
        >
          <RefreshCw className={clsx(refreshing && "animate-spin")} size={15} />
          <span>{t("common.refresh")}</span>
        </button>
      </div>

      {installRunning ? (
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-status-update/35 bg-status-update/10 px-3 py-2 text-body-sm text-status-update">
          <Loader2 className="shrink-0 animate-spin" size={15} />
          <span className="truncate">
            {t("conversation.scriptMarket.installing")} - {installTask.item_id}
          </span>
        </div>
      ) : null}

      {error ? (
        <div className="mt-3 flex items-start gap-2 rounded-lg border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove">
          <CircleAlert className="mt-0.5 shrink-0" size={15} />
          <span>{error}</span>
        </div>
      ) : loading ? (
        <div className="mt-3 grid gap-2" aria-busy="true">
          {Array.from({ length: 2 }).map((_, index) => (
            <div className="rounded-lg border border-theme-card-border bg-theme-control/40 p-3" key={index}>
              <div className="h-4 w-48 max-w-full animate-pulse rounded bg-theme-control" />
              <div className="mt-2 h-3 w-full max-w-lg animate-pulse rounded bg-theme-control" />
            </div>
          ))}
        </div>
      ) : visibleEntries.length === 0 ? (
        <div className="mt-3 rounded-lg border border-theme-card-border bg-theme-control/40 px-3 py-2 text-body-sm text-on-surface-variant">
          {t("conversation.scriptMarket.emptyForKind")}
        </div>
      ) : (
        <div className="mt-3 grid max-h-72 gap-2 overflow-y-auto pr-1">
          {visibleEntries.map((entry) => (
            <ScriptResourceRow
              disabled={disabled || installRunning}
              entry={entry}
              key={entry.item.id}
              onInstall={() => void handleInstall(entry)}
              onUse={() => handleUse(entry)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function ScriptResourceRow({
  disabled,
  entry,
  onInstall,
  onUse,
}: {
  disabled: boolean;
  entry: ConversationScriptCatalogEntry;
  onInstall: () => void;
  onUse: () => void;
}) {
  const { t } = useI18n();
  const manifestPath = manifestPathForEntry(entry);

  return (
    <article className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-theme-card-border bg-theme-card px-3 py-2 max-[640px]:grid-cols-1">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h4 className="min-w-0 truncate text-body-sm font-semibold text-on-surface">{entry.item.name}</h4>
          <Badge tone={entry.installed ? "create" : "primary"}>
            {entry.installed
              ? t("conversation.scriptMarket.installed")
              : t("conversation.scriptMarket.install")}
          </Badge>
          {entry.update_available ? (
            <Badge tone="conflict">{t("conversation.scriptMarket.updateAvailable")}</Badge>
          ) : null}
        </div>
        {entry.item.description ? (
          <p className="mt-1 line-clamp-2 text-body-sm text-on-surface-variant">{entry.item.description}</p>
        ) : null}
        <div className="mt-2 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-body-xs text-on-surface-variant">
          <span>{entry.item.provider ?? entry.item.id}</span>
          <span>{entry.item.version}</span>
          {manifestPath ? <span className="max-w-sm truncate">{manifestPath}</span> : null}
          {entry.item.repository_url ? (
            <a
              className="inline-flex items-center gap-1 text-primary hover:underline"
              href={entry.item.repository_url}
              rel="noreferrer"
              target="_blank"
            >
              <ExternalLink size={12} />
              {t("conversation.scriptMarket.repository")}
            </a>
          ) : null}
        </div>
      </div>
      <div className="flex items-center justify-end gap-2">
        {entry.installed && manifestPath ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={disabled}
            onClick={onUse}
            type="button"
          >
            <Check size={15} />
            {t("conversation.scriptMarket.useInstalled")}
          </Button>
        ) : null}
        {!entry.installed || entry.update_available ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={disabled}
            onClick={onInstall}
            type="button"
          >
            {entry.update_available ? <ShieldCheck size={15} /> : <Download size={15} />}
            {entry.update_available
              ? t("conversation.scriptMarket.update")
              : t("conversation.scriptMarket.install")}
          </Button>
        ) : null}
      </div>
    </article>
  );
}

function manifestPathForEntry(entry: ConversationScriptCatalogEntry) {
  if (entry.installed_adapter?.manifest_path) {
    return entry.installed_adapter.manifest_path;
  }
  if (!entry.install_path) {
    return null;
  }
  const manifestFile = entry.item.manifest_file?.trim() || "conversation-adapter.json";
  return `${entry.install_path.replace(/\/$/, "")}/${manifestFile}`;
}

function manifestPathFromInstallResult(result: unknown) {
  if (!result || typeof result !== "object" || !("manifest_path" in result)) {
    return null;
  }
  const value = (result as { manifest_path?: unknown }).manifest_path;
  return typeof value === "string" && value.trim() ? value : null;
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
