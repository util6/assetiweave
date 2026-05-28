import { useEffect, useMemo, useState } from "react";
import { deleteSource as deleteSourceById, listSkillSources, revealPath, scanSkillSources, updateSource } from "../../services/catalog";
import type { Asset, Source } from "../../types";

export function useSourcesController(assets: Asset[], onCatalogRefresh?: (assets?: Asset[]) => Promise<void>) {
  const [sources, setSources] = useState<Source[]>([]);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void refreshSources();
  }, []);

  const assetCounts = useMemo(() => {
    return assets.reduce<Record<string, number>>((counts, asset) => {
      counts[asset.source_id] = (counts[asset.source_id] ?? 0) + 1;
      return counts;
    }, {});
  }, [assets]);

  const filteredSources = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return sources;
    }

    return sources.filter((source) => {
      const searchable = [
        source.name,
        source.root_path,
        source.kind,
        source.default_kind ?? "",
        source.last_scan_status ?? "",
        ...source.include_globs,
        ...source.exclude_globs,
      ]
        .join(" ")
        .toLowerCase();
      return searchable.includes(normalizedQuery);
    });
  }, [query, sources]);

  const summary = useMemo(() => {
    return {
      total: sources.length,
      enabled: sources.filter((source) => source.enabled).length,
      assets: Object.values(assetCounts).reduce((total, count) => total + count, 0),
      issues: sources.filter((source) => source.last_scan_status?.startsWith("error:")).length,
    };
  }, [assetCounts, sources]);

  async function refreshSources() {
    setSources(await listSkillSources());
  }

  async function toggleSource(source: Source) {
    setBusy(true);
    try {
      const saved = await updateSource({ ...source, enabled: !source.enabled });
      setSources((currentSources) => currentSources.map((candidate) => (candidate.id === saved.id ? saved : candidate)));
    } finally {
      setBusy(false);
    }
  }

  async function removeSource(source: Source, confirmMessage: string) {
    if (!window.confirm(confirmMessage)) {
      return;
    }

    setBusy(true);
    try {
      await deleteSourceById(source.id);
      setSources((currentSources) => currentSources.filter((candidate) => candidate.id !== source.id));
      await onCatalogRefresh?.();
    } finally {
      setBusy(false);
    }
  }

  async function scanAllSources() {
    setBusy(true);
    try {
      const scannedAssets = await scanSkillSources();
      await onCatalogRefresh?.(scannedAssets);
      await refreshSources();
    } finally {
      setBusy(false);
    }
  }

  return {
    assetCounts,
    busy,
    filteredSources,
    query,
    revealPath,
    removeSource,
    scanAllSources,
    setQuery,
    sources,
    summary,
    toggleSource,
  };
}
