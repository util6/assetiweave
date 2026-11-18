import clsx from "clsx";
import {
  CircleAlert,
  Loader2,
  PackageCheck,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import { Skeleton } from "../foundation/skeleton";
import { TruncatedText } from "../foundation/TruncatedText";
import { formatErrorMessage } from "../../utils/error";
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
  registerConversationAdapter,
  rollbackConversationAdapterPackageVersion,
  setConversationAdapterPackageUpdatePolicy,
  switchConversationAdapterPackageVersion,
  subscribeConversationScriptInstallTask,
  unregisterConversationAdapter,
  uninstallConversationAdapterPackage,
  updateConversationAdapterPackage,
  type ConversationAdapterPackageChangePreflight,
  type ConversationAdapterPackageCatalogEntry,
  type ConversationAdapterCatalogRelease,
  type ConversationAdapterPackageInspection,
  type ConversationAdapterPackageVersion,
  type ConversationScriptInstallTaskSnapshot,
} from "../../services/conversations";
import type {
  ConversationPackageUpdatePolicy,
  ConversationRecordKind,
} from "../../types";
import type { NotificationMessage } from "../notifications/NotificationBanner";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { Button } from "../ui/button";
import { ConversationScriptResourceDetailDialog } from "./ConversationScriptResourceDetailDialog";
import { ConversationScriptResourceRow } from "./ConversationScriptResourceRow";
import {
  actionLabelKey,
  countUpdateOrRepairEntries,
  isBuiltInEntry,
  isUpdateOrRepairEntry,
  manifestPathForEntry,
  manifestPathFromInstallResult,
  packageActionForEntry,
  packageLifecycleAction,
  packageTaskCompletedLabel,
  packageTaskFailedLabel,
  packageTaskRunningLabel,
  resolvePackageChangeVersion,
  type PackageChangeAction,
} from "./conversationScriptResourceModel";

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
  onManifestSelect?: (manifestPath: string) => void;
  onNotify: (notification: ScriptResourceNotification) => void;
  onNotifyError: (message: string) => void;
  recordKind: ConversationRecordKind;
}) {
  const { t } = useI18n();
  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  const onManifestSelectRef = useRef(onManifestSelect);
  onManifestSelectRef.current = onManifestSelect;
  const onNotifyRef = useRef(onNotify);
  onNotifyRef.current = onNotify;
  const onNotifyErrorRef = useRef(onNotifyError);
  onNotifyErrorRef.current = onNotifyError;

  const notify = useCallback((notification: ScriptResourceNotification) => {
    onNotifyRef.current(notification);
  }, []);
  const notifyError = useCallback((message: string) => {
    onNotifyErrorRef.current(message);
  }, []);
  const notifyInstalled = useCallback(() => {
    return onInstalledRef.current();
  }, []);
  const notifyManifestSelect = useCallback((manifestPath: string) => {
    onManifestSelectRef.current?.(manifestPath);
  }, []);

  const [entries, setEntries] = useState<
    ConversationAdapterPackageCatalogEntry[]
  >([]);
  const [activeView, setActiveView] = useState<
    "connected" | "updates" | "discover"
  >("connected");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installTask, setInstallTask] =
    useState<ConversationScriptInstallTaskSnapshot | null>(null);
  const [pendingChange, setPendingChange] = useState<{
    action: PackageChangeAction;
    entry: ConversationAdapterPackageCatalogEntry;
    preflight: ConversationAdapterPackageChangePreflight;
  } | null>(null);
  const [confirmingChange, setConfirmingChange] = useState(false);
  const [detailEntry, setDetailEntry] =
    useState<ConversationAdapterPackageCatalogEntry | null>(null);
  const [detailInspection, setDetailInspection] =
    useState<ConversationAdapterPackageInspection | null>(null);
  const [detailReleases, setDetailReleases] = useState<
    ConversationAdapterCatalogRelease[]
  >([]);
  const [installedVersions, setInstalledVersions] = useState<
    ConversationAdapterPackageVersion[]
  >([]);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [selectedVersion, setSelectedVersion] = useState<string>("");
  const [versionMutation, setVersionMutation] = useState<string | null>(null);
  const [pendingVersionDeletion, setPendingVersionDeletion] = useState<
    string | null
  >(null);
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
        return nextEntries;
      } catch (loadError) {
        const message = errorMessage(loadError);
        setError(message);
        notifyError(message);
        return null;
      } finally {
        setLoading(false);
        setRefreshing(false);
      }
    },
    [notifyError],
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
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void subscribeConversationScriptInstallTask((snapshot) => {
      if (!cancelled) {
        setInstallTask(snapshot);
      }
    })
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
    setDetailEntry((current) => {
      if (!current) {
        return current;
      }
      return (
        entries.find((entry) => entry.item.id === current.item.id) ?? current
      );
    });
  }, [entries]);

  useEffect(() => {
    if (
      !installTask ||
      installTask.status === "running" ||
      handledInstallTaskIds.current.has(installTask.id)
    ) {
      return;
    }
    handledInstallTaskIds.current.add(installTask.id);
    if (installTask.status === "completed") {
      const manifestPath = manifestPathFromInstallResult(installTask.result);
      if (manifestPath) {
        notifyManifestSelect(manifestPath);
      }
      notify({
        message: t(packageTaskCompletedLabel(installTask.action)),
        tone: "success",
      });
      void Promise.resolve(notifyInstalled()).finally(() => {
        void loadCatalog("refresh");
      });
      return;
    }

    notifyError(
      installTask.error?.message ||
        t(packageTaskFailedLabel(installTask.action)),
    );
  }, [
    installTask,
    loadCatalog,
    notify,
    notifyError,
    notifyInstalled,
    notifyManifestSelect,
    t,
  ]);

  const recordEntries = useMemo(
    () => entries.filter((entry) => entry.item.record_kind === recordKind),
    [entries, recordKind],
  );
  const viewCounts = useMemo(
    () => ({
      connected: recordEntries.filter((entry) => entry.installed).length,
      updates: countUpdateOrRepairEntries(recordEntries),
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
  const isInitialLoading = loading && entries.length === 0;
  const isValidating = (loading || refreshing) && entries.length > 0;

  async function handleInstall(entry: ConversationAdapterPackageCatalogEntry) {
    const action = packageActionForEntry(entry);
    if (!action) {
      return;
    }
    if (!entry.installed && entry.item.source.type === "local_directory") {
      const manifestPath = manifestPathForEntry(entry);
      if (!manifestPath) {
        notifyError(t("conversation.scriptMarket.localManifestMissing"));
        return;
      }
      try {
        await registerConversationAdapter(manifestPath, false, true);
        notify({
          message: t("conversation.scriptMarket.registerCompleted"),
          tone: "success",
        });
        await Promise.resolve(notifyInstalled());
        await loadCatalog("refresh");
      } catch (registerError) {
        notifyError(errorMessage(registerError));
      }
      return;
    }
    if (entry.status === "uninstalled" && isBuiltInEntry(entry)) {
      const manifestPath = manifestPathForEntry(entry);
      if (!manifestPath) {
        notifyError(t("conversation.scriptMarket.builtInManifestMissing"));
        return;
      }
      try {
        await registerConversationAdapter(manifestPath, false, true);
        notify({
          message: t("conversation.scriptMarket.registerCompleted"),
          tone: "success",
        });
        await Promise.resolve(notifyInstalled());
        await loadCatalog("refresh");
      } catch (registerError) {
        notifyError(errorMessage(registerError));
      }
      return;
    }
    await beginPackageChange(entry, action);
  }

  async function handleCheckUpdates() {
    setCheckingUpdates(true);
    try {
      await checkConversationAdapterPackageUpdates({ force: true });
      const refreshedEntries = await loadCatalog("refresh");
      setActiveView("updates");
      // 与“可更新”标签页同口径：更新 ∪ 修复，而不是只统计 update_available。
      const count = countUpdateOrRepairEntries(
        (refreshedEntries ?? []).filter(
          (entry) => entry.item.record_kind === recordKind,
        ),
      );
      notify({
        message: t("conversation.scriptMarket.checkUpdatesCompleted", {
          count,
        }),
        tone: "success",
      });
    } catch (checkError) {
      notifyError(errorMessage(checkError));
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
        action: action === "repair" ? "update" : action,
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
      notifyError(errorMessage(installError));
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
        notify({
          message: t("conversation.scriptMarket.uninstallCompleted"),
          tone: "success",
        });
        await Promise.resolve(notifyInstalled());
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
      const version = resolvePackageChangeVersion({
        entryId: entry.item.id,
        detailEntryId: detailEntry?.item.id ?? null,
        releaseVersions: detailReleases.map((release) => release.version),
        selectedVersion,
      });
      const task =
        action === "update" || action === "repair"
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
        notify({
          message: t("conversation.scriptMarket.installStarted"),
          tone: "info",
        });
      }
    } catch (installError) {
      notifyError(errorMessage(installError));
    } finally {
      setConfirmingChange(false);
    }
  }

  async function openPackageDetail(
    entry: ConversationAdapterPackageCatalogEntry,
  ) {
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
              adapterId:
                entry.installed_adapter?.id ?? entry.item.adapter_id ?? null,
            }).catch(() => null)
          : Promise.resolve(null),
        listConversationAdapterPackageReleases({
          packageId: entry.item.id,
          refresh: false,
        }).catch(() => []),
        entry.installed_package?.origin === "managed_release"
          ? listInstalledConversationAdapterPackageVersions(packageId).catch(
              () => [],
            )
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

  async function mutateInstalledVersion(
    action: "switch" | "rollback" | "delete",
    version?: string,
  ) {
    if (!detailEntry?.installed_package) {
      return;
    }
    const packageId = detailEntry.installed_package.package_id;
    const mutationKey = `${action}:${version ?? "previous"}`;
    setVersionMutation(mutationKey);
    try {
      if (action === "switch" && version) {
        await switchConversationAdapterPackageVersion({
          packageId,
          version,
          confirmed: true,
        });
      } else if (action === "rollback") {
        await rollbackConversationAdapterPackageVersion({
          packageId,
          confirmed: true,
        });
      } else if (action === "delete" && version) {
        await deleteConversationAdapterPackageVersion({
          packageId,
          version,
          confirmed: true,
        });
      }
      const [nextVersions] = await Promise.all([
        listInstalledConversationAdapterPackageVersions(packageId),
        loadCatalog("refresh"),
      ]);
      setInstalledVersions(nextVersions);
      if (action !== "delete" || nextVersions.length === 0) {
        setDetailEntry(null);
      }
      notify({
        message: t("conversation.scriptMarket.versionActionCompleted"),
        tone: "success",
      });
    } catch (mutationError) {
      notifyError(errorMessage(mutationError));
    } finally {
      setVersionMutation(null);
    }
  }

  async function changeUpdatePolicy(
    updatePolicy: ConversationPackageUpdatePolicy,
  ) {
    if (!detailEntry?.installed_package) {
      return;
    }
    setPolicySaving(true);
    try {
      const updated = await setConversationAdapterPackageUpdatePolicy({
        packageId: detailEntry.installed_package.package_id,
        updatePolicy,
      });
      setDetailEntry((current) =>
        current ? { ...current, installed_package: updated } : current,
      );
      notify({
        message: t("conversation.scriptMarket.policySaved"),
        tone: "success",
      });
    } catch (policyError) {
      notifyError(errorMessage(policyError));
    } finally {
      setPolicySaving(false);
    }
  }

  return (
    <section className="rounded-2xl border border-theme-card-border bg-theme-card/55 p-3">
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
            className="inline-flex h-9 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control px-3 text-body-sm text-theme-control-fg transition-[transform,background-color,border-color,box-shadow,color] duration-200 hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
            disabled={disabled || loading || refreshing || checkingUpdates}
            onClick={() => void loadCatalog("refresh")}
            type="button"
          >
            <RefreshCw
              className={clsx(refreshing && "animate-spin")}
              size={15}
            />
            <span>{t("common.refresh")}</span>
          </button>
          <Button
            disabled={
              disabled ||
              loading ||
              refreshing ||
              checkingUpdates ||
              installRunning
            }
            onClick={() => void handleCheckUpdates()}
            type="button"
            variant="outline"
          >
            <ShieldCheck
              className={clsx(checkingUpdates && "animate-pulse")}
              size={15}
            />
            {checkingUpdates
              ? t("conversation.scriptMarket.checkingUpdates")
              : t("conversation.scriptMarket.checkUpdates")}
          </Button>
        </div>
      </div>

      <div
        className="conversation-pill-tabs mt-3 flex items-center gap-1"
        role="tablist"
      >
        {(["connected", "updates", "discover"] as const).map((view) => (
          <button
            aria-selected={activeView === view}
            className={clsx(
              "conversation-pill-tab rounded-xl px-3 py-2 text-body-sm transition-[transform,background-color,color] duration-200 hover:-translate-y-px active:translate-y-0",
              activeView === view
                ? "text-on-surface"
                : "text-on-surface-variant hover:bg-theme-control-hover/70 hover:text-on-surface",
            )}
            data-active={activeView === view}
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
        <div className="mt-3 flex items-center gap-2 rounded-xl border border-status-update/35 bg-status-update/10 px-3 py-2 text-body-sm text-status-update">
          <Loader2 className="shrink-0 animate-spin" size={15} />
          <TruncatedText>
            {`${t(packageTaskRunningLabel(installTask.action))} - ${installTask.package_id ?? installTask.item_id}`}
          </TruncatedText>
        </div>
      ) : null}

      {error ? (
        <div className="mt-3 flex items-start gap-2 rounded-xl border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove">
          <CircleAlert className="mt-0.5 shrink-0" size={15} />
          <span>{error}</span>
        </div>
      ) : isInitialLoading ? (
        <div className="mt-3 grid gap-2">
          <div
            aria-live="polite"
            className="flex items-center gap-2 rounded-xl border border-status-update/30 bg-status-update/10 px-3 py-2 text-body-sm text-status-update"
            role="status"
          >
            <Loader2 className="shrink-0 animate-spin" size={15} />
            <span>{t("conversation.scriptMarket.loading")}</span>
          </div>
          {Array.from({ length: 2 }).map((_, index) => (
            <div
              className="rounded-xl border border-theme-card-border bg-theme-control/40 p-3"
              key={index}
            >
              <Skeleton className="h-4 w-48 max-w-full rounded" />
              <Skeleton className="mt-2 h-3 w-full max-w-lg rounded" />
            </div>
          ))}
        </div>
      ) : (
        <>
          {isValidating ? (
            <div
              aria-live="polite"
              className="mt-3 flex items-center gap-2 rounded-xl border border-status-update/30 bg-status-update/10 px-3 py-2 text-body-sm text-status-update"
              role="status"
            >
              <Loader2 className="shrink-0 animate-spin" size={15} />
              <span>{t("conversation.scriptMarket.loading")}</span>
            </div>
          ) : null}
          {visibleEntries.length === 0 ? (
            <div className="mt-3 rounded-xl border border-theme-card-border bg-theme-control/40 px-3 py-2 text-body-sm text-on-surface-variant">
              {t("conversation.scriptMarket.emptyForKind")}
            </div>
          ) : (
            <div className="mt-3 grid max-h-72 gap-2 overflow-y-auto pr-1">
              {visibleEntries.map((entry) => (
                <ConversationScriptResourceRow
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
                />
              ))}
            </div>
          )}
        </>
      )}
      {detailEntry ? (
        <ConversationScriptResourceDetailDialog
          entry={detailEntry}
          error={detailError}
          inspection={detailInspection}
          installedVersions={installedVersions}
          loading={detailLoading}
          onChangePolicy={(policy) => void changeUpdatePolicy(policy)}
          onClose={() => setDetailEntry(null)}
          onLifecycle={() => {
            const action = packageLifecycleAction(detailEntry);
            if (action) {
              void beginPackageChange(detailEntry, action);
            }
          }}
          onMutateVersion={(action, version) =>
            void mutateInstalledVersion(action, version)
          }
          onPrimaryAction={() => void handleInstall(detailEntry)}
          onRequestDeleteVersion={setPendingVersionDeletion}
          onSelectVersion={setSelectedVersion}
          policySaving={policySaving}
          releases={detailReleases}
          selectedVersion={selectedVersion}
          versionMutation={versionMutation}
        />
      ) : null}

      <ConfirmDialog
        busy={confirmingChange}
        confirmLabel={
          pendingChange ? t(actionLabelKey(pendingChange.action)) : undefined
        }
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
          <div className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-control/45 p-3 text-body-xs text-on-surface-variant">
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
                    <li className="break-all" key={path}>
                      {path}
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
        ) : null}
      </ConfirmDialog>

      <ConfirmDialog
        busy={Boolean(versionMutation)}
        confirmLabel={t("conversation.scriptMarket.deleteVersion")}
        layer="nested"
        message={t("conversation.scriptMarket.deleteVersionConfirm", {
          version: pendingVersionDeletion ?? "",
        })}
        onClose={() => setPendingVersionDeletion(null)}
        onConfirm={() => {
          const version = pendingVersionDeletion;
          setPendingVersionDeletion(null);
          if (version) {
            void mutateInstalledVersion("delete", version);
          }
        }}
        open={Boolean(pendingVersionDeletion)}
        title={t("conversation.scriptMarket.deleteVersion")}
        tone="danger"
      />
    </section>
  );
}

function errorMessage(error: unknown) {
  return formatErrorMessage(error);
}
