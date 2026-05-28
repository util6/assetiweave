import { useState } from "react";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { SourceRow } from "./SourceRow";
import { useI18n } from "../../i18n/I18nProvider";

export function SourceList({
  appShortcuts,
  assetMountStatuses,
  assets,
  busy,
  expandedAssetIds,
  onDelete,
  onAssetReveal,
  onReveal,
  onToggleAsset,
  onToggleMount,
  onToggle,
  profiles,
  selectedMounts,
  sources,
}: {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  busy: boolean;
  expandedAssetIds: Set<string>;
  onDelete: (source: Source) => void;
  onAssetReveal: (path: string) => void;
  onReveal: (path: string) => void;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onToggle: (source: Source) => void;
  profiles: TargetProfile[];
  selectedMounts: Record<string, string[]>;
  sources: Source[];
}) {
  const { t } = useI18n();
  const [expandedSourceIds, setExpandedSourceIds] = useState<Set<string>>(new Set());
  const mountStatusesByAssetId = assetMountStatuses.reduce<Map<string, AssetMountStatus[]>>((grouped, status) => {
    grouped.set(status.asset_id, [...(grouped.get(status.asset_id) ?? []), status]);
    return grouped;
  }, new Map());

  function toggleSourceExpanded(sourceId: string) {
    setExpandedSourceIds((current) => {
      const next = new Set(current);
      if (next.has(sourceId)) {
        next.delete(sourceId);
      } else {
        next.add(sourceId);
      }
      return next;
    });
  }

  if (sources.length === 0) {
    return (
      <div className="rounded-xl border border-border bg-surface-card/60 px-4 py-10 text-center text-body-md text-on-surface-variant">
        {t("source.empty")}
      </div>
    );
  }

  return (
    <div className="overflow-hidden rounded-xl border border-border bg-surface-card/60" aria-label={t("source.page.title")}>
      {sources.map((source) => (
        <SourceRow
          appShortcuts={appShortcuts}
          assets={assets.filter((asset) => asset.source_id === source.id && asset.kind === "skill")}
          mountStatusesByAssetId={mountStatusesByAssetId}
          busy={busy}
          expanded={expandedSourceIds.has(source.id)}
          expandedAssetIds={expandedAssetIds}
          key={source.id}
          onDelete={() => onDelete(source)}
          onAssetReveal={onAssetReveal}
          onReveal={() => onReveal(source.root_path)}
          onToggleAsset={onToggleAsset}
          onToggleExpanded={() => toggleSourceExpanded(source.id)}
          onToggleMount={onToggleMount}
          onToggle={() => onToggle(source)}
          profiles={profiles}
          selectedMounts={selectedMounts}
          source={source}
        />
      ))}
    </div>
  );
}
