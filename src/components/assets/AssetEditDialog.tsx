import clsx from "clsx";
import { Archive, FilePenLine, X } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { assetKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { Asset, AssetGroupDetail, AssetMountStatus, Source, TargetProfile } from "../../types";
import { assetSourceHref, assetSourceLabel } from "../../utils/assetSource";
import { openExternalLink } from "../../utils/externalLinks";
import { getMountDisplayState } from "../../utils/mountState";
import { displayAssetPath } from "../../utils/path";
import { Button } from "../ui/button";

export function AssetEditDialog({
  asset,
  busy,
  groups,
  mountStatuses,
  onBackup,
  onClose,
  onSetGroupMembership,
  onSubmit,
  onToggleMount,
  profiles,
  source,
}: {
  asset: Asset | null;
  busy: boolean;
  groups: AssetGroupDetail[];
  mountStatuses: AssetMountStatus[];
  onBackup?: () => Promise<void>;
  onClose: () => void;
  onSetGroupMembership: (group: AssetGroupDetail, enabled: boolean) => Promise<void>;
  onSubmit: (description: string | null) => Promise<void>;
  onToggleMount: (profileId: string) => Promise<void>;
  profiles: TargetProfile[];
  source?: Source;
}) {
  const { t } = useI18n();
  const [description, setDescription] = useState("");
  const assetMountStatuses = useMemo(
    () => (asset ? mountStatuses.filter((status) => status.asset_id === asset.id) : []),
    [asset, mountStatuses],
  );

  useEffect(() => {
    setDescription(asset?.description ?? "");
  }, [asset]);

  if (!asset) {
    return null;
  }
  const sourceHref = assetSourceHref(asset);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit(description.trim() || null);
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/72 px-4 py-6 backdrop-blur-sm" role="presentation">
      <section
        aria-label={t("asset.editDialog.title")}
        aria-modal="true"
        className="flex max-h-[92vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-theme-card-border bg-theme-card shadow-[0_24px_70px_rgb(var(--theme-panel-shadow)/0.34)]"
        role="dialog"
      >
        <header className="flex min-h-14 items-center justify-between gap-3 border-b border-theme-card-border bg-theme-card-header px-4 py-3">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-xl border border-status-update/30 bg-status-update/15 text-status-update">
              <FilePenLine size={18} />
            </span>
            <div className="min-w-0">
              <h2 className="truncate text-body-md font-semibold text-on-surface">{t("asset.editDialog.title")}</h2>
              <p className="mt-0.5 truncate font-mono text-body-sm text-outline">{asset.name}</p>
            </div>
          </div>
          <button
            aria-label={t("common.close")}
            className="grid size-8 place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
            disabled={busy}
            onClick={onClose}
            title={t("common.close")}
            type="button"
          >
            <X size={17} />
          </button>
        </header>

        <form className="grid gap-4 overflow-y-auto p-4" onSubmit={(event) => void handleSubmit(event)}>
          <section className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card-header/55 p-3">
            <p className="text-body-sm text-on-surface-variant">{t("asset.editDialog.readonlyMeta")}</p>
            <div className="grid gap-2 text-body-sm">
              <ReadonlyRow href={sourceHref} label={t("asset.source")} value={assetSourceLabel(asset, source)} mono />
              <ReadonlyRow label={t("asset.deleteDialog.path")} value={displayAssetPath(asset)} mono />
              <ReadonlyRow label={t("source.field.defaultKind")} value={assetKindLabel(asset.kind, t)} />
            </div>
          </section>

          <label className="grid gap-1.5">
            <span className="text-body-sm font-medium text-on-surface-variant">{t("asset.description")}</span>
            <textarea
              className="min-h-28 resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-body-sm text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onChange={(event) => setDescription(event.target.value)}
              placeholder={t("asset.editDialog.descriptionPlaceholder")}
              value={description}
            />
          </label>

          <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card-header/55 p-3">
            <div>
              <div className="text-label-caps uppercase text-outline">{t("asset.editDialog.groups")}</div>
              <p className="mt-1 text-body-sm text-on-surface-variant">{t("asset.editDialog.groupsHelp")}</p>
            </div>
            {groups.length === 0 ? (
              <div className="rounded-lg border border-theme-card-border bg-theme-card/70 px-3 py-4 text-body-sm text-on-surface-variant">
                {t("asset.editDialog.noGroups")}
              </div>
            ) : (
              <div className="grid gap-2">
                {groups.map((group) => (
                  <AssetGroupMembershipRow
                    asset={asset}
                    busy={busy}
                    group={group}
                    key={group.group.id}
                    onSetGroupMembership={onSetGroupMembership}
                  />
                ))}
              </div>
            )}
          </section>

          <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card-header/55 p-3">
            <div>
              <div className="text-label-caps uppercase text-outline">{t("asset.editDialog.mounts")}</div>
              <p className="mt-1 text-body-sm text-on-surface-variant">{t("asset.editDialog.mountsHelp")}</p>
            </div>
            {profiles.length === 0 ? (
              <div className="rounded-lg border border-theme-card-border bg-theme-card/70 px-3 py-4 text-body-sm text-on-surface-variant">
                {t("asset.editDialog.noProfiles")}
              </div>
            ) : (
              <div className="grid gap-2">
                {profiles.map((profile) => {
                  const status = assetMountStatuses.find((candidate) => candidate.profile_id === profile.id);
                  return (
                    <AssetProfileMountRow
                      busy={busy}
                      key={profile.id}
                      onToggleMount={onToggleMount}
                      profile={profile}
                      status={status}
                    />
                  );
                })}
              </div>
            )}
          </section>

          <footer className="flex items-center justify-between gap-3 border-t border-theme-card-border pt-4 max-[640px]:flex-col max-[640px]:items-stretch">
            <div className="max-[640px]:grid">
              {asset.kind === "skill" && onBackup && (
                <Button
                  className="max-[640px]:w-full"
                  disabled={busy || Boolean(asset.backup_status)}
                  onClick={() => void onBackup()}
                  type="button"
                  variant="outline"
                >
                  <Archive size={16} />
                  {asset.backup_status ? t("backup.action.inDirectory") : t("backup.action.backupToDirectory")}
                </Button>
              )}
            </div>
            <div className="flex items-center justify-end gap-2 max-[640px]:grid max-[640px]:grid-cols-2">
              <Button disabled={busy} onClick={onClose} type="button" variant="outline">
                {t("common.cancel")}
              </Button>
              <Button disabled={busy} type="submit">
                {busy ? t("common.saving") : t("asset.editDialog.submit")}
              </Button>
            </div>
          </footer>
        </form>
      </section>
    </div>
  );
}

function AssetGroupMembershipRow({
  asset,
  busy,
  group,
  onSetGroupMembership,
}: {
  asset: Asset;
  busy: boolean;
  group: AssetGroupDetail;
  onSetGroupMembership: (group: AssetGroupDetail, enabled: boolean) => Promise<void>;
}) {
  const { t } = useI18n();
  const member = group.members.find((candidate) => candidate.asset_id === asset.id);
  const manualMember = group.manual_asset_ids.includes(asset.id);
  const ruleMatched = member?.origin === "rule" || member?.origin === "manual_and_rule";
  const inGroup = Boolean(member) || manualMember;
  const canRemoveManual = manualMember;
  const canAddManual = !manualMember && !ruleMatched;

  return (
    <div className="grid min-h-[72px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-lg border border-theme-card-border bg-theme-card/70 px-3 py-2 max-[720px]:grid-cols-1">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="truncate font-mono text-code-md font-semibold text-on-surface">{group.group.name}</span>
          {inGroup && <StatusChip>{t("asset.editDialog.inGroup")}</StatusChip>}
          {manualMember && <StatusChip>{t("asset.editDialog.manualMember")}</StatusChip>}
          {ruleMatched && <StatusChip>{t("asset.editDialog.ruleMatched")}</StatusChip>}
          {!group.group.enabled && <StatusChip>{t("group.disabled")}</StatusChip>}
        </div>
        <p className="mt-1 line-clamp-1 text-body-sm text-on-surface-variant">
          {group.group.description ?? t("group.noDescription")}
        </p>
      </div>
      <Button
        disabled={busy || (!canAddManual && !canRemoveManual)}
        onClick={() => void onSetGroupMembership(group, canAddManual)}
        type="button"
        variant={canRemoveManual ? "outline" : "secondary"}
      >
        {canRemoveManual ? t("asset.editDialog.removeManualGroup") : t("asset.editDialog.addToGroup")}
      </Button>
    </div>
  );
}

function AssetProfileMountRow({
  busy,
  onToggleMount,
  profile,
  status,
}: {
  busy: boolean;
  onToggleMount: (profileId: string) => Promise<void>;
  profile: TargetProfile;
  status?: AssetMountStatus;
}) {
  const { t } = useI18n();
  const displayState = getMountDisplayState(status);
  const mounted = displayState === "mounted";
  const issue = displayState === "conflict" || displayState === "broken";

  return (
    <div className="grid min-h-[72px] grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-3 rounded-lg border border-theme-card-border bg-theme-card/70 px-3 py-2 max-[720px]:grid-cols-1">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="truncate font-mono text-code-md font-semibold text-on-surface">{profile.name}</span>
          <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
            {profile.app_kind}
          </span>
        </div>
        <p className="mt-1 truncate font-mono text-body-sm text-outline">{profile.target_paths[0] ?? ""}</p>
      </div>
      <span
        className={clsx(
          "rounded-md px-2 py-0.5 text-label-caps uppercase",
          mounted && "bg-status-create/15 text-status-create",
          issue && "bg-status-remove/15 text-status-remove",
          !mounted && !issue && "bg-theme-control-hover text-outline",
        )}
      >
        {t(`mount.display.${displayState}`)}
      </span>
      <Button disabled={busy || issue} onClick={() => void onToggleMount(profile.id)} type="button" variant="outline">
        {mounted ? t("mount.unmount", { profile: profile.name }) : t("mount.mountTo", { profile: profile.name })}
      </Button>
    </div>
  );
}

function StatusChip({ children }: { children: string }) {
  return (
    <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-0.5 text-label-caps uppercase text-on-surface-variant">
      {children}
    </span>
  );
}

function ReadonlyRow({ href, label, mono = false, value }: { href?: string; label: string; mono?: boolean; value: string }) {
  const valueClassName = mono
    ? "mt-0.5 truncate font-mono text-body-sm text-on-surface"
    : "mt-0.5 truncate text-body-sm text-on-surface";

  return (
    <div className="min-w-0">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      {href ? (
        <a
          className={`${valueClassName} block text-primary hover:text-primary-strong hover:underline hover:decoration-primary/55 hover:underline-offset-2`}
          href={href}
          onClick={(event) => {
            event.preventDefault();
            void openExternalLink(href);
          }}
          rel="noreferrer"
          target="_blank"
          title={value}
        >
          {value}
        </a>
      ) : (
        <div className={valueClassName}>{value}</div>
      )}
    </div>
  );
}
