import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { AssetRow } from "./AssetRow";

export function AssetList({
  assets,
  assetMountStatuses,
  sources,
  profiles,
  appShortcuts,
  expandedIds,
  selectedMounts,
  onToggleAsset,
  onToggleMount,
  onRevealPath,
}: {
  assets: Asset[];
  assetMountStatuses: AssetMountStatus[];
  sources: Source[];
  profiles: TargetProfile[];
  appShortcuts: AppShortcut[];
  expandedIds: Set<string>;
  selectedMounts: Record<string, string[]>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onRevealPath: (path: string) => void;
}) {
  const { t } = useI18n();
  const sourceById = new Map(sources.map((source) => [source.id, source]));
  const mountStatusesByAssetId = assetMountStatuses.reduce<Map<string, AssetMountStatus[]>>((grouped, status) => {
    grouped.set(status.asset_id, [...(grouped.get(status.asset_id) ?? []), status]);
    return grouped;
  }, new Map());

  return (
    <div
      className="asset-list-surface overflow-hidden rounded-xl border border-border shadow-[0_18px_42px_rgba(2,8,23,0.26)]"
      aria-label={t("asset.list.aria")}
    >
      {assets.map((asset) => (
        <AssetRow
          appShortcuts={appShortcuts}
          asset={asset}
          expanded={expandedIds.has(asset.id)}
          key={asset.id}
          onRevealPath={onRevealPath}
          onToggleExpanded={() => onToggleAsset(asset.id)}
          onToggleMount={(profileId) => onToggleMount(asset.id, profileId)}
          profiles={profiles}
          selectedProfileIds={selectedMounts[asset.id] ?? []}
          source={sourceById.get(asset.source_id)}
          mountStatuses={mountStatusesByAssetId.get(asset.id) ?? []}
        />
      ))}
    </div>
  );
}
