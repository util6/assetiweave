import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, TargetProfile } from "../../types";
import { AssetRow } from "./AssetRow";

export function AssetList({
  assets,
  profiles,
  appShortcuts,
  expandedIds,
  selectedMounts,
  onToggleAsset,
  onToggleMount,
  onRevealPath,
}: {
  assets: Asset[];
  profiles: TargetProfile[];
  appShortcuts: AppShortcut[];
  expandedIds: Set<string>;
  selectedMounts: Record<string, string[]>;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onRevealPath: (path: string) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="glass-card overflow-hidden rounded-xl border border-border" aria-label={t("asset.list.aria")}>
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
        />
      ))}
    </div>
  );
}
