import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { groupMountStatusesByAssetId } from "../../utils/mountState";
import type { AssetViewMode } from "./AssetToolbar";
import { AssetGridView } from "./AssetGridView";
import { AssetRow } from "./AssetRow";

export function AssetList({
  assets,
  assetMountStatuses,
  sources,
  profiles,
  appShortcuts,
  expandedIds,
  viewMode,
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
  viewMode: AssetViewMode;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onRevealPath: (path: string) => void;
}) {
  const { t } = useI18n();
  const sourceById = new Map(sources.map((source) => [source.id, source]));
  const mountStatusesByAssetId = groupMountStatusesByAssetId(assetMountStatuses);

  if (viewMode === "grid") {
    return (
      <AssetGridView
        appShortcuts={appShortcuts}
        assets={assets}
        mountStatusesByAssetId={mountStatusesByAssetId}
        onRevealPath={onRevealPath}
        onToggleMount={onToggleMount}
        profiles={profiles}
        sourceById={sourceById}
      />
    );
  }

  return (
    <div
      className="asset-list-surface overflow-hidden rounded-xl border border-border shadow-[0_18px_42px_rgba(2,8,23,0.26)]"
      aria-label={t("asset.list.aria")}
    >
      {assets.map((asset) => {
        const mountStatuses = mountStatusesByAssetId.get(asset.id) ?? [];

        return (
          <AssetRow
            appShortcuts={appShortcuts}
            asset={asset}
            expanded={expandedIds.has(asset.id)}
            key={asset.id}
            onRevealPath={onRevealPath}
            onToggleExpanded={() => onToggleAsset(asset.id)}
            onToggleMount={(profileId) => onToggleMount(asset.id, profileId)}
            profiles={profiles}
            source={sourceById.get(asset.source_id)}
            mountStatuses={mountStatuses}
          />
        );
      })}
    </div>
  );
}
