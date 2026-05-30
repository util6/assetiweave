import clsx from "clsx";
import { FolderOpen, Pencil, Trash2 } from "lucide-react";
import { assetKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { displayAssetPath } from "../../utils/path";
import { kindBadgeClass } from "../../utils/styles";
import { QuickMountButtons } from "./QuickMountButtons";

export function AssetGridView({
  appShortcuts,
  assets,
  mountStatusesByAssetId,
  onRevealPath,
  onToggleMount,
  profiles,
  selectedMounts,
  sourceById,
}: {
  appShortcuts: AppShortcut[];
  assets: Asset[];
  mountStatusesByAssetId: Map<string, AssetMountStatus[]>;
  onRevealPath: (path: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  profiles: TargetProfile[];
  selectedMounts: Record<string, string[]>;
  sourceById: Map<string, Source>;
}) {
  const { t } = useI18n();

  if (assets.length === 0) {
    return (
      <div className="rounded-xl border border-border bg-surface-card/60 px-4 py-10 text-center text-body-md text-on-surface-variant">
        {t("asset.empty")}
      </div>
    );
  }

  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4" aria-label={t("asset.grid.aria")}>
      {assets.map((asset) => {
        const source = sourceById.get(asset.source_id);
        const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];
        const selectedProfileIds = mergeProfileIds(
          selectedMounts[asset.id] ?? [],
          mountStatuses.filter((status) => status.state === "mounted").map((status) => status.profile_id),
        );
        const mountBlockedReason = isDirectMountBlockedSource(source) ? t("mount.blocked") : undefined;

        return (
          <article
            className="group flex min-h-[236px] flex-col rounded-xl border border-border bg-surface-card/70 p-4 shadow-[0_18px_42px_rgba(2,8,23,0.2)] transition-colors hover:border-outline-variant hover:bg-surface-card/90"
            key={asset.id}
          >
            <div className="flex min-w-0 items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex min-w-0 flex-wrap items-center gap-2">
                  <span className="min-w-0 max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[14px] font-semibold leading-5 text-on-surface">
                    {asset.name}
                  </span>
                  <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
                </div>
                <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-outline">
                  {source?.name ?? asset.source_id}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-1.5">
                <GridIconButton label={t("asset.revealPath")} onClick={() => onRevealPath(asset.absolute_path)}>
                  <FolderOpen size={16} />
                </GridIconButton>
                <GridIconButton label={t("asset.edit")}>
                  <Pencil size={16} />
                </GridIconButton>
                <GridIconButton danger label={t("asset.delete")}>
                  <Trash2 size={16} />
                </GridIconButton>
              </div>
            </div>

            <p className="mt-4 line-clamp-3 min-h-12 text-body-sm text-on-surface-variant">
              {asset.description ?? t("asset.noDescription")}
            </p>

            <button
              className="mt-3 block max-w-full overflow-hidden text-ellipsis whitespace-nowrap rounded-lg border border-border/70 bg-surface-lowest/35 px-3 py-2 text-left font-mono text-body-sm text-on-surface-variant transition-colors hover:border-outline-variant hover:text-primary"
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
                  mountBlockedReason ? "text-status-conflict" : selectedProfileIds.length > 0 ? "text-status-create" : "text-on-surface-variant",
                )}
              >
                <span
                  className={clsx(
                    "size-1.5 shrink-0 rounded-full",
                    mountBlockedReason ? "bg-status-conflict" : selectedProfileIds.length > 0 ? "bg-status-create" : "bg-outline",
                  )}
                  aria-hidden="true"
                />
                {mountBlockedReason ?? t("mount.selected", { count: selectedProfileIds.length })}
              </span>
              <div className="shrink-0 rounded-xl border border-border/70 bg-surface-lowest/35 p-1.5">
                <QuickMountButtons
                  asset={asset}
                  mountBlockedReason={mountBlockedReason}
                  profiles={profiles}
                  shortcuts={appShortcuts}
                  selectedProfileIds={selectedProfileIds}
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
        "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-primary",
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

function mergeProfileIds(...groups: string[][]) {
  return [...new Set(groups.flat())];
}
