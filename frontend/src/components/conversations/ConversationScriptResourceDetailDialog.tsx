import {
  History,
  Info,
  Loader2,
  PowerOff,
  RotateCcw,
  Trash2,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { Button } from "../ui/button";
import { SimpleSelect } from "../ui/select";
import type {
  ConversationAdapterCatalogRelease,
  ConversationAdapterPackageCatalogEntry,
  ConversationAdapterPackageInspection,
  ConversationAdapterPackageVersion,
} from "../../services/conversations";
import type { ConversationPackageUpdatePolicy } from "../../types";
import { Badge } from "../foundation/Badge";
import { DialogFrame } from "../foundation/DialogFrame";
import { Skeleton } from "../foundation/skeleton";
import {
  DetailField,
  PackageActionIcon,
} from "./ConversationScriptResourceRow";
import {
  actionLabelKey,
  isBuiltInEntry,
  manifestDisplayPathForEntry,
  packageActionForEntry,
  packageLifecycleAction,
  packageTrustSummary,
} from "./conversationScriptResourceModel";

export function ConversationScriptResourceDetailDialog({
  entry,
  error,
  inspection,
  installedVersions,
  loading,
  onClose,
  onChangePolicy,
  onLifecycle,
  onMutateVersion,
  onPrimaryAction,
  onRequestDeleteVersion,
  onSelectVersion,
  policySaving,
  releases,
  selectedVersion,
  versionMutation,
}: {
  entry: ConversationAdapterPackageCatalogEntry;
  error: string | null;
  inspection: ConversationAdapterPackageInspection | null;
  installedVersions: ConversationAdapterPackageVersion[];
  loading: boolean;
  onClose: () => void;
  onChangePolicy: (policy: ConversationPackageUpdatePolicy) => void;
  onLifecycle: () => void;
  onMutateVersion: (
    action: "switch" | "rollback" | "delete",
    version?: string,
  ) => void;
  onPrimaryAction: () => void;
  onRequestDeleteVersion: (version: string) => void;
  onSelectVersion: (version: string) => void;
  policySaving: boolean;
  releases: ConversationAdapterCatalogRelease[];
  selectedVersion: string;
  versionMutation: string | null;
}) {
  const { t } = useI18n();
  const lifecycleAction = packageLifecycleAction(entry);
  const primaryAction = packageActionForEntry(entry);
  const trust = packageTrustSummary(entry);
  const unavailable = t("conversation.scriptMarket.valueUnavailable");

  return (
    <DialogFrame
      closeLabel={t("common.close")}
      contentClassName="grid gap-4"
      footer={
        <>
          <Button onClick={() => onClose()} type="button" variant="outline">
            {t("common.close")}
          </Button>
          {lifecycleAction ? (
            <Button onClick={onLifecycle} type="button" variant="outline">
              <PowerOff size={15} />
              {t(actionLabelKey(lifecycleAction!))}
            </Button>
          ) : null}
          {primaryAction ? (
            <Button onClick={() => onPrimaryAction()} type="button">
              <PackageActionIcon action={primaryAction!} />
              {t(actionLabelKey(primaryAction!))}
            </Button>
          ) : null}
        </>
      }
      icon={<Info size={18} />}
      onClose={() => onClose()}
      size="xl"
      title={entry.item.name}
    >
      {loading ? (
        <div aria-busy="true" className="grid gap-2" role="status">
          <span className="sr-only">
            {t("conversation.scriptMarket.loading")}
          </span>
          <Skeleton className="h-5 w-52 rounded" />
          <Skeleton className="h-32 rounded" />
        </div>
      ) : error ? (
        <div className="rounded-xl border border-status-remove/35 bg-status-remove/10 p-3 text-body-sm text-status-remove">
          {error}
        </div>
      ) : (
        <>
          <div className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card/65 p-3 sm:grid-cols-2">
            <DetailField
              label={t("conversation.scriptMarket.packageId")}
              value={entry.installed_package?.package_id ?? entry.item.id}
            />
            <DetailField
              label={t("conversation.scriptMarket.adapterId")}
              value={
                entry.installed_adapter?.id ?? entry.item.adapter_id ?? "-"
              }
            />
            <DetailField
              label={t("conversation.scriptMarket.origin")}
              value={
                inspection?.origin ??
                entry.installed_package?.origin ??
                entry.item.provider ??
                "-"
              }
            />
            <DetailField
              label={t("conversation.scriptMarket.publisher")}
              value={releases[0]?.publisher ?? entry.item.provider ?? "-"}
            />
            <DetailField
              label={t("conversation.scriptMarket.currentVersion")}
              value={
                entry.installed_package?.version ??
                entry.installed_adapter?.version ??
                t("conversation.scriptMarket.notInstalled")
              }
            />
            <DetailField
              label={t("conversation.scriptMarket.latestVersion")}
              value={releases[0]?.version ?? entry.item.version}
            />
            <DetailField
              label={t("conversation.scriptMarket.runtimeGate")}
              value={
                entry.installed_package?.runtime_gate_status ?? entry.status
              }
            />
            <DetailField
              label={t("conversation.scriptMarket.recordKind")}
              value={entry.item.record_kind}
            />
            <DetailField
              label={t("conversation.scriptMarket.path")}
              value={
                entry.display_install_path ??
                entry.installed_package?.install_dir ??
                entry.install_path ??
                "-"
              }
              wide
            />
            <DetailField
              label={t("conversation.scriptMarket.manifest")}
              value={manifestDisplayPathForEntry(entry) ?? "-"}
              wide
            />
            <DetailField
              label={t("conversation.scriptMarket.trustedVersion")}
              value={trust?.trustedVersion ?? unavailable}
            />
            <DetailField
              label={t("conversation.scriptMarket.managedDirectoryVersion")}
              value={trust?.managedDirectoryVersion ?? unavailable}
            />
            <DetailField
              label={t("conversation.scriptMarket.contentHash")}
              value={trust?.contentHash ?? unavailable}
              wide
            />
            <DetailField
              label={t("conversation.scriptMarket.trustedHash")}
              value={trust?.trustedHash ?? unavailable}
              wide
            />
          </div>

          {entry.installed_package?.origin === "dev_override" ? (
            <div className="rounded-xl border border-status-update/35 bg-status-update/10 p-3 text-body-sm text-status-update">
              {t("conversation.scriptMarket.devOverrideNotice")}
            </div>
          ) : null}

          {inspection?.affected_sources.length ? (
            <section>
              <h4 className="text-body-sm font-semibold text-on-surface">
                {t("conversation.scriptMarket.affectedSources")}
              </h4>
              <ul className="mt-2 grid gap-1 text-body-sm text-on-surface-variant">
                {inspection.affected_sources.map((source) => (
                  <li
                    className="rounded-xl border border-theme-card-border px-2 py-1.5"
                    key={source.id}
                  >
                    {source.name} · {source.id}
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          {entry.installed_package?.origin === "managed_release" ? (
            <section>
              <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-theme-card-border bg-theme-control/40 p-3">
                <label
                  className="text-body-sm font-medium text-on-surface"
                  htmlFor="conversation-package-update-policy"
                >
                  {t("conversation.scriptMarket.updatePolicy")}
                </label>
                <SimpleSelect
                  ariaLabel={t("conversation.scriptMarket.updatePolicy")}
                  className="w-auto min-w-36"
                  disabled={policySaving}
                  id="conversation-package-update-policy"
                  onChange={(nextPolicy) =>
                    onChangePolicy(
                      nextPolicy as ConversationPackageUpdatePolicy,
                    )
                  }
                  options={(
                    [
                      "manual",
                      "follow_stable",
                      "follow_beta",
                      "pin_exact",
                    ] as const
                  ).map((policy) => ({
                    label: t(`conversation.scriptMarket.policy.${policy}`),
                    value: policy,
                  }))}
                  size="sm"
                  value={entry.installed_package.update_policy}
                />
              </div>
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-body-sm font-semibold text-on-surface">
                  {t("conversation.scriptMarket.installedVersions")}
                </h4>
                {entry.runtime_ready &&
                installedVersions.some(
                  (version) =>
                    version.version !== entry.installed_package?.version,
                ) ? (
                  <Button
                    disabled={Boolean(versionMutation)}
                    onClick={() => onMutateVersion("rollback")}
                    type="button"
                    variant="outline"
                  >
                    <RotateCcw size={15} />
                    {t("conversation.scriptMarket.rollback")}
                  </Button>
                ) : null}
              </div>
              <p className="mt-1 text-body-xs text-on-surface-variant">
                {t(
                  entry.runtime_ready
                    ? "conversation.scriptMarket.deleteRunningVersionHelp"
                    : "conversation.scriptMarket.deleteVersionHelp",
                )}
              </p>
              <div className="mt-2 grid gap-2">
                {installedVersions.map((version) => {
                  const active =
                    entry.runtime_ready &&
                    version.version === entry.installed_package?.version;
                  return (
                    <div
                      className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-theme-card-border bg-theme-card/55 p-3"
                      key={version.version}
                    >
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-body-sm font-semibold text-on-surface">
                          {version.version}
                        </span>
                        {active ? (
                          <Badge tone="primary">
                            {t("conversation.scriptMarket.activeVersion")}
                          </Badge>
                        ) : null}
                        <span className="text-body-xs text-on-surface-variant">
                          {version.runtime_gate_status}
                        </span>
                      </div>
                      {!active ? (
                        <div className="flex gap-2">
                          <Button
                            disabled={Boolean(versionMutation)}
                            onClick={() =>
                              onMutateVersion("switch", version.version)
                            }
                            type="button"
                            variant="outline"
                          >
                            {versionMutation === `switch:${version.version}` ? (
                              <Loader2 className="animate-spin" size={15} />
                            ) : null}
                            {t(
                              entry.status === "uninstalled"
                                ? "conversation.scriptMarket.registerVersion"
                                : "conversation.scriptMarket.switchVersion",
                            )}
                          </Button>
                          <Button
                            disabled={Boolean(versionMutation)}
                            onClick={() =>
                              onRequestDeleteVersion(version.version)
                            }
                            type="button"
                            variant="destructive"
                          >
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
          ) : entry.installed ? (
            <section className="rounded-xl border border-theme-card-border bg-theme-control/40 p-3">
              <h4 className="text-body-sm font-semibold text-on-surface">
                {t("conversation.scriptMarket.installedVersions")}
              </h4>
              <p className="mt-1 text-body-sm text-on-surface-variant">
                {t(
                  isBuiltInEntry(entry)
                    ? "conversation.scriptMarket.builtInDeleteUnavailable"
                    : "conversation.scriptMarket.externalDeleteUnavailable",
                )}
              </p>
              {!isBuiltInEntry(entry) ? (
                <Button
                  className="mt-3"
                  disabled
                  type="button"
                  variant="destructive"
                >
                  <Trash2 size={15} />
                  {t("conversation.scriptMarket.deleteVersion")}
                </Button>
              ) : null}
            </section>
          ) : null}

          <section>
            <div className="flex items-center justify-between gap-2">
              <h4 className="flex items-center gap-2 text-body-sm font-semibold text-on-surface">
                <History size={15} />
                {t("conversation.scriptMarket.versionHistory")}
              </h4>
              {releases.length > 0 ? (
                <SimpleSelect
                  ariaLabel={t("conversation.scriptMarket.selectVersion")}
                  className="w-auto min-w-44"
                  onChange={(version) => onSelectVersion(version)}
                  options={releases.map((release) => ({
                    label: `${release.version} · ${release.channel}`,
                    value: release.version,
                  }))}
                  size="sm"
                  value={selectedVersion}
                />
              ) : null}
            </div>
            {releases.length === 0 ? (
              <p className="mt-2 text-body-sm text-on-surface-variant">
                {t("conversation.scriptMarket.noVersionHistory")}
              </p>
            ) : (
              <div className="mt-2 grid gap-2">
                {releases.map((release) => (
                  <article
                    className="rounded-xl border border-theme-card-border bg-theme-card/55 p-3"
                    key={release.version}
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-body-sm font-semibold text-on-surface">
                        {release.version}
                      </span>
                      <Badge
                        tone={release.breaking_change ? "remove" : "primary"}
                      >
                        {release.channel}
                      </Badge>
                      <span className="text-body-xs text-on-surface-variant">
                        Core {release.core_compatibility}
                      </span>
                    </div>
                    <pre className="mt-2 whitespace-pre-wrap font-sans text-body-sm leading-6 text-on-surface-variant">
                      {release.changelog_markdown}
                    </pre>
                  </article>
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </DialogFrame>
  );
}
