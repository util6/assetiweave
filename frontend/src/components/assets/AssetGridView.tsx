import clsx from "clsx";
import { FolderOpen, Pencil, Trash2 } from "lucide-react";
import { assetKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { getAssetMountSummaryState, getMountedProfileIds } from "../../utils/mountState";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { displayAssetPath } from "../../utils/path";
import { assetSourceHref, assetSourceLabel } from "../../utils/assetSource";
import { openExternalLink } from "../../utils/externalLinks";
import { kindBadgeClass } from "../../utils/styles";
import { MountStatePill } from "./MountStatePill";
import { QuickMountButtons } from "./QuickMountButtons";
import { SkillBackupBadge } from "./SkillBackupBadge";

export function AssetGridView({
  appShortcuts,
  assets,
  mountStatusesByAssetId,
  onDeleteAsset,
  onEditAsset,
  onRevealPath,
  onToggleMount,
  profiles,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onDeleteAsset: (asset: Asset) => void;
  onEditAsset: (asset: Asset) => void;
  onRevealPath: (path: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();

  if (assets.length === 0) {
    return (
      <div className="rounded-xl border border-theme-card-border bg-theme-card/70 px-4 py-10 text-center text-body-md text-on-surface-variant">
        {t("asset.empty")}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4" aria-label={t("asset.grid.aria")}>
      {assets.map((asset) => {
        const source = sourceById.get(asset.source_id);
        const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];
        const mountedProfileIds = getMountedProfileIds(mountStatuses);
        const mountSummaryState = getAssetMountSummaryState(mountStatuses);
        const mountBlockedReason = isDirectMountBlockedSource(source) ? t("mount.blocked") : undefined;
        const sourceLabel = assetSourceLabel(asset, source);
        const sourceHref = assetSourceHref(asset);

        return (
          <article
            className="group flex min-h-[236px] flex-col rounded-xl border border-theme-card-border bg-theme-card/78 p-4 shadow-[0_18px_42px_rgb(var(--theme-panel-shadow)/0.16)] transition-colors hover:border-theme-nav-active-border hover:bg-theme-card"
            key={asset.id}
          >
            <div className="flex min-w-0 items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="min-w-0 max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[14px] font-semibold leading-5 text-on-surface">
                    {asset.name}
                  </span>
                  <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
                  <SkillBackupBadge asset={asset} />
                  <MountStatePill compact state={mountSummaryState} />
                </div>
                <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-outline">
                  {sourceHref ? (
                    <a
                      className="text-primary hover:text-primary-strong hover:underline hover:decoration-primary/55 hover:underline-offset-2"
                      href={sourceHref}
                      onClick={(event) => {
                        event.preventDefault();
                        void openExternalLink(sourceHref);
                      }}
                      rel="noreferrer"
                      target="_blank"
                      title={sourceLabel}
                    >
                      {sourceLabel}
                    </a>
                  ) : (
                    sourceLabel
                  )}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-1.5">
                <GridIconButton label={t("asset.revealPath")} onClick={() => onRevealPath(asset.absolute_path)}>
                  <FolderOpen size={16} />
                </GridIconButton>
                <GridIconButton label={t("asset.edit")} onClick={() => onEditAsset(asset)}>
                  <Pencil size={16} />
                </GridIconButton>
                <GridIconButton danger label={t("asset.delete")} onClick={() => onDeleteAsset(asset)}>
                  <Trash2 size={16} />
                </GridIconButton>
              </div>
            </div>

            <p className="mt-4 line-clamp-3 min-h-12 text-body-sm text-on-surface-variant">
              {asset.description ?? t("asset.noDescription")}
            </p>

            <button
              className="mt-3 block max-w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-lg border border-theme-control-border bg-theme-control/55 px-3 py-2 text-left font-mono text-body-sm text-on-surface-variant transition-colors hover:border-theme-nav-active-border hover:text-primary"
              onClick={() => onRevealPath(asset.absolute_path)}
              title={t("asset.revealPath")}
              type="button"
            >
              {displayAssetPath(asset)}
            </button>

            <div className="mt-auto flex min-w-0 items-center justify-between gap-3 pt-4">
              <span
                className={clsx(
                  "inline-flex min-w-0 items-center gap-1.5 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm",
                  mountBlockedReason ? "text-status-conflict" : mountSummaryTextClass(mountSummaryState),
                )}
              >
                <span
                  className={clsx(
                    "size-1.5 shrink-0 rounded-full",
                    mountBlockedReason ? "bg-status-conflict" : mountSummaryDotClass(mountSummaryState),
                  )}
                  aria-hidden="true"
                />
                {mountBlockedReason ?? t("mount.selected", { count: mountedProfileIds.length })}
              </span>
              <div className="shrink-0 rounded-xl border border-theme-control-border bg-theme-control/55 p-1.5">
                <QuickMountButtons
                  asset={asset}
                  mountBlockedReason={mountBlockedReason}
                  mountStatuses={mountStatuses}
                  profiles={profiles}
                  shortcuts={appShortcuts}
                  onToggle={(profileId) => onToggleMount(asset.id, profileId)}
                />
              </div>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function GridIconButton({
  children,
  danger = false,
  label,
  onClick,
}: {
  children: React.ReactNode;
  danger?: boolean;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "grid size-8 place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-primary",
        danger && "hover:text-status-remove",
      )}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}

function mountSummaryTextClass(state: ReturnType<typeof getAssetMountSummaryState>) {
  if (state === "mounted") return "text-status-create";
  if (state === "conflict" || state === "broken") return "text-status-remove";
  return "text-on-surface-variant";
}

function mountSummaryDotClass(state: ReturnType<typeof getAssetMountSummaryState>) {
  if (state === "mounted") return "bg-status-create";
  if (state === "conflict" || state === "broken") return "bg-status-remove";
  return "bg-outline";
}
