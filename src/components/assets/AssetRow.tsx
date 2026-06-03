import clsx from "clsx";
import { Pencil, Trash2 } from "lucide-react";
import { assetKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { getAssetMountSummaryState } from "../../utils/mountState";
import { isDirectMountBlockedSource } from "../../utils/mountPolicy";
import { displayAssetPath } from "../../utils/path";
import { kindBadgeClass } from "../../utils/styles";
import { AssetMountPanel } from "./AssetMountPanel";
import { InlineMeta } from "./InlineMeta";
import { MountStatePill } from "./MountStatePill";
import { QuickMountButtons } from "./QuickMountButtons";

export function AssetRow({
  asset,
  source,
  mountStatuses,
  profiles,
  appShortcuts,
  expanded,
  onToggleExpanded,
  onToggleMount,
  onRevealPath,
  onEdit,
  onDelete,
}: {
  asset: Asset;
  source?: Source;
  mountStatuses: AssetMountStatus[];
  profiles: TargetProfile[];
  appShortcuts: AppShortcut[];
  expanded: boolean;
  onToggleExpanded: () => void;
  onToggleMount: (profileId: string) => void;
  onRevealPath: (path: string) => void;
  onEdit?: () => void;
  onDelete?: () => void;
}) {
  const { t } = useI18n();
  const mountBlockedReason = isDirectMountBlockedSource(source) ? t("mount.blocked") : undefined;
  const mountSummaryState = getAssetMountSummaryState(mountStatuses);

  return (
    <article
      className={clsx(
        "group cursor-pointer border-b border-theme-card-border/80 transition-all last:border-b-0 hover:bg-theme-card-header/70",
        expanded && "asset-expanded bg-theme-card-header shadow-[inset_3px_0_0_rgb(var(--theme-nav-indicator)/0.62)]",
      )}
      onClick={onToggleExpanded}
    >
      <div className="grid min-h-[116px] grid-cols-[minmax(0,1fr)_auto] items-start gap-5 px-5 py-4 max-[980px]:grid-cols-1 max-[980px]:gap-3">
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <span className="min-w-0 max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[14px] font-semibold leading-5 text-on-surface">
              {asset.name}
            </span>
            <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
            <MountStatePill state={mountSummaryState} />
            <span className="rounded-md border border-theme-control-border bg-theme-control-hover/70 px-2 py-0.5 text-[10px] font-bold text-on-surface-variant">
              {t("asset.origin.local")}
            </span>
          </div>
          <button
            className="asset-description mt-2 block max-w-full font-mono text-body-sm text-on-surface-variant/80 transition-colors hover:text-primary"
            onClick={(event) => {
              event.stopPropagation();
              onRevealPath(asset.absolute_path);
            }}
            title={t("asset.revealPath")}
            type="button"
          >
            {displayAssetPath(asset)}
          </button>
          <div className="mt-3 flex min-w-0 items-start gap-5 max-[980px]:flex-col max-[980px]:gap-2">
            <InlineMeta label={t("asset.description")} value={asset.description ?? t("asset.noDescription")} />
            <InlineMeta label={t("asset.source")} value={asset.source_id} mono />
          </div>
        </div>
        <div
          className="flex w-[292px] shrink-0 items-center justify-end gap-2 rounded-xl border border-theme-control-border bg-theme-control/55 p-1.5 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.38)] max-[980px]:w-full max-[980px]:justify-start"
          onClick={(event) => event.stopPropagation()}
        >
          <QuickMountButtons
            asset={asset}
            mountBlockedReason={mountBlockedReason}
            mountStatuses={mountStatuses}
            profiles={profiles}
            shortcuts={appShortcuts}
            onToggle={onToggleMount}
          />
          {(onEdit || onDelete) && <span className="h-6 w-px bg-theme-control-border/80" aria-hidden="true" />}
          {onEdit && (
            <button
              className="grid size-8 place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-primary"
              aria-label={t("asset.edit")}
              onClick={onEdit}
              type="button"
            >
              <Pencil size={17} />
            </button>
          )}
          {onDelete && (
            <button
              className="grid size-8 place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-status-remove"
              aria-label={t("asset.delete")}
              onClick={onDelete}
              type="button"
            >
              <Trash2 size={17} />
            </button>
          )}
        </div>
      </div>

      {expanded && (
        <AssetMountPanel
          appShortcuts={appShortcuts}
          asset={asset}
          mountBlockedReason={mountBlockedReason}
          mountStatuses={mountStatuses}
          profiles={profiles}
          onToggle={onToggleMount}
        />
      )}
    </article>
  );
}
