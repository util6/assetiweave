import { listen } from "@tauri-apps/api/event";
import clsx from "clsx";
import {
  Check,
  CircleAlert,
  Download,
  ExternalLink,
  History,
  Info,
  Loader2,
  PackageCheck,
  PowerOff,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Wrench,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  checkConversationAdapterPackageUpdates,
  getConversationAdapterPackageTask,
  inspectConversationAdapterPackage,
  installConversationAdapterPackage,
  deleteConversationAdapterPackageVersion,
  listInstalledConversationAdapterPackageVersions,
  listConversationAdapterPackageReleases,
  listConversationAdapterPackages,
  prepareConversationAdapterPackageChange,
  rollbackConversationAdapterPackageVersion,
  setConversationAdapterPackageUpdatePolicy,
  switchConversationAdapterPackageVersion,
  unregisterConversationAdapter,
  uninstallConversationAdapterPackage,
  updateConversationAdapterPackage,
  type ConversationAdapterPackageChangePreflight,
  type ConversationAdapterPackageCatalogEntry,
  type ConversationAdapterPackageCatalogStatus,
  type ConversationAdapterCatalogRelease,
  type ConversationAdapterPackageInspection,
  type ConversationAdapterPackageVersion,
  type ConversationScriptInstallTaskSnapshot,
} from "../../services/conversations";
import type { ConversationPackageUpdatePolicy, ConversationRecordKind } from "../../types";
import type { NotificationMessage } from "../notifications/NotificationBanner";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
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
  const [entries, setEntries] = useState<ConversationAdapterPackageCatalogEntry[]>([]);
  const [activeView, setActiveView] = useState<"connected" | "updates" | "discover">("connected");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installTask, setInstallTask] = useState<ConversationScriptInstallTaskSnapshot | null>(null);
  const [pendingChange, setPendingChange] = useState<{
    action: PackageChangeAction;
    entry: ConversationAdapterPackageCatalogEntry;
    preflight: ConversationAdapterPackageChangePreflight;
  } | null>(null);
  const [confirmingChange, setConfirmingChange] = useState(false);
  const [detailEntry, setDetailEntry] = useState<ConversationAdapterPackageCatalogEntry | null>(null);
  const [detailInspection, setDetailInspection] = useState<ConversationAdapterPackageInspection | null>(null);
  const [detailReleases, setDetailReleases] = useState<ConversationAdapterCatalogRelease[]>([]);
  const [installedVersions, setInstalledVersions] = useState<ConversationAdapterPackageVersion[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [selectedVersion, setSelectedVersion] = useState<string>("");
  const [versionMutation, setVersionMutation] = useState<string | null>(null);
  const [policySaving, setPolicySaving] = useState(false);
  const handledInstallTaskIds = useRef(new Set<string>());

  const loadCatalog = useCallback(
    async (mode: "initial" | "refresh" = "refresh") => {
      if (mode === "initial") {
        setLoading(true);
      } else {
        setRefreshing(true);
      }
      try {
        const nextEntries = await listConversationAdapterPackages();
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
    void getConversationAdapterPackageTask()
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
      void getConversationAdapterPackageTask()
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
        message: t(packageTaskCompletedLabel(installTask.action)),
        tone: "success",
      });
      void Promise.resolve(onInstalled()).finally(() => {
        void loadCatalog("refresh");
      });
      return;
    }

    onNotifyError(installTask.error || t(packageTaskFailedLabel(installTask.action)));
  }, [installTask, loadCatalog, onInstalled, onManifestSelect, onNotify, onNotifyError, t]);

  const recordEntries = useMemo(
    () => entries.filter((entry) => entry.item.record_kind === recordKind),
    [entries, recordKind],
  );
  const viewCounts = useMemo(
    () => ({
      connected: recordEntries.filter((entry) => entry.installed).length,
      updates: recordEntries.filter(isUpdateOrRepairEntry).length,
      discover: recordEntries.filter((entry) => !entry.installed).length,
    }),
    [recordEntries],
  );
  const visibleEntries = useMemo(() => {
    switch (activeView) {
      case "updates":
        return recordEntries.filter(isUpdateOrRepairEntry);
      case "discover":
        return recordEntries.filter((entry) => !entry.installed);
      case "connected":
      default:
        return recordEntries.filter((entry) => entry.installed);
    }
  }, [activeView, recordEntries]);
  const installRunning = installTask?.status === "running";

  async function handleInstall(entry: ConversationAdapterPackageCatalogEntry) {
    const action = packageActionForEntry(entry);
    if (!action) {
      return;
    }
    await beginPackageChange(entry, action);
  }

  async function handleCheckUpdates() {
    setCheckingUpdates(true);
    try {
      const statuses = await checkConversationAdapterPackageUpdates({ force: true });
      await loadCatalog("refresh");
      setActiveView("updates");
      onNotify({
        message: t("conversation.scriptMarket.checkUpdatesCompleted", {
          count: statuses.filter((status) => status.update_available).length,
        }),
        tone: "success",
      });
    } catch (checkError) {
      onNotifyError(errorMessage(checkError));
    } finally {
      setCheckingUpdates(false);
    }
  }

  async function beginPackageChange(
    entry: ConversationAdapterPackageCatalogEntry,
    action: PackageChangeAction,
  ) {
    try {
      const preflight = await prepareConversationAdapterPackageChange({
        action:
          action === "repair"
            ? "update"
            : action,
        packageId: entry.item.id,
        adapterId: entry.installed_adapter?.id ?? entry.item.adapter_id ?? null,
      });
      if (preflight.task_conflicts.length > 0) {
        throw new Error(
          t("conversation.scriptMarket.taskConflict", {
            tasks: preflight.task_conflicts.join(", "),
          }),
        );
      }
      setPendingChange({ action, entry, preflight });
    } catch (installError) {
      onNotifyError(errorMessage(installError));
    }
  }

  async function confirmPackageChange() {
    if (!pendingChange) {
      return;
    }
    setConfirmingChange(true);
    try {
      const { action, entry } = pendingChange;
      if (action === "unregister") {
        const adapterId = entry.installed_adapter?.id ?? entry.item.adapter_id;
        if (!adapterId) {
          throw new Error("conversation adapter id is missing");
        }
        await unregisterConversationAdapter({ adapterId, confirmed: true });
        setPendingChange(null);
        setDetailEntry(null);
        onNotify({
          message: t("conversation.scriptMarket.uninstallCompleted"),
          tone: "success",
        });
        await Promise.resolve(onInstalled());
        await loadCatalog("refresh");
        return;
      }
      if (action === "uninstall") {
        const task = await uninstallConversationAdapterPackage({
          packageId: entry.installed_package?.package_id ?? entry.item.id,
          confirmed: true,
        });
        setPendingChange(null);
        setDetailEntry(null);
        setInstallTask(task);
        return;
      }
      const version =
        detailEntry?.item.id === entry.item.id && selectedVersion
          ? selectedVersion
          : undefined;
      const task = action === "update" || action === "repair"
        ? await updateConversationAdapterPackage({
            packageId: entry.item.id,
            version,
            confirmed: true,
          })
        : await installConversationAdapterPackage({
            packageId: entry.item.id,
            version,
            confirmed: true,
          });
      setPendingChange(null);
      setInstallTask(task);
      if (task.status === "running") {
        onNotify({
          message: t("conversation.scriptMarket.installStarted"),
          tone: "info",
        });
      }
    } catch (installError) {
      onNotifyError(errorMessage(installError));
    } finally {
      setConfirmingChange(false);
    }
  }

  async function openPackageDetail(entry: ConversationAdapterPackageCatalogEntry) {
    setDetailEntry(entry);
    setDetailInspection(null);
    setDetailReleases([]);
    setInstalledVersions([]);
    setDetailError(null);
    setDetailLoading(true);
    try {
      const packageId = entry.installed_package?.package_id ?? entry.item.id;
      const [inspection, releases, versions] = await Promise.all([
        entry.installed
          ? inspectConversationAdapterPackage({
              packageId: entry.installed_package?.package_id ?? null,
              adapterId: entry.installed_adapter?.id ?? entry.item.adapter_id ?? null,
            })
          : Promise.resolve(null),
        listConversationAdapterPackageReleases({
          packageId: entry.item.id,
          refresh: false,
        }).catch(() => []),
        entry.installed_package?.origin === "managed_release"
          ? listInstalledConversationAdapterPackageVersions(packageId).catch(() => [])
          : Promise.resolve([]),
      ]);
      setDetailInspection(inspection);
      setDetailReleases(releases);
      setInstalledVersions(versions);
      setSelectedVersion(releases[0]?.version ?? entry.item.version);
    } catch (detailLoadError) {
      setDetailError(errorMessage(detailLoadError));
    } finally {
      setDetailLoading(false);
    }
  }

  async function mutateInstalledVersion(action: "switch" | "rollback" | "delete", version?: string) {
    if (!detailEntry?.installed_package) {
      return;
    }
    const packageId = detailEntry.installed_package.package_id;
    const mutationKey = `${action}:${version ?? "previous"}`;
    if (action === "delete" && !window.confirm(t("conversation.scriptMarket.deleteVersionConfirm", { version: version ?? "" }))) {
      return;
    }
    setVersionMutation(mutationKey);
    try {
      if (action === "switch" && version) {
        await switchConversationAdapterPackageVersion({ packageId, version, confirmed: true });
      } else if (action === "rollback") {
        await rollbackConversationAdapterPackageVersion({ packageId, confirmed: true });
      } else if (action === "delete" && version) {
        await deleteConversationAdapterPackageVersion({ packageId, version, confirmed: true });
      }
      const [nextVersions] = await Promise.all([
        listInstalledConversationAdapterPackageVersions(packageId),
        loadCatalog("refresh"),
      ]);
      setInstalledVersions(nextVersions);
      if (action !== "delete" || nextVersions.length === 0) {
        setDetailEntry(null);
      }
      onNotify({ message: t("conversation.scriptMarket.versionActionCompleted"), tone: "success" });
    } catch (mutationError) {
      onNotifyError(errorMessage(mutationError));
    } finally {
      setVersionMutation(null);
    }
  }

  async function changeUpdatePolicy(updatePolicy: ConversationPackageUpdatePolicy) {
    if (!detailEntry?.installed_package) {
      return;
    }
    setPolicySaving(true);
    try {
      const updated = await setConversationAdapterPackageUpdatePolicy({
        packageId: detailEntry.installed_package.package_id,
        updatePolicy,
      });
      setDetailEntry((current) => current ? { ...current, installed_package: updated } : current);
      onNotify({ message: t("conversation.scriptMarket.policySaved"), tone: "success" });
    } catch (policyError) {
      onNotifyError(errorMessage(policyError));
    } finally {
      setPolicySaving(false);
    }
  }

  function handleUse(entry: ConversationAdapterPackageCatalogEntry) {
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
        <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
          <button
            className="inline-flex h-9 items-center justify-center gap-2 whitespace-nowrap rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
            disabled={disabled || loading || refreshing || checkingUpdates}
            onClick={() => void loadCatalog("refresh")}
            type="button"
          >
            <RefreshCw className={clsx(refreshing && "animate-spin")} size={15} />
            <span>{t("common.refresh")}</span>
          </button>
          <Button
            disabled={disabled || loading || refreshing || checkingUpdates || installRunning}
            onClick={() => void handleCheckUpdates()}
            type="button"
            variant="outline"
          >
            <ShieldCheck className={clsx(checkingUpdates && "animate-pulse")} size={15} />
            {checkingUpdates
              ? t("conversation.scriptMarket.checkingUpdates")
              : t("conversation.scriptMarket.checkUpdates")}
          </Button>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-1 border-b border-theme-card-border" role="tablist">
        {(["connected", "updates", "discover"] as const).map((view) => (
          <button
            aria-selected={activeView === view}
            className={clsx(
              "border-b-2 px-3 py-2 text-body-sm transition-colors",
              activeView === view
                ? "border-primary text-on-surface"
                : "border-transparent text-on-surface-variant hover:text-on-surface",
            )}
            key={view}
            onClick={() => setActiveView(view)}
            role="tab"
            type="button"
          >
            {t(`conversation.scriptMarket.view.${view}`)} ({viewCounts[view]})
          </button>
        ))}
      </div>

      {installRunning ? (
        <div className="mt-3 flex items-center gap-2 rounded-lg border border-status-update/35 bg-status-update/10 px-3 py-2 text-body-sm text-status-update">
          <Loader2 className="shrink-0 animate-spin" size={15} />
          <span className="truncate">
            {t(packageTaskRunningLabel(installTask.action))} - {installTask.package_id ?? installTask.item_id}
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
          <div
            aria-live="polite"
            className="flex items-center gap-2 rounded-lg border border-status-update/30 bg-status-update/10 px-3 py-2 text-body-sm text-status-update"
            role="status"
          >
            <Loader2 className="shrink-0 animate-spin" size={15} />
            <span>{t("conversation.scriptMarket.loading")}</span>
          </div>
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
              changeDisabled={disabled || installRunning}
              disabled={disabled}
              entry={entry}
              key={entry.item.id}
              onInstall={() => void handleInstall(entry)}
              onInspect={() => void openPackageDetail(entry)}
              onLifecycle={() => {
                const action = packageLifecycleAction(entry);
                if (action) {
                  void beginPackageChange(entry, action);
                }
              }}
              onUse={() => handleUse(entry)}
            />
          ))}
        </div>
      )}
      {detailEntry ? (
        <DialogFrame
          closeLabel={t("common.close")}
          contentClassName="grid max-h-[70vh] gap-4 overflow-y-auto"
          footer={
            <>
              <Button onClick={() => setDetailEntry(null)} type="button" variant="outline">
                {t("common.close")}
              </Button>
              {packageLifecycleAction(detailEntry) ? (
                <Button
                  onClick={() => {
                    const action = packageLifecycleAction(detailEntry);
                    if (action) {
                      void beginPackageChange(detailEntry, action);
                    }
                  }}
                  type="button"
                  variant="outline"
                >
                  <PowerOff size={15} />
                  {t(actionLabelKey(packageLifecycleAction(detailEntry)!))}
                </Button>
              ) : null}
              {packageActionForEntry(detailEntry) ? (
                <Button onClick={() => void handleInstall(detailEntry)} type="button">
                  <Download size={15} />
                  {t(actionLabelKey(packageActionForEntry(detailEntry)!))}
                </Button>
              ) : null}
            </>
          }
          icon={<Info size={18} />}
          onClose={() => setDetailEntry(null)}
          size="xl"
          title={detailEntry.item.name}
        >
          {detailLoading ? (
            <div aria-busy="true" className="grid gap-2">
              <div className="h-5 w-52 animate-pulse rounded bg-theme-control" />
              <div className="h-32 animate-pulse rounded bg-theme-control" />
            </div>
          ) : detailError ? (
            <div className="rounded-lg border border-status-remove/35 bg-status-remove/10 p-3 text-body-sm text-status-remove">
              {detailError}
            </div>
          ) : (
            <>
              <div className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-card/65 p-3 sm:grid-cols-2">
                <DetailField label={t("conversation.scriptMarket.packageId")} value={detailEntry.installed_package?.package_id ?? detailEntry.item.id} />
                <DetailField label={t("conversation.scriptMarket.adapterId")} value={detailEntry.installed_adapter?.id ?? detailEntry.item.adapter_id ?? "-"} />
                <DetailField label={t("conversation.scriptMarket.origin")} value={detailInspection?.origin ?? detailEntry.installed_package?.origin ?? detailEntry.item.provider ?? "-"} />
                <DetailField label={t("conversation.scriptMarket.publisher")} value={detailReleases[0]?.publisher ?? detailEntry.item.provider ?? "-"} />
                <DetailField label={t("conversation.scriptMarket.currentVersion")} value={detailEntry.installed_package?.version ?? detailEntry.installed_adapter?.version ?? t("conversation.scriptMarket.notInstalled")} />
                <DetailField label={t("conversation.scriptMarket.latestVersion")} value={detailReleases[0]?.version ?? detailEntry.item.version} />
                <DetailField label={t("conversation.scriptMarket.runtimeGate")} value={detailEntry.installed_package?.runtime_gate_status ?? detailEntry.status} />
                <DetailField label={t("conversation.scriptMarket.recordKind")} value={detailEntry.item.record_kind} />
                <DetailField label={t("conversation.scriptMarket.path")} value={detailEntry.display_install_path ?? detailEntry.installed_package?.install_dir ?? detailEntry.install_path ?? "-"} wide />
                <DetailField label={t("conversation.scriptMarket.manifest")} value={manifestDisplayPathForEntry(detailEntry) ?? "-"} wide />
                <DetailField label={t("conversation.scriptMarket.contentHash")} value={detailEntry.installed_package?.installed_content_hash ?? detailEntry.installed_adapter?.content_hash ?? "-"} wide />
                <DetailField label={t("conversation.scriptMarket.trustedHash")} value={detailEntry.installed_package?.trusted_package_hash ?? detailEntry.installed_adapter?.trusted_hash ?? "-"} wide />
              </div>

              {detailEntry.installed_package?.origin === "dev_override" ? (
                <div className="rounded-lg border border-status-update/35 bg-status-update/10 p-3 text-body-sm text-status-update">
                  {t("conversation.scriptMarket.devOverrideNotice")}
                </div>
              ) : null}

              {detailInspection?.affected_sources.length ? (
                <section>
                  <h4 className="text-body-sm font-semibold text-on-surface">
                    {t("conversation.scriptMarket.affectedSources")}
                  </h4>
                  <ul className="mt-2 grid gap-1 text-body-sm text-on-surface-variant">
                    {detailInspection.affected_sources.map((source) => (
                      <li className="rounded-md border border-theme-card-border px-2 py-1.5" key={source.id}>
                        {source.name} · {source.id}
                      </li>
                    ))}
                  </ul>
                </section>
              ) : null}

              {detailEntry.installed_package?.origin === "managed_release" ? (
                <section>
                  <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-theme-card-border bg-theme-control/40 p-3">
                    <label className="text-body-sm font-medium text-on-surface" htmlFor="conversation-package-update-policy">
                      {t("conversation.scriptMarket.updatePolicy")}
                    </label>
                    <select
                      className="h-9 rounded-md border border-theme-control-border bg-theme-control px-2 text-body-sm text-theme-control-fg"
                      disabled={policySaving}
                      id="conversation-package-update-policy"
                      onChange={(event) => void changeUpdatePolicy(event.target.value as ConversationPackageUpdatePolicy)}
                      value={detailEntry.installed_package.update_policy}
                    >
                      {(["manual", "follow_stable", "follow_beta", "pin_exact"] as const).map((policy) => (
                        <option key={policy} value={policy}>{t(`conversation.scriptMarket.policy.${policy}`)}</option>
                      ))}
                    </select>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <h4 className="text-body-sm font-semibold text-on-surface">
                      {t("conversation.scriptMarket.installedVersions")}
                    </h4>
                    {detailEntry.runtime_ready && installedVersions.some((version) => version.version !== detailEntry.installed_package?.version) ? (
                      <Button
                        disabled={Boolean(versionMutation)}
                        onClick={() => void mutateInstalledVersion("rollback")}
                        type="button"
                        variant="outline"
                      >
                        <RotateCcw size={15} />
                        {t("conversation.scriptMarket.rollback")}
                      </Button>
                    ) : null}
                  </div>
                  <div className="mt-2 grid gap-2">
                    {installedVersions.map((version) => {
                      const active = detailEntry.runtime_ready
                        && version.version === detailEntry.installed_package?.version;
                      return (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-theme-card-border bg-theme-card/55 p-3" key={version.version}>
                          <div className="flex items-center gap-2">
                            <span className="font-mono text-body-sm font-semibold text-on-surface">{version.version}</span>
                            {active ? <Badge tone="primary">{t("conversation.scriptMarket.activeVersion")}</Badge> : null}
                            <span className="text-body-xs text-on-surface-variant">{version.runtime_gate_status}</span>
                          </div>
                          {!active ? (
                            <div className="flex gap-2">
                              <Button disabled={Boolean(versionMutation)} onClick={() => void mutateInstalledVersion("switch", version.version)} type="button" variant="outline">
                                {versionMutation === `switch:${version.version}` ? <Loader2 className="animate-spin" size={15} /> : null}
                                {t(detailEntry.status === "uninstalled"
                                  ? "conversation.scriptMarket.registerVersion"
                                  : "conversation.scriptMarket.switchVersion")}
                              </Button>
                              <Button disabled={Boolean(versionMutation)} onClick={() => void mutateInstalledVersion("delete", version.version)} type="button" variant="destructive">
                                <Trash2 size={15} />
                                {t("conversation.scriptMarket.deleteVersion")}
                              </Button>
                            </div>
                          ) : null}
                        </div>
                      );
                    })}
                  </div>
                </section>
              ) : null}

              <section>
                <div className="flex items-center justify-between gap-2">
                  <h4 className="flex items-center gap-2 text-body-sm font-semibold text-on-surface">
                    <History size={15} />
                    {t("conversation.scriptMarket.versionHistory")}
                  </h4>
                  {detailReleases.length > 0 ? (
                    <select
                      aria-label={t("conversation.scriptMarket.selectVersion")}
                      className="h-9 rounded-md border border-theme-control-border bg-theme-control px-2 text-body-sm text-theme-control-fg"
                      onChange={(event) => setSelectedVersion(event.target.value)}
                      value={selectedVersion}
                    >
                      {detailReleases.map((release) => (
                        <option key={release.version} value={release.version}>
                          {release.version} · {release.channel}
                        </option>
                      ))}
                    </select>
                  ) : null}
                </div>
                {detailReleases.length === 0 ? (
                  <p className="mt-2 text-body-sm text-on-surface-variant">
                    {t("conversation.scriptMarket.noVersionHistory")}
                  </p>
                ) : (
                  <div className="mt-2 grid gap-2">
                    {detailReleases.map((release) => (
                      <article className="rounded-lg border border-theme-card-border bg-theme-card/55 p-3" key={release.version}>
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-mono text-body-sm font-semibold text-on-surface">{release.version}</span>
                          <Badge tone={release.breaking_change ? "remove" : "primary"}>{release.channel}</Badge>
                          <span className="text-body-xs text-on-surface-variant">Core {release.core_compatibility}</span>
                        </div>
                        <pre className="mt-2 whitespace-pre-wrap font-sans text-body-sm leading-6 text-on-surface-variant">{release.changelog_markdown}</pre>
                      </article>
                    ))}
                  </div>
                )}
              </section>
            </>
          )}
        </DialogFrame>
      ) : null}

      <ConfirmDialog
        busy={confirmingChange}
        confirmLabel={pendingChange ? t(actionLabelKey(pendingChange.action)) : undefined}
        message={
          pendingChange
            ? t("conversation.scriptMarket.confirmMessage", {
                action: t(actionLabelKey(pendingChange.action)),
                name: pendingChange.entry.item.name,
              })
            : ""
        }
        onClose={() => setPendingChange(null)}
        onConfirm={() => void confirmPackageChange()}
        open={Boolean(pendingChange)}
        title={t("conversation.scriptMarket.confirmTitle")}
      >
        {pendingChange ? (
          <div className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-control/45 p-3 text-body-xs text-on-surface-variant">
            <p>{t("conversation.scriptMarket.recordsPreserved")}</p>
            {pendingChange.preflight.affected_sources.length > 0 ? (
              <div>
                <p className="font-medium text-on-surface">
                  {t("conversation.scriptMarket.affectedSources")}
                </p>
                <ul className="mt-1 list-disc space-y-1 pl-4">
                  {pendingChange.preflight.affected_sources.map((source) => (
                    <li key={source.id}>{source.name}</li>
                  ))}
                </ul>
              </div>
            ) : null}
            {pendingChange.preflight.managed_paths.length > 0 ? (
              <div>
                <p className="font-medium text-on-surface">
                  {t("conversation.scriptMarket.managedPaths")}
                </p>
                <ul className="mt-1 space-y-1 font-mono">
                  {pendingChange.preflight.managed_paths.map((path) => (
                    <li className="break-all" key={path}>{path}</li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
        ) : null}
      </ConfirmDialog>
    </section>
  );
}

function ScriptResourceRow({
  changeDisabled,
  disabled,
  entry,
  onInstall,
  onInspect,
  onLifecycle,
  onUse,
}: {
  changeDisabled: boolean;
  disabled: boolean;
  entry: ConversationAdapterPackageCatalogEntry;
  onInstall: () => void;
  onInspect: () => void;
  onLifecycle: () => void;
  onUse: () => void;
}) {
  const { t } = useI18n();
  const manifestPath = manifestPathForEntry(entry);
  const manifestDisplayPath = manifestDisplayPathForEntry(entry);
  const packageAction = packageActionForEntry(entry);
  const lifecycleAction = packageLifecycleAction(entry);
  const canUseInstalled = Boolean(
    manifestPath &&
      entry.installed &&
      entry.runtime_ready,
  );

  return (
    <article className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-theme-card-border bg-theme-card px-3 py-2 max-[640px]:grid-cols-1">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h4 className="min-w-0 truncate text-body-sm font-semibold text-on-surface">{entry.item.name}</h4>
          <Badge tone={statusBadgeTone(entry.status)}>{t(statusLabelKey(entry.status))}</Badge>
          {entry.update_available && entry.status !== "update_available" ? (
            <Badge tone="conflict">{t("conversation.scriptMarket.updateAvailable")}</Badge>
          ) : null}
          {entry.ahead_of_release && entry.status !== "ahead_of_release" ? (
            <Badge tone="create">{t("conversation.scriptMarket.aheadOfRelease")}</Badge>
          ) : null}
        </div>
        {entry.item.description ? (
          <p className="mt-1 line-clamp-2 text-body-sm text-on-surface-variant">{entry.item.description}</p>
        ) : null}
        <div className="mt-2 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-body-xs text-on-surface-variant">
          <span>{entry.item.provider ?? entry.item.id}</span>
          <span>{entry.item.version}</span>
          {manifestDisplayPath ? <span className="max-w-sm truncate">{manifestDisplayPath}</span> : null}
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
        {entry.error_message ? (
          <div className="mt-2 flex items-start gap-2 rounded-md border border-status-remove/30 bg-status-remove/10 px-2 py-1.5 text-body-xs text-status-remove">
            <CircleAlert className="mt-0.5 shrink-0" size={13} />
            <span className="min-w-0 break-words">{entry.error_message}</span>
          </div>
        ) : null}
      </div>
      <div className="flex items-center justify-end gap-2">
        <Button
          aria-label={t("conversation.scriptMarket.details")}
          className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
          disabled={disabled}
          onClick={onInspect}
          type="button"
          variant="outline"
        >
          <Info size={15} />
          {t("conversation.scriptMarket.details")}
        </Button>
        {canUseInstalled ? (
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
        {lifecycleAction ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={changeDisabled}
            onClick={onLifecycle}
            type="button"
            variant="outline"
          >
            <PowerOff size={15} />
            {t(actionLabelKey(lifecycleAction))}
          </Button>
        ) : null}
        {packageAction ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={changeDisabled}
            onClick={onInstall}
            type="button"
          >
            {packageAction === "repair" ? (
              <Wrench size={15} />
            ) : packageAction === "update" ? (
              <ShieldCheck size={15} />
            ) : (
              <Download size={15} />
            )}
            {t(actionLabelKey(packageAction))}
          </Button>
        ) : null}
      </div>
    </article>
  );
}

type PackageAction = "install" | "update" | "repair";
type PackageChangeAction = PackageAction | "unregister" | "uninstall";

function packageActionForEntry(entry: ConversationAdapterPackageCatalogEntry): PackageAction | null {
  const origin = entry.installed_package?.origin;
  if (entry.installed && origin !== "managed_release") {
    return null;
  }
  if (entry.status === "uninstalled") {
    return "install";
  }
  if (
    entry.status === "runtime_missing" ||
    entry.status === "verification_failed" ||
    entry.status === "hash_mismatch" ||
    entry.status === "manifest_invalid" ||
    entry.status === "core_incompatible"
  ) {
    return "repair";
  }
  if (entry.update_available || entry.status === "update_available") {
    return "update";
  }
  if (!entry.installed || entry.status === "not_installed") {
    return "install";
  }
  return null;
}

function packageLifecycleAction(
  entry: ConversationAdapterPackageCatalogEntry,
): "unregister" | "uninstall" | null {
  if (!entry.installed || entry.status === "built_in") {
    return null;
  }
  if (entry.status === "uninstalled") {
    return null;
  }
  return entry.installed_package?.origin === "managed_release" ? "uninstall" : "unregister";
}

function isUpdateOrRepairEntry(entry: ConversationAdapterPackageCatalogEntry) {
  return Boolean(
    entry.update_available ||
      [
        "update_available",
        "runtime_missing",
        "verification_failed",
        "hash_mismatch",
        "manifest_invalid",
        "core_incompatible",
      ].includes(entry.status),
  );
}

function statusLabelKey(status: ConversationAdapterPackageCatalogStatus) {
  switch (status) {
    case "installed":
    case "legacy_installed":
      return "conversation.scriptMarket.installed";
    case "uninstalled":
      return "conversation.scriptMarket.uninstalled";
    case "built_in":
      return "conversation.scriptMarket.builtIn";
    case "local_registered":
      return "conversation.scriptMarket.localRegistered";
    case "git_registered":
      return "conversation.scriptMarket.gitRegistered";
    case "dev_override":
      return "conversation.scriptMarket.devOverride";
    case "update_available":
      return "conversation.scriptMarket.updateAvailable";
    case "runtime_missing":
      return "conversation.scriptMarket.runtimeMissing";
    case "verification_failed":
      return "conversation.scriptMarket.verificationFailed";
    case "hash_mismatch":
      return "conversation.scriptMarket.hashMismatch";
    case "manifest_invalid":
      return "conversation.scriptMarket.manifestInvalid";
    case "core_incompatible":
      return "conversation.scriptMarket.coreIncompatible";
    case "ahead_of_release":
      return "conversation.scriptMarket.aheadOfRelease";
    case "not_installed":
    default:
      return "conversation.scriptMarket.notInstalled";
  }
}

function statusBadgeTone(status: ConversationAdapterPackageCatalogStatus) {
  switch (status) {
    case "installed":
    case "legacy_installed":
    case "built_in":
    case "local_registered":
    case "git_registered":
    case "dev_override":
    case "ahead_of_release":
      return "create";
    case "update_available":
    case "runtime_missing":
    case "uninstalled":
      return "conflict";
    case "verification_failed":
    case "hash_mismatch":
    case "manifest_invalid":
    case "core_incompatible":
      return "remove";
    case "not_installed":
    default:
      return "primary";
  }
}

function actionLabelKey(action: PackageChangeAction) {
  switch (action) {
    case "unregister":
      return "conversation.scriptMarket.uninstall";
    case "uninstall":
      return "conversation.scriptMarket.uninstall";
    case "repair":
      return "conversation.scriptMarket.repair";
    case "update":
      return "conversation.scriptMarket.update";
    case "install":
    default:
      return "conversation.scriptMarket.registerPackage";
  }
}

function packageTaskRunningLabel(action?: ConversationScriptInstallTaskSnapshot["action"]) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updating";
    case "uninstall":
      return "conversation.scriptMarket.uninstalling";
    case "install":
    default:
      return "conversation.scriptMarket.installing";
  }
}

function packageTaskCompletedLabel(action?: ConversationScriptInstallTaskSnapshot["action"]) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updateCompleted";
    case "uninstall":
      return "conversation.scriptMarket.uninstallCompleted";
    case "install":
    default:
      return "conversation.scriptMarket.installCompleted";
  }
}

function packageTaskFailedLabel(action?: ConversationScriptInstallTaskSnapshot["action"]) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updateFailed";
    case "uninstall":
      return "conversation.scriptMarket.uninstallFailed";
    case "install":
    default:
      return "conversation.scriptMarket.installFailed";
  }
}

function DetailField({
  label,
  value,
  wide = false,
}: {
  label: string;
  value: string;
  wide?: boolean;
}) {
  return (
    <div className={clsx("min-w-0", wide && "sm:col-span-2")}>
      <p className="text-body-xs font-medium text-on-surface-variant">{label}</p>
      <p className="mt-0.5 break-all font-mono text-body-xs text-on-surface">{value}</p>
    </div>
  );
}

function manifestPathForEntry(entry: ConversationAdapterPackageCatalogEntry) {
  if (entry.installed_package?.adapter_manifest_path) {
    return entry.installed_package.adapter_manifest_path;
  }
  if (entry.installed_adapter?.manifest_path) {
    return entry.installed_adapter.manifest_path;
  }
  if (!entry.install_path) {
    return null;
  }
  const manifestFile = entry.item.manifest_file?.trim() || "conversation-adapter.json";
  return `${entry.install_path.replace(/\/$/, "")}/${manifestFile}`;
}

function manifestDisplayPathForEntry(entry: ConversationAdapterPackageCatalogEntry) {
  return entry.display_manifest_path ?? manifestPathForEntry(entry);
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
