import { useMemo, useState } from "react";
import type { AppShortcut, Asset, AssetMountStatus, Source, TargetProfile } from "../../types";
import { groupMountStatusesByAssetId } from "../../utils/mountState";
import { SourceColumnView } from "./SourceColumnView";
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
  onSetSourceMountProfile,
  onToggleAsset,
  onToggleMount,
  onToggle,
  profiles,
  sources,
  viewMode,
}: {
  appShortcuts: AppShortcut[];
  assetMountStatuses: AssetMountStatus[];
  assets: Asset[];
  busy: boolean;
  expandedAssetIds: Set<string>;
  onDelete: (source: Source) => void;
  onAssetReveal: (path: string) => void;
  onReveal: (path: string) => void;
  onSetSourceMountProfile: (assetIds: string[], profileId: string, enabled: boolean) => void;
  onToggleAsset: (assetId: string) => void;
  onToggleMount: (assetId: string, profileId: string) => void;
  onToggle: (source: Source) => void;
  profiles: TargetProfile[];
  sources: Source[];
  viewMode: "list" | "columns";
}) {
  const { t } = useI18n();
  const [expandedSourceIds, setExpandedSourceIds] = useState<Set<string>>(new Set());
  const [selectedSourceId, setSelectedSourceId] = useState<string | null>(null);
  const mountStatusesByAssetId = groupMountStatusesByAssetId(assetMountStatuses);
  const assetsBySourceId = useMemo(() => {
    return assets.reduce<Map<string, Asset[]>>((grouped, asset) => {
      if (asset.kind !== "skill") {
        return grouped;
      }

      grouped.set(asset.source_id, [...(grouped.get(asset.source_id) ?? []), asset]);
      return grouped;
    }, new Map());
  }, [assets]);

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

  if (viewMode === "columns") {
    const selectedSource = sources.find((source) => source.id === selectedSourceId) ?? sources[0]!;

    return (
      <SourceColumnView
        appShortcuts={appShortcuts}
        assetsBySourceId={assetsBySourceId}
        busy={busy}
        mountStatusesByAssetId={mountStatusesByAssetId}
        onAssetReveal={onAssetReveal}
        onReveal={onReveal}
        onSelectSource={setSelectedSourceId}
        onSetSourceMountProfile={onSetSourceMountProfile}
        onToggleMount={onToggleMount}
        profiles={profiles}
        selectedSource={selectedSource}
        sources={sources}
      />
    );
  }

  return (
    <div className="overflow-hidden rounded-xl border border-border bg-surface-card/60" aria-label={t("source.page.title")}>
      {sources.map((source) => (
        <SourceRow
          appShortcuts={appShortcuts}
          assets={assetsBySourceId.get(source.id) ?? []}
          mountStatusesByAssetId={mountStatusesByAssetId}
          busy={busy}
          expanded={expandedSourceIds.has(source.id)}
          expandedAssetIds={expandedAssetIds}
          key={source.id}
          onDelete={() => onDelete(source)}
          onAssetReveal={onAssetReveal}
          onReveal={() => onReveal(source.root_path)}
          onSetSourceMountProfile={onSetSourceMountProfile}
          onToggleAsset={onToggleAsset}
          onToggleExpanded={() => toggleSourceExpanded(source.id)}
          onToggleMount={onToggleMount}
          onToggle={() => onToggle(source)}
          profiles={profiles}
          source={source}
        />
      ))}
    </div>
  );
}
