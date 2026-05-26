import clsx from "clsx";
import { Pencil, Trash2 } from "lucide-react";
import { assetKindLabel } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, TargetProfile } from "../../types";
import { displayAssetPath } from "../../utils/path";
import { kindBadgeClass } from "../../utils/styles";
import { AssetMountPanel } from "./AssetMountPanel";
import { InlineMeta } from "./InlineMeta";
import { QuickMountButtons } from "./QuickMountButtons";

export function AssetRow({
  asset,
  profiles,
  appShortcuts,
  expanded,
  selectedProfileIds,
  onToggleExpanded,
  onToggleMount,
  onRevealPath,
}: {
  asset: Asset;
  profiles: TargetProfile[];
  appShortcuts: AppShortcut[];
  expanded: boolean;
  selectedProfileIds: string[];
  onToggleExpanded: () => void;
  onToggleMount: (profileId: string) => void;
  onRevealPath: (path: string) => void;
}) {
  const { t } = useI18n();

  return (
    <article
      className={clsx(
        "group cursor-pointer border-b border-border transition-colors last:border-b-0 hover:bg-surface-low",
        expanded && "asset-expanded bg-surface-low",
      )}
      onClick={onToggleExpanded}
    >
      <div className="relative flex min-h-28 items-start justify-between gap-4 px-4 py-3.5">
        <div className="min-w-0 flex-1 pr-80">
          <div className="flex items-center gap-2">
            <span className="font-mono text-code-md text-on-surface">{asset.name}</span>
            <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
            <span className="rounded-md bg-surface-highest px-2 py-0.5 text-[10px] font-bold text-on-surface-variant">
              {t("asset.origin.local")}
            </span>
          </div>
          <button
            className="asset-description mt-2 block max-w-full font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
            onClick={(event) => {
              event.stopPropagation();
              onRevealPath(asset.absolute_path);
            }}
            title={t("asset.revealPath")}
            type="button"
          >
            {displayAssetPath(asset)}
          </button>
          <div className="mt-3 flex min-w-0 items-start gap-4 max-[980px]:flex-col max-[980px]:gap-2">
            <InlineMeta label={t("asset.description")} value={asset.description ?? t("asset.noDescription")} />
            <InlineMeta label={t("asset.source")} value={asset.source_id} mono />
          </div>
        </div>
        <div className="absolute right-4 top-3.5 flex w-72 justify-end gap-3" onClick={(event) => event.stopPropagation()}>
          <QuickMountButtons
            asset={asset}
            profiles={profiles}
            shortcuts={appShortcuts}
            selectedProfileIds={selectedProfileIds}
            onToggle={onToggleMount}
          />
          <button className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-primary" aria-label={t("asset.edit")} type="button">
            <Pencil size={17} />
          </button>
          <button
            className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-status-remove"
            aria-label={t("asset.delete")}
            type="button"
          >
            <Trash2 size={17} />
          </button>
        </div>
      </div>

      {expanded && (
        <AssetMountPanel
          appShortcuts={appShortcuts}
          asset={asset}
          profiles={profiles}
          selectedProfileIds={selectedProfileIds}
          onToggle={onToggleMount}
        />
      )}
    </article>
  );
}
